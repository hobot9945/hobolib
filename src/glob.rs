//! Common library constants and variables.

use std::sync::atomic::{AtomicBool, Ordering};

static PRINT_FILE_LINE: AtomicBool = AtomicBool::new(true);

/// Configures printing macros such as `prln!()` and `wrln!()` to prepend the file name
/// and the call-site line number to the message.
///
/// # Explanation
/// Sometimes a forgotten macro keeps printing unwanted messages during program execution,
/// but locating it is difficult. In that case, enable this flag and the macro will print
/// the exact location where it was called.
///
/// # Params
/// * value: value to set
pub fn set_print_file_line(value: bool) {
    PRINT_FILE_LINE.store(value, Ordering::Relaxed);
}

/// Returns the current state of the `PRINT_FILE_LINE` flag.
pub fn print_file_line() -> bool {
    PRINT_FILE_LINE.load(Ordering::Relaxed)
}
