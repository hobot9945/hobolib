//! Общие константы и переменные библиотеки.

use std::sync::atomic::{AtomicBool, Ordering};

static PRINT_FILE_LINE: AtomicBool = AtomicBool::new(true);

/// Указание макросам печати, таким как prln!() и wrln!() дополнять сообщение именем файла и номера
/// строки вызова макроса.
///
/// # Объяснение
/// Бывает что забытый макрос печатает ненужные сообщения при работе программы, но его крайне трудно
/// найти. В этом случае выставляем этот флаг и макрос показывает точку в программе где он находится.
pub fn set_print_file_line(value: bool) {
    PRINT_FILE_LINE.store(value, Ordering::Relaxed);
}

/// Возвращает текущее состояние флага PRINT_FILE_LINE.
pub fn print_file_line() -> bool {
    PRINT_FILE_LINE.load(Ordering::Relaxed)
}
