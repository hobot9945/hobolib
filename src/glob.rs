//! Common library constants, variables, and global application state.
//!
//! RESPONSIBILITIES:
//! - Debug printing configuration (`PRINT_FILE_LINE` flag).
//! - Global shutdown coordination across threads.

use std::sync::atomic::{AtomicBool, Ordering};

/********************************* Debug Printing *************************************************/

static PRINT_FILE_LINE: AtomicBool = AtomicBool::new(true);

/// Configures printing macros such as `prln!()` and `wrln!()` to prepend the file name
/// and the call-site line number to the message.
///
/// # Explanation
/// Sometimes a forgotten macro keeps printing unwanted messages during program execution,
/// but locating it is difficult. In that case, enable this flag and the macro will print
/// the exact location where it was called.
///
/// # Arguments
/// * `value`: value to set.
pub fn set_print_file_line(value: bool) {
    PRINT_FILE_LINE.store(value, Ordering::Relaxed);
}   // set_print_file_line()

/// Returns the current state of the `PRINT_FILE_LINE` flag.
pub fn print_file_line() -> bool {
    PRINT_FILE_LINE.load(Ordering::Relaxed)
}   // print_file_line()

/********************************* Shutdown *******************************************************/

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Requests global application shutdown.
///
/// Thread-safe. Can be called from any thread. Once set, the flag cannot be cleared.
pub fn request_shutdown() {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}   // request_shutdown()

/// Returns `true` if global application shutdown has been requested.
pub fn is_shutdown_requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
}   // is_shutdown_requested()