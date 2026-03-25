use std::process::Command;

fn main() {
    // Git commit hash.
    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_else(|| "unknown".into());
    println!(
        "cargo:rustc-env=WACP_GIT_COMMIT={}",
        commit.trim()
    );

    // Build timestamp (UTC).
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| {
            let secs = d.as_secs();
            // Simple UTC date-time: YYYY-MM-DD HH:MM:SS UTC
            let days = secs / 86400;
            let time = secs % 86400;
            let h = time / 3600;
            let m = (time % 3600) / 60;
            let s = time % 60;
            // Approximate date from epoch days (good enough for build stamps)
            let (y, mo, day) = epoch_days_to_ymd(days);
            format!("{y:04}-{mo:02}-{day:02} {h:02}:{m:02}:{s:02} UTC")
        })
        .unwrap_or_else(|_| "unknown".into());
    println!("cargo:rustc-env=WACP_BUILD_TIMESTAMP={timestamp}");
}

fn epoch_days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Civil date from epoch days (algorithm from Howard Hinnant).
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
