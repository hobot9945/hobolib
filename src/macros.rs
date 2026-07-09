//! Часто используемые макросы.

/// `prntln!` - макрос для печати форматированной строки с опциональным добавлением имени файла и номера строки.
///
/// Этот макрос работает аналогично стандартному макросу `println!`, но имеет дополнительную
/// функциональность: если флаг `PRINT_FILE_LINE` (управляется через `glob::set_print_file_line`) равен `true`,
/// то перед выводом форматированной строки будет напечатано имя файла и номер строки,
/// откуда был вызван макрос.
///
/// # Параметры
///
/// Макрос принимает те же параметры, что и стандартный `println!`
#[macro_export]
macro_rules! prntln {
    ($($arg:tt)*) => {
        if $crate::glob::print_file_line() {
            print!("{}:{}: ", file!(), line!());
        }

        println!($($arg)*);
    };
}

/// `eprntln!` - аналогичен системному макросу eprintln!, с той разницей, что выводится имя файла и
/// номер строки, в которой макрос был вызван.
///
/// # Параметры
/// Макрос принимает те же параметры, что и стандартный `eprintln!`
#[macro_export]
macro_rules! eprntln {
    ($($arg:tt)*) => {
        eprint!("{}:{}: ", file!(), line!());
        eprintln!($($arg)*);
    };
}

/// `prln!` - макрос для печати выражения и его значения.
///
/// Этот макрос принимает список выражений, разделенных запятыми,
/// и выводит каждое выражение в формате "выражение=значение". Первым параметром может идти вводная строка
///
/// # Примеры
///
/// ```
/// use hobolib::prln;
/// let x = 10;
/// let y = 5;
/// prln!(x, y, x + y, 2 * x - y);
/// // Вывод:
/// // x=10, y=5, x + y=15, 2 * x - y=15
/// prln!("выражения:", x, y, x + y, 2 * x - y);
/// //  Вывод:
/// // выражения: x=10, y=5, x + y=15, 2 * x - y=15
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

/// `prlnln!` - то же самое что и `prln!`, но печатает каждый аргумент с новой стоки.
///
/// # Примеры
///
/// ```
/// use hobolib::prlnln;
/// let x = 10;
/// let y = 5;
/// prlnln!(x, y, x + y, 2 * x - y);
/// // Вывод:
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


/// Делает то же самое что print!, он для вывода использует write_all() (без буферизации).
/// Нужно для печати из тестов, не дожидаясь их окончания.
// #[macro_export]
// macro_rules! writ { // Работает только в линуксе.
//     ($($arg:tt)*) => {
//         let mut res = std::io::stdout();
//         std::io::Write::write_all(&mut res, &format!($($arg)*).as_bytes()[..]).unwrap();
//     };
// }
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

/// Делает то же самое что println!, он для вывода использует write_all() (без буферизации).
/// Нужно для печати из тестов, не дожидаясь их окончания.
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

/// `wrln!` - макрос для печати выражения и его значения. Нужен для печати из тестов не дожидаясь их
/// окончания.
///
/// Этот макрос принимает список выражений, разделенных запятыми,
/// и выводит каждое выражение в формате "выражение=значение". Первым параметром может идти вводная строка
///
/// # Примеры
///
/// ```
/// use hobolib::wrln;
/// let x = 10;
/// let y = 5;
/// wrln!(x, y, x + y, 2 * x - y);
/// // Вывод:
/// // x=10, y=5, x + y=15, 2 * x - y=15
/// wrln!("выражения:", x, y, x + y, 2 * x - y);
/// //  Вывод:
/// // выражения: x=10, y=5, x + y=15, 2 * x - y=15
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

/// `wrlnln!` - то же самое что и `wrln!`, но печатает каждый аргумент с новой стоки.
///
/// # Примеры
///
/// ```
/// use hobolib::wrlnln;
/// let x = 10;
/// let y = 5;
/// wrlnln!(x, y, x + y, 2 * x - y);
/// // Вывод:
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

        prln!("выражения:", x, y, x + y, 2 * x - y);
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

        wrln!("выражения:", x, y, x + y, 2 * x - y);
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
