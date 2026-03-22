use rusqlite::{params, Connection, OptionalExtension};

use crate::error::StorageError;

/// Metadata for one trail entry's position in storage.
#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub sequence_number: u64,
    pub timestamp_bytes: [u8; 10],
    pub workspace_id: Option<String>,
    pub actor: String,
    pub event_type: String,
    pub segment_id: u64,
    pub offset: u64,
    pub length: u32,
}

/// Query filters for the trail index.
#[derive(Debug, Clone, Default)]
pub struct TrailQuery {
    pub workspace_id: Option<String>,
    pub event_type: Option<String>,
    pub actor: Option<String>,
    pub from_timestamp: Option<[u8; 10]>,
    pub to_timestamp: Option<[u8; 10]>,
    pub limit: u32,
}

impl TrailQuery {
    pub fn new() -> Self {
        Self {
            limit: 100,
            ..Default::default()
        }
    }
}

/// Result of a trail query.
#[derive(Debug)]
pub struct QueryResult {
    pub entries: Vec<IndexEntry>,
    pub has_more: bool,
}

/// SQLite-backed trail index (storage spec §4).
pub struct TrailIndex {
    conn: Connection,
}

impl TrailIndex {
    /// Open or create a file-backed index.
    pub fn open(path: &std::path::Path) -> Result<Self, StorageError> {
        let conn = Connection::open(path)?;
        let idx = Self { conn };
        idx.create_tables()?;
        Ok(idx)
    }

    /// Open an in-memory index for testing.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()?;
        let idx = Self { conn };
        idx.create_tables()?;
        Ok(idx)
    }

    fn create_tables(&self) -> Result<(), StorageError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS trail_index (
                sequence_number INTEGER PRIMARY KEY,
                timestamp       BLOB NOT NULL,
                workspace_id    TEXT,
                actor           TEXT NOT NULL,
                event_type      TEXT NOT NULL,
                segment_id      INTEGER NOT NULL,
                offset          INTEGER NOT NULL,
                length          INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_workspace ON trail_index(workspace_id, sequence_number);
            CREATE INDEX IF NOT EXISTS idx_event_type ON trail_index(event_type, sequence_number);
            CREATE INDEX IF NOT EXISTS idx_timestamp ON trail_index(timestamp);
            CREATE INDEX IF NOT EXISTS idx_actor ON trail_index(actor, sequence_number);",
        )?;
        Ok(())
    }

    /// Insert a single entry.
    pub fn insert(&self, entry: &IndexEntry) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO trail_index (sequence_number, timestamp, workspace_id, actor, event_type, segment_id, offset, length)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                entry.sequence_number as i64,
                &entry.timestamp_bytes[..],
                entry.workspace_id,
                entry.actor,
                entry.event_type,
                entry.segment_id as i64,
                entry.offset as i64,
                entry.length as i64,
            ],
        )?;
        Ok(())
    }

    /// Insert multiple entries in one transaction.
    pub fn insert_batch(&self, entries: &[IndexEntry]) -> Result<(), StorageError> {
        let tx = self.conn.unchecked_transaction()?;
        for entry in entries {
            tx.execute(
                "INSERT INTO trail_index (sequence_number, timestamp, workspace_id, actor, event_type, segment_id, offset, length)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    entry.sequence_number as i64,
                    &entry.timestamp_bytes[..],
                    entry.workspace_id,
                    entry.actor,
                    entry.event_type,
                    entry.segment_id as i64,
                    entry.offset as i64,
                    entry.length as i64,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Execute a query with filters.
    pub fn query(&self, q: &TrailQuery) -> Result<QueryResult, StorageError> {
        let limit = if q.limit == 0 { 100 } else { q.limit };
        // Fetch one extra to detect has_more.
        let fetch_limit = limit + 1;

        let mut sql = String::from(
            "SELECT sequence_number, timestamp, workspace_id, actor, event_type, segment_id, offset, length FROM trail_index WHERE 1=1",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(ref ws) = q.workspace_id {
            sql.push_str(" AND workspace_id = ?");
            param_values.push(Box::new(ws.clone()));
        }
        if let Some(ref et) = q.event_type {
            sql.push_str(" AND event_type = ?");
            param_values.push(Box::new(et.clone()));
        }
        if let Some(ref actor) = q.actor {
            sql.push_str(" AND actor = ?");
            param_values.push(Box::new(actor.clone()));
        }
        if let Some(ref from) = q.from_timestamp {
            sql.push_str(" AND timestamp >= ?");
            param_values.push(Box::new(from.to_vec()));
        }
        if let Some(ref to) = q.to_timestamp {
            sql.push_str(" AND timestamp <= ?");
            param_values.push(Box::new(to.to_vec()));
        }

        sql.push_str(" ORDER BY sequence_number ASC LIMIT ?");
        param_values.push(Box::new(fetch_limit as i64));

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            let seq: i64 = row.get(0)?;
            let ts_blob: Vec<u8> = row.get(1)?;
            let ws: Option<String> = row.get(2)?;
            let actor: String = row.get(3)?;
            let event_type: String = row.get(4)?;
            let seg: i64 = row.get(5)?;
            let off: i64 = row.get(6)?;
            let len: i64 = row.get(7)?;

            let mut ts = [0u8; 10];
            if ts_blob.len() == 10 {
                ts.copy_from_slice(&ts_blob);
            }

            Ok(IndexEntry {
                sequence_number: seq as u64,
                timestamp_bytes: ts,
                workspace_id: ws,
                actor,
                event_type,
                segment_id: seg as u64,
                offset: off as u64,
                length: len as u32,
            })
        })?;

        let mut entries: Vec<IndexEntry> = Vec::new();
        for row in rows {
            entries.push(row?);
        }

        let has_more = entries.len() > limit as usize;
        if has_more {
            entries.truncate(limit as usize);
        }

        Ok(QueryResult { entries, has_more })
    }

    /// Highest sequence number in the index.
    pub fn last_sequence(&self) -> Result<Option<u64>, StorageError> {
        let result: Option<i64> = self
            .conn
            .query_row(
                "SELECT MAX(sequence_number) FROM trail_index",
                [],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        Ok(result.map(|v| v as u64))
    }
}
