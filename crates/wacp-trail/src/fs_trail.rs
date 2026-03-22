use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::chain::ChainHash;
use crate::error::StorageError;
use crate::traits::*;

/// Configuration for the filesystem trail backend.
pub struct FileTrailConfig {
    pub dir: PathBuf,
    pub max_segment_size: u64,
}

impl Default for FileTrailConfig {
    fn default() -> Self {
        Self {
            dir: PathBuf::from("trail"),
            max_segment_size: 64 * 1024 * 1024, // 64 MB
        }
    }
}

/// Production trail storage: append-only segment files with per-entry fsync.
pub struct FileTrailStorage {
    dir: PathBuf,
    active_segment: SegmentId,
    active_file: File,
    active_size: u64,
    sequence_counter: u64,
    chain_head: ChainHash,
    max_segment_size: u64,
}

const ENTRY_OVERHEAD: u64 = 4 + 32; // length prefix + chain hash

impl FileTrailStorage {
    /// Open an existing trail or create a new one.
    pub fn open(config: FileTrailConfig) -> Result<Self, StorageError> {
        fs::create_dir_all(&config.dir)?;

        // Find existing segments.
        let mut seg_ids = list_segments(&config.dir)?;
        seg_ids.sort();

        if seg_ids.is_empty() {
            // Fresh trail.
            let seg_id = SegmentId(0);
            let file = create_segment_file(&config.dir, seg_id)?;
            return Ok(Self {
                dir: config.dir,
                active_segment: seg_id,
                active_file: file,
                active_size: 0,
                sequence_counter: 0,
                chain_head: ChainHash::GENESIS,
                max_segment_size: config.max_segment_size,
            });
        }

        // Resume from existing trail — scan all segments to reconstruct state.
        let mut sequence_counter = 0u64;
        let mut chain_head = ChainHash::GENESIS;
        for &seg_id in &seg_ids {
            let entries = scan_segment_file(&config.dir, SegmentId(seg_id))?;
            for (_, _, hash) in &entries {
                sequence_counter += 1;
                chain_head = ChainHash(*hash);
            }
        }

        let active_seg = SegmentId(*seg_ids.last().unwrap());
        let path = segment_path(&config.dir, active_seg);
        let active_size = fs::metadata(&path)?.len();
        let file = OpenOptions::new().append(true).open(&path)?;

        Ok(Self {
            dir: config.dir,
            active_segment: active_seg,
            active_file: file,
            active_size,
            sequence_counter,
            chain_head,
            max_segment_size: config.max_segment_size,
        })
    }

    /// Open with crash recovery — truncate partial tail entry.
    pub fn recover(config: FileTrailConfig) -> Result<Self, StorageError> {
        fs::create_dir_all(&config.dir)?;

        let mut seg_ids = list_segments(&config.dir)?;
        seg_ids.sort();

        if seg_ids.is_empty() {
            return Self::open(config);
        }

        // Validate and possibly truncate the active (last) segment.
        let active_seg_id = *seg_ids.last().unwrap();
        let path = segment_path(&config.dir, SegmentId(active_seg_id));
        truncate_partial_tail(&path)?;

        Self::open(config)
    }
}

impl TrailStorage for FileTrailStorage {
    fn append(
        &mut self,
        entry: &[u8],
        chain_hash: &[u8; 32],
    ) -> Result<WriteReceipt, StorageError> {
        let offset = self.active_size;
        let len = entry.len() as u32;

        // Write: [4-byte length BE][entry][32-byte hash]
        self.active_file.write_all(&len.to_be_bytes())?;
        self.active_file.write_all(entry)?;
        self.active_file.write_all(chain_hash)?;
        self.active_file.sync_all()?;

        self.active_size += ENTRY_OVERHEAD + entry.len() as u64;
        self.sequence_counter += 1;
        self.chain_head = ChainHash(*chain_hash);

        let receipt = WriteReceipt {
            segment: self.active_segment,
            offset,
            sequence: self.sequence_counter,
        };

        // Rotate if needed.
        if self.active_size >= self.max_segment_size {
            self.rotate()?;
        }

        Ok(receipt)
    }

    fn read(
        &self,
        segment: SegmentId,
        offset: u64,
        _length: u32,
    ) -> Result<Vec<u8>, StorageError> {
        let path = segment_path(&self.dir, segment);
        let mut file = File::open(&path).map_err(|_| StorageError::SegmentNotFound(segment.0))?;
        file.seek(SeekFrom::Start(offset))?;

        let mut len_buf = [0u8; 4];
        file.read_exact(&mut len_buf)?;
        let len = u32::from_be_bytes(len_buf) as usize;

        let mut entry = vec![0u8; len];
        file.read_exact(&mut entry)?;
        // Skip hash (32 bytes) — caller doesn't need it from read.

        Ok(entry)
    }

