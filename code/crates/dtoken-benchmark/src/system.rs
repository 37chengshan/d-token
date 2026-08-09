//! Platform measurement: process CPU time and peak RSS via `getrusage`
//! (Unix).  Non-Unix platforms report `None` — the runner never fabricates a
//! platform number it did not observe.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProcessStats {
    /// Total user + system CPU time in microseconds (wall-independent).
    pub cpu_us: Option<u64>,
    /// Peak resident set size in bytes.
    pub max_rss_bytes: Option<u64>,
}

#[cfg(unix)]
pub fn process_stats() -> ProcessStats {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: `usage` is a valid out-pointer for getrusage.
    let result = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    if result != 0 {
        return ProcessStats::default();
    }
    let cpu_us = (usage.ru_utime.tv_sec.saturating_mul(1_000_000)
        + usage.ru_utime.tv_usec as i64
        + usage.ru_stime.tv_sec.saturating_mul(1_000_000)
        + usage.ru_stime.tv_usec as i64)
        .max(0) as u64;
    // macOS reports bytes; Linux reports KiB.
    let max_rss_bytes = if cfg!(target_os = "macos") {
        usage.ru_maxrss.max(0) as u64
    } else {
        (usage.ru_maxrss.max(0) as u64).saturating_mul(1024)
    };
    ProcessStats {
        cpu_us: Some(cpu_us),
        max_rss_bytes: Some(max_rss_bytes),
    }
}

#[cfg(not(unix))]
pub fn process_stats() -> ProcessStats {
    ProcessStats::default()
}

pub fn platform_id() -> String {
    format!(
        "{}-{}-rustc-{}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        option_env!("RUSTC_VERSION").unwrap_or("unknown"),
    )
}
