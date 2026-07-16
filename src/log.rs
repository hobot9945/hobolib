//! log.rs — Simple file logger.
//!
//! Provides a file-backed logger with three severity levels: Error, Warning, Info.
//! The log file is truncated on creation so each application run starts fresh.
//! Each write is flushed immediately to keep the log durable.
//!
//! # RESPONSIBILITY
//! - Open and truncate the log file on creation.
//! - Write timestamped, level-tagged messages with source location.
//! - Flush after each write.
//! - Flush on drop.
#![allow(unused)]
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::SystemTime;

/// Log severity level.
pub enum LogLevel {
    Error,
    Warning,
    Info,
}   // LogLevel

impl LogLevel {

    /// Returns a short fixed-width tag for log output.
    fn _tag(&self) -> &'static str {
        match self {
            LogLevel::Error   => "ERR",
            LogLevel::Warning => "WRN",
            LogLevel::Info    => "INF",
        }
    }   // _tag()

}   // impl LogLevel

/// File-backed logger.
///
/// Holds an open file handle. The file is truncated when the logger is created.
/// All writes are flushed immediately.
pub struct Log {
    _file: File,
}   // Log

impl Log {

    /// Creates a new logger, opening (or creating) the specified file.
    ///
    /// The file is truncated — old content is discarded.
    ///
    /// # Parameters
    /// - `path`: path to the log file (e.g. "wordsling.log").
    pub fn new(path: impl AsRef<Path>) -> io::Result<Self> {

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path.as_ref())?;

        Ok(Log { _file: file })
    }   // new()

    /// Writes a formatted message to the log.
    ///
    /// Intended to be called from the `log_err!`, `log_wrn!`, `log_inf!` macros,
    /// which automatically supply source file, line and severity level.
    ///
    /// # Parameters
    /// - `level`: severity level.
    /// - `source_file`: source file name from `file!()`.
    /// - `source_line`: source line number from `line!()`.
    /// - `args`: formatted message arguments from `format_args!()`.
    pub fn write(
        &mut self,
        level: LogLevel,
        source_file: &str,
        source_line: u32,
        args: fmt::Arguments,
    ) -> io::Result<()> {

        // Format: "2025-01-15 14:23:05.347 ERR [src/screen_writer.rs:87] message"
        let timestamp = _timestamp();

        writeln!(
            self._file,
            "{} {} [{}:{}] {}",
            timestamp, level._tag(), source_file, source_line, args
        )?;

        // Flush immediately so the record survives a crash.
        self._file.flush()
    }   // write()

}   // impl Log

impl Drop for Log {

    /// Best-effort flush on drop.
    fn drop(&mut self) {
        let _ = self._file.flush();
    }   // drop()

}   // impl Drop for Log

/// Builds a local wall-clock timestamp string with millisecond precision.
///
/// Uses only `std` without external crates. The output format is
/// `YYYY-MM-DD HH:MM:SS.mmm` in **UTC** (not local time).
///
/// # Note
/// For true local-time formatting the `chrono` crate would be needed.
/// UTC is acceptable for a diagnostic log.
fn _timestamp() -> String {

    let now = SystemTime::now();
    let duration = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();

    let total_secs = duration.as_secs();
    let millis = duration.subsec_millis();

    // Break total seconds into date and time components.
    let secs_in_day = total_secs % 86400;
    let hours = secs_in_day / 3600;
    let minutes = (secs_in_day % 3600) / 60;
    let seconds = secs_in_day % 60;

    // Days since Unix epoch.
    let mut days = (total_secs / 86400) as i64;

    // Convert days since epoch to year/month/day.
    // Algorithm based on Howard Hinnant's civil_from_days.
    days += 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let doe = (days - era * 146_097) as u64;                          // day of era
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // year of era
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);               // day of year
    let mp = (5 * doy + 2) / 153;                                     // month proxy
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        year, month, day, hours, minutes, seconds, millis
    )
}   // _timestamp()