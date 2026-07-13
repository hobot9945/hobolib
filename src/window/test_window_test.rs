//! test_window_test.rs — Дымовые тесты для Win32-интеграции агента.
//!
//! ОПИСАНИЕ:
//! Тесты этого файла завязаны на реальное окружение пользователя (окна/фокус),
//! поэтому они помечены как #[ignore] и запускаются только вручную.

fn needle() -> String {
    "Notepad".to_string()
}

#[cfg(test)]
mod tests {
    use crate::clipboard;
    use crate::window::paste_text_into_window_by_needle;
    use crate::writln;
    use super::*;

    /// Дымовой тест: вставка текста в окно + верификация через Ctrl+A/Ctrl+C.
    ///
    /// Алгоритм использования:
    /// 1. Открой/подготовь окно (needle = WINDOW_TITLE).
    /// 2. Поставь курсор ввода в нужное текстовое поле (внутри страницы/приложения).
    /// 3. Запусти тест вручную (#[ignore]).
    /// 4. Тест:
    ///    - вставит `payload`,
    ///    - проверит, что он реально оказался в поле,
    ///    - проверит, что clipboard восстановился к `clip_before`.
    #[test]
    fn smoke_paste_text_with_verification() {

        // Текст, который пробуем вставить.
        let payload = "Привет от теста!";

        // Текст, который должен восстановиться в clipboard после вставки и проверки.
        let clip_before = "Этот текст был до вставки.";

        // Подготовить clipboard: задаём известное значение, чтобы потом проверить восстановление.
        clipboard::set_clipboard_text(&clip_before)
            .unwrap_or_else(|e| panic!("[-] Не удалось подготовить clipboard перед тестом: {}", e));

        // Вставка + верификация (внутри paste_text_into_window_by_needle()).
        let res = paste_text_into_window_by_needle(&needle(), payload);

        // В дымовом тесте важно увидеть текст ошибки, если что-то пошло не так.
        match res {
            Ok(_) => writln!("[+] paste_text_into_window_by_needle(): вставка подтверждена."),
            Err(e) => panic!("[-] Вставка/верификация не удалась: {}", e),
        }   // match

        // Проверяем, что clipboard восстановился (best effort, только текст).
        let clip_after = clipboard::get_clipboard_text()
            .unwrap_or_else(|e| panic!("[-] Не удалось прочитать clipboard после теста: {}", e));

        if clip_after != clip_before {
            panic!(
                "[-] Clipboard не восстановился.\nОжидалось:\n'{}'\nПолучилось:\n'{}'",
                clip_before, clip_after
            );
        }   // if

        println!("[+] Clipboard восстановился корректно.");
    }   // just_a_run_paste_with_verification()
}
