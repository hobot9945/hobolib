//! Commonly used macros.

/// `prntln!` - macro for printing a formatted string with an optional file name and line number prefix.
///
/// Works like the standard `println!` macro, but with extra functionality: if the `PRINT_FILE_LINE`
/// flag (controlled via `glob::set_print_file_line`) is `true`, the file name and line number
/// of the call site are printed before the formatted string.
///
/// # Parameters
///
/// Accepts the same parameters as the standard `println!`.
#[macro_export]
macro_rules! prntln {
    ($($arg:tt)*) => {
        if $crate::glob::print_file_line() {
            print!("{}:{}: ", file!(), line!());
        }

        println!($($arg)*);
    };
}

/// `eprntln!` - equivalent to the standard `eprintln!`, but always prepends the file name
/// and line number of the call site.
///
/// # Parameters
/// Accepts the same parameters as the standard `eprintln!`.
#[macro_export]
macro_rules! eprntln {
    ($($arg:tt)*) => {
        eprint!("{}:{}: ", file!(), line!());
        eprintln!($($arg)*);
    };
}

/// `prln!` - concise macro for debug-printing expressions and their values.
///
/// Intended as a quick debugging tool, more rigid but shorter than `println!`.
/// Accepts a comma-separated list of expressions and prints each one in the format
/// `expression=value`. An optional string literal may be passed as the first argument
/// to serve as a label.
///
/// # Usage
/// - For quick inspection of variables during development.
/// - Not intended for user-facing output or structured logging.
///
/// # Examples
///
/// ```
/// use hobolib::prln;
/// let x = 10;
/// let y = 5;
/// prln!(x, y, x + y, 2 * x - y);
/// // Output:
/// // x=10, y=5, x + y=15, 2 * x - y=15
/// prln!("expressions:", x, y, x + y, 2 * x - y);
/// // Output:
/// // expressions: x=10, y=5, x + y=15, 2 * x - y=15
/// ```
#[macro_export]
macro_rules! prln {
    ($message:literal) => {
        if $crate::glob::print_file_line() {
            print!("{}:{}: ", file!(), line!());
        }

        println!("{}", $message);
    };

    ($message:literal, $($val:expr),*) => {
        if $crate::glob::print_file_line() {
            print!("{}:{}: ", file!(), line!());
        };

        print!("{} ", $message);

        $(
            print!("{}={:#?}, ", stringify!($val), $val);
        )*
        println!();
    };

    ($($val:expr),*) => {
        if $crate::glob::print_file_line() {
            print!("{}:{}: ", file!(), line!());
        };

        $(
            print!("{}={:#?}, ", stringify!($val), $val);
        )*
        println!();
    };
}

/// `prlnln!` - same as `prln!`, but prints each argument on a separate line.
///
/// # Examples
///
/// ```
/// use hobolib::prlnln;
/// let x = 10;
/// let y = 5;
/// prlnln!(x, y, x + y, 2 * x - y);
/// // Output:
/// // x=10
/// // y=5
/// // x + y=15
/// // 2 * x - y=15
/// ```
#[macro_export]
macro_rules! prlnln {
    ($message:literal) => {
        if $crate::glob::print_file_line() {
            print!("{}:{}: ", file!(), line!());
        };

        println!("{}", $message);
    };

    ($message:literal, $($val:expr),*) => {
        if $crate::glob::print_file_line() {
            print!("{}:{}: ", file!(), line!());
        };

        println!("{}", $message);

        $(
            println!("{}={:#?}", stringify!($val), $val);
        )*
    };

    ($($val:expr),*) => {
        if $crate::glob::print_file_line() {
            print!("{}:{}: ", file!(), line!());
        };

        $(
            println!("{}={:#?}", stringify!($val), $val);
        )*
    };
}


/// Same as `print!`, but uses `write_all()` (unbuffered output).
/// Needed for printing from tests without waiting for them to finish.
///
/// # Examples
///
/// ```
/// #[macro_export]
/// macro_rules! writ { // Works on Linux only.
///     ($($arg:tt)*) => {
///         let mut res = std::io::stdout();
///         std::io::Write::write_all(&mut res, &format!($($arg)*).as_bytes()[..]).unwrap();
///     };
/// }
/// ```
#[macro_export]
macro_rules! writ {
    ($($arg:tt)*) => {
        // Оборачиваем в блок, чтобы переменная res была локальной
        {
            let mut res = std::io::stdout();
            // Вызываем методы трейта напрямую, не импортируя его через 'use'
            let _ = std::io::Write::write_all(&mut res, format!($($arg)*).as_bytes());
            let _ = std::io::Write::flush(&mut res);
        }
    };
}

/// Same as `println!`, but uses `write_all()` (unbuffered output).
/// Needed for printing from tests without waiting for them to finish.
#[macro_export]
macro_rules! writln {
    () => {
        // Оборачиваем в блок, чтобы переменная res была локальной
        {
            let mut res = std::io::stdout();
            std::io::Write::write_all(&mut res, b"\n").unwrap();
        }
    };

    ($($arg:tt)*) => {
        // Оборачиваем в блок, чтобы переменная res была локальной
        {
            let mut res = std::io::stdout();
            std::io::Write::write_all(&mut res, &format!($($arg)*).as_bytes()[..]).unwrap();
            std::io::Write::write_all(&mut res, b"\n").unwrap();
        }
    };
}

