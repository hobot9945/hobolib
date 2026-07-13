use std::panic;
use crate::glob::request_shutdown;
use crate::misc::message_box;

/// Installs a global panic hook that requests application shutdown
/// on any panic in any thread.
///
/// When a panic occurs in any thread:
/// 1. The default handler prints the panic message and backtrace to stderr.
/// 2. A modal error dialog is shown to inform the user.
/// 3. Application shutdown is requested via the global state flag.
pub fn install_panic_hook() {

    // Save the default hook callback function.
    let default_hook = panic::take_hook();

    panic::set_hook(Box::new(move |panic_info| {

        // Call the default hook first (prints panic message and backtrace)
        default_hook(panic_info);

        // Build a human-readable error message
        let message = match panic_info.payload().downcast_ref::<&str>() {
            Some(msg) => msg.to_string(),
            None => match panic_info.payload().downcast_ref::<String>() {
                Some(msg) => msg.clone(),
                None => "Unknown error".to_string(),
            },  // match String
        };  // match &str

        let location = match panic_info.location() {
            Some(loc) => format!("{}:{}", loc.file(), loc.line()),
            None => "unknown location".to_string(),
        };  // match location

        let full_message = format!(
            "The application encountered a fatal error and will be shut down.\n\n\
             Error: {}\n\
             Location: {}",
            message,
            location
        );

        // Show modal error dialog to the user
        message_box::show_error("Wordsling - Fatal Error", &full_message);

        // Request application shutdown
        request_shutdown();
    }));
}   // install_panic_hook()