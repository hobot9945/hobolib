//! Modal message box helpers for user-facing notifications.
//!
//! RESPONSIBILITIES:
//! - Display error, warning and informational modal dialogs.
//! - Provide a simple API that hides platform-specific details.
//!
//! Currently supports Windows only. On other platforms the functions
//! fall back to printing to `stderr`.

/// Displays a modal error message box.
///
/// The dialog blocks the calling thread until the user closes it.
///
/// # Parameters
/// - `title`: Window title.
/// - `message`: Error description shown in the dialog body.
#[allow(dead_code)]
pub fn show_error(title: &str, message: &str) {
    _show_message_box(title, message, _MessageBoxStyle::Error);
}   // show_error()

/// Displays a modal warning message box.
///
/// The dialog blocks the calling thread until the user closes it.
///
/// # Parameters
/// - `title`: Window title.
/// - `message`: Warning text shown in the dialog body.
#[allow(dead_code)]
pub fn show_warning(title: &str, message: &str) {
    _show_message_box(title, message, _MessageBoxStyle::Warning);
}   // show_warning()

/// Displays a modal informational message box.
///
/// The dialog blocks the calling thread until the user closes it.
///
/// # Parameters
/// - `title`: Window title.
/// - `message`: Informational text shown in the dialog body.
#[allow(dead_code)]
pub fn show_info(title: &str, message: &str) {
    _show_message_box(title, message, _MessageBoxStyle::Info);
}   // show_info()

// ---------------------------------------------------------------------------
// Platform-specific implementation
// ---------------------------------------------------------------------------

/// Internal message box style selector.
enum _MessageBoxStyle {
    Error,
    Warning,
    Info,
}   // enum _MessageBoxStyle

/// Displays a modal message box with the given style.
///
/// On Windows, delegates to the Win32 `MessageBoxW` API.
/// On other platforms, prints the message to `stderr` as a fallback.
///
/// # Parameters
/// - `title`: Window title text.
/// - `message`: Dialog body text.
/// - `style`: Visual style and icon of the dialog.
fn _show_message_box(title: &str, message: &str, style: _MessageBoxStyle) {
    #[cfg(target_os = "windows")]
    {
        _show_message_box_windows(title, message, style);
    }   // cfg windows

    #[cfg(not(target_os = "windows"))]
    {
        let prefix = match style {
            _MessageBoxStyle::Error   => "ERROR",
            _MessageBoxStyle::Warning => "WARNING",
            _MessageBoxStyle::Info    => "INFO",
        };  // match
        eprintln!("[{}] {}: {}", prefix, title, message);
    }   // cfg not windows
}   // _show_message_box()

/// Win32 `MessageBoxW` implementation.
#[cfg(target_os = "windows")]
fn _show_message_box_windows(title: &str, message: &str, style: _MessageBoxStyle) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW,
        MB_ICONERROR,
        MB_ICONWARNING,
        MB_ICONINFORMATION,
        MB_OK,
        MB_SYSTEMMODAL,
    };

    let flags = MB_OK | MB_SYSTEMMODAL | match style {
        _MessageBoxStyle::Error   => MB_ICONERROR,
        _MessageBoxStyle::Warning => MB_ICONWARNING,
        _MessageBoxStyle::Info    => MB_ICONINFORMATION,
    };  // match

    // Convert Rust &str to null-terminated UTF-16 (Windows wide string)
    let title_wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let message_wide: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),       // hWnd: no parent window
            message_wide.as_ptr(),      // lpText
            title_wide.as_ptr(),        // lpCaption
            flags,                      // uType
        );
    }   // unsafe
}   // _show_message_box_windows()