/// `wrln!` - macro for printing expressions and their values. Unbuffered; intended for use
/// in tests so output appears immediately without waiting for the test to complete.
///
/// Accepts a comma-separated list of expressions and prints each one in the format
/// `expression=value`. An optional string literal may be passed as the first argument
/// to serve as a label.
///
/// # Examples
///
/// ```
/// use hobolib::wrln;
/// let x = 10;
/// let y = 5;
/// wrln!(x, y, x + y, 2 * x - y);
/// // Output:
/// // x=10, y=5, x + y=15, 2 * x - y=15
/// wrln!("expressions:", x, y, x + y, 2 * x - y);
/// // Output:
/// // expressions: x=10, y=5, x + y=15, 2 * x - y=15
/// ```
#[macro_export]
macro_rules! wrln {
    ($message:literal) => {
        if $crate::glob::print_file_line() {
            $crate::writ!("{}:{}: ", file!(), line!());
        }

        $crate::writln!("{}", $message);
    };

    ($message:literal, $($val:expr),*) => {
        if $crate::glob::print_file_line() {
            $crate::writ!("{}:{}: ", file!(), line!());
        };

        $crate::writ!("{} ", $message);

        $(
            $crate::writ!("{}={:#?}, ", stringify!($val), $val);
        )*
        $crate::writln!();
    };

    ($($val:expr),*) => {
        if $crate::glob::print_file_line() {
            $crate::writ!("{}:{}: ", file!(), line!());
        };

        $(
            $crate::writ!("{}={:#?}, ", stringify!($val), $val);
        )*
        $crate::writln!();
    };
}

/// `wrlnln!` - same as `wrln!`, but prints each argument on a separate line.
///
/// # Examples
///
/// ```
/// use hobolib::wrlnln;
/// let x = 10;
/// let y = 5;
/// wrlnln!(x, y, x + y, 2 * x - y);
/// // Output:
/// // x=10
/// // y=5
/// // x + y=15
/// // 2 * x - y=15
/// ```
#[macro_export]
macro_rules! wrlnln {
    ($message:literal) => {
        if $crate::glob::print_file_line() {
            $crate::writ!("{}:{}: ", file!(), line!());
        };

        $crate::writln!("{}", $message);
    };

    ($message:literal, $($val:expr),*) => {
        if $crate::glob::print_file_line() {
            $crate::writ!("{}:{}: ", file!(), line!());
        };

        $crate::writln!("{}", $message);

        $(
            $crate::writln!("{}={:#?}", stringify!($val), $val);
        )*
    };

    ($($val:expr),*) => {
        if $crate::glob::print_file_line() {
            $crate::writ!("{}:{}: ", file!(), line!());
        };

        $(
            $crate::writln!("{}={:#?}", stringify!($val), $val);
        )*
    };
}

/// Handles a fatal (non-recoverable) error without panicking.
///
/// Logs the error to `stderr` with file name and line number, displays a modal
/// error dialog to the user, and requests global application shutdown.
///
/// # Usage
/// The caller **must return** after invoking this macro. The macro does not
/// alter control flow (no `panic!`, no `return`).
///
/// # Parameters
/// Accepts the same parameters as the standard `format!`.
///
/// # Example
/// ```ignore
/// fatal!("Failed to bind to port {}: {}", port, err);
/// return;
/// ```
#[macro_export]
macro_rules! fatal {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        $crate::eprntln!("{}", msg);
        $crate::misc::message_box::show_error("Fatal Error", &msg);
        $crate::glob::request_shutdown();
    }};
}

#[cfg(test)]
mod tests {

    // use super::*;

    #[test]
    fn test_prln() {
        prln!("literal");
        //  Вывод:
        // literal

        let x = 10;
        let y = 5;
        prln!(x, y, x + y, 2 * x - y);
        // Вывод:
        // x=10, y=5, x + y=15, 2 * x - y=15

        prln!("expression:", x, y, x + y, 2 * x - y);
        //  Вывод:
        // выражения: x=10, y=5, x + y=15, 2 * x - y=15

        let x = 10;
        let y = 5;
        prlnln!(x, y, x + y, 2 * x - y);
        // Вывод:
        // x=10
        // y=5
        // x + y=15
        // 2 * x - y=15

        std::eprintln!("error message");
        //  Вывод:
        // hobolib/src/macro:105: error message
    }

    #[test]
    fn test_wrln() {

        let x = 10;
        let y = 5;
        writln!("x = {}, y = {}", x, y);
        //  Вывод:
        // x = 10, y = 5

        wrln!("literal");
        //  Вывод:
        // literal

        let x = 10;
        let y = 5;
        wrln!(x, y, x + y, 2 * x - y);
        // Вывод:
        // x=10, y=5, x + y=15, 2 * x - y=15

        wrln!("expression:", x, y, x + y, 2 * x - y);
        //  Вывод:
        // выражения: x=10, y=5, x + y=15, 2 * x - y=15

        let x = 10;
        let y = 5;
        wrlnln!(x, y, x + y, 2 * x - y);
        // Вывод:
        // x=10
        // y=5
        // x + y=15
        // 2 * x - y=15
    }
}