    fn rotate(&mut self) -> Result<SegmentId, StorageError> {
        let new_id = SegmentId(self.active_segment.0 + 1);
        let file = create_segment_file(&self.dir, new_id)?;
        self.active_segment = new_id;
        self.active_file = file;
        self.active_size = 0;
        Ok(new_id)
    }

    fn scan(
        &self,
        segment: SegmentId,
        from_offset: u64,
    ) -> Result<Vec<ScannedEntry>, StorageError> {
        scan_segment_file_from(&self.dir, segment, from_offset)
    }

    fn metadata(&self) -> StorageMetadata {
        StorageMetadata {
            current_segment: self.active_segment,
            total_entries: self.sequence_counter,
            chain_head: self.chain_head.0,
            total_bytes: self.active_size, // approximate — doesn't sum sealed segments
        }
    }
}

// ── Helpers ──

fn segment_path(dir: &Path, id: SegmentId) -> PathBuf {
    dir.join(format!("segment-{:06}.trail", id.0))
}

fn create_segment_file(dir: &Path, id: SegmentId) -> Result<File, StorageError> {
    let path = segment_path(dir, id);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .truncate(false)
        .open(path)?;
    Ok(file)
}

fn list_segments(dir: &Path) -> Result<Vec<u64>, StorageError> {
    let mut ids = Vec::new();
    if !dir.exists() {
        return Ok(ids);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(rest) = name.strip_prefix("segment-")
            && let Some(num_str) = rest.strip_suffix(".trail")
                && let Ok(n) = num_str.parse::<u64>() {
                    ids.push(n);
                }
    }
    Ok(ids)
}

fn scan_segment_file(dir: &Path, id: SegmentId) -> Result<Vec<ScannedEntry>, StorageError> {
    scan_segment_file_from(dir, id, 0)
}

fn scan_segment_file_from(
    dir: &Path,
    id: SegmentId,
    from_offset: u64,
) -> Result<Vec<ScannedEntry>, StorageError> {
    let path = segment_path(dir, id);
    let mut file =
        File::open(&path).map_err(|_| StorageError::SegmentNotFound(id.0))?;
    let file_len = file.metadata()?.len();
    file.seek(SeekFrom::Start(from_offset))?;

    let mut results = Vec::new();
    let mut pos = from_offset;

    while pos + 4 <= file_len {
        let mut len_buf = [0u8; 4];
        if file.read_exact(&mut len_buf).is_err() {
            break;
        }
        let len = u32::from_be_bytes(len_buf) as usize;

        if pos + 4 + len as u64 + 32 > file_len {
            break; // partial entry
        }

        let mut entry = vec![0u8; len];
        if file.read_exact(&mut entry).is_err() {
            break;
        }

        let mut hash = [0u8; 32];
        if file.read_exact(&mut hash).is_err() {
            break;
        }

        results.push((pos, entry, hash));
        pos += 4 + len as u64 + 32;
    }

    Ok(results)
}

/// Truncate partial entry at the tail of a segment file.
fn truncate_partial_tail(path: &Path) -> Result<(), StorageError> {
    if !path.exists() {
        return Ok(());
    }

    let entries = {
        let file = File::open(path)?;
        let file_len = file.metadata()?.len();
        // Scan and find last valid position.
        let mut f = file;
        let mut pos = 0u64;
        let mut last_valid_end = 0u64;

        while pos + 4 <= file_len {
            let mut len_buf = [0u8; 4];
            f.seek(SeekFrom::Start(pos))?;
            if f.read_exact(&mut len_buf).is_err() {
                break;
            }
            let len = u32::from_be_bytes(len_buf) as usize;
            let entry_end = pos + 4 + len as u64 + 32;

            if entry_end > file_len {
                break; // partial
            }

            // Skip over entry + hash to validate it's complete.
            last_valid_end = entry_end;
            pos = entry_end;
        }

        (file_len, last_valid_end)
    };

    let (file_len, last_valid_end) = entries;
    if last_valid_end < file_len {
        // Truncate the partial tail.
        let file = OpenOptions::new().write(true).open(path)?;
        file.set_len(last_valid_end)?;
        file.sync_all()?;
    }

    Ok(())
}
