//! test_screenshot_test.rs — Дымовые тесты подсистемы скриншотов.
//!
//! ОПИСАНИЕ:
//! Набор smoke тестов для ручного запуска, проверяющих базовую работоспособность
//! захвата изображений через xcap и сохранения результата в PNG.
//!
//! ОТВЕТСТВЕННОСТЬ:
//! - Запустить самый простой сценарий захват всех мониторов -> PNG.
//! - Подтвердить, что файл создан и не пустой.
//!
//! ПРИМЕЧАНИЯ:
//! - Тест помечен #[ignore], чтобы не запускаться в обычном cargo test.
//! - Пишет в %TEMP%, имя содержит timestamp чтобы избежать коллизий.

//----------------------------------------------------------------------------------------------
//                  Вспомогательные функции для тестов
//----------------------------------------------------------------------------------------------

use windows::core::BOOL;

/// Описание: Ищет окно по подстроке заголовка (case-insensitive).
///
/// # Параметры
/// - `needle`: Подстрока для поиска в заголовках окон.
///
/// # Возвращаемое значение
/// Тип: Result<HWND, String>: Дескриптор первого найденного окна или ошибка.
///
/// # Алгоритм работы
/// - Перебирает все top-level окна через EnumWindows.
/// - Фильтрует только видимые окна.
/// - Возвращает первое окно, заголовок которого содержит needle (case-insensitive).
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - needle пустой;
/// - ни одно окно не найдено по заданной подстроке.
fn find_window_by_needle(needle: &str) -> Result<windows::Win32::Foundation::HWND, String> {
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowTextW, IsWindowVisible,
    };

    let needle = needle.trim();
    if needle.is_empty() {
        return Err("needle пустой".to_string());
    }   // if

    // Контекст для callback-функции EnumWindows.
    #[derive(Default)]
    struct EnumCtx {
        needle_lower: String,
        found_hwnd: Option<HWND>,
    }   // EnumCtx

    // Callback для EnumWindows.
    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam.0 as *mut EnumCtx);

        // Уже нашли — стоп.
        if ctx.found_hwnd.is_some() {
            return BOOL(0);
        }   // if

        // Фильтр: только видимые окна.
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }   // if

        // Получаем заголовок окна.
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return BOOL(1);
        }   // if

        let mut buf: Vec<u16> = vec![0u16; (len as usize) + 1];
        let written = GetWindowTextW(hwnd, &mut buf);
        if written <= 0 {
            return BOOL(1);
        }   // if

        let title = String::from_utf16_lossy(&buf[..written as usize]);
        // Case-insensitive поиск подстроки.
        if !title.to_lowercase().contains(&ctx.needle_lower) {
            return BOOL(1);
        }   // if

        ctx.found_hwnd = Some(hwnd);
        BOOL(0) // Стоп после первого совпадения.
    }   // enum_proc()

    let mut ctx = EnumCtx {
        needle_lower: needle.to_lowercase(),
        ..Default::default()
    };

    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM((&mut ctx as *mut EnumCtx) as isize));
    }   // unsafe

    ctx.found_hwnd.ok_or_else(|| {
        format!("окно не найдено по needle='{}'", needle)
    })
}   // find_window_by_needle()

#[cfg(test)]
mod smoke_tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    use crate::screenshot::{capture_all_monitors_to_png, logical_to_physical_map};
    use crate::screenshot::test_screenshot_test::find_window_by_needle;

    /// Подстрока заголовка окна для поиска в тестах захвата окна.
    /// Измени на заголовок окна, которое гарантированно открыто на твоей машине.
    const WINDOW_NEEDLE: &str = "deepseek";

    /// Координаты и размеры области экрана для тестов захвата региона.
    /// Область 400x300 в левом верхнем углу основного монитора.
    const REGION_X: i32 = 100;
    const REGION_Y: i32 = 100;
    const REGION_WIDTH: u32 = 400;
    const REGION_HEIGHT: u32 = 300;

    /// Описание: Дымовой тест захват всех мониторов -> PNG.
    ///
    /// # Ошибки
    /// Падает (panic), если:
    /// - не удалось сделать скриншот или сохранить PNG;
    /// - файл не создан или имеет нулевой размер.
    ///
    /// # Побочные эффекты
    /// - Создает PNG-файл в каталоге %TEMP%.
    /// - Пишет путь к файлу в stderr (для удобства ручной проверки).
    #[test]
    #[ignore]
    fn smoke_capture_all_monitors_to_png() {
        // Генерируем уникальный суффикс имени файла, чтобы не конфликтовать с предыдущими запусками теста.
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_millis();

        // Пишем в текущую директорию.
        let path = PathBuf::from(format!("hobot_smoke_all_monitors_{}.png", ts));

        // Основное действие: захват всех мониторов и сохранение в PNG.
        capture_all_monitors_to_png(&path)
            .unwrap_or_else(|e| panic!("capture_all_monitors_to_png failed: {}", e));

        // Проверяем, что файл реально создан и не пустой (минимальная sanity-проверка результата).
        let meta = std::fs::metadata(&path)
            .unwrap_or_else(|e| panic!("screenshot file not created ({}): {}", path.display(), e));

        assert!(meta.len() > 0, "screenshot file is empty: {}", path.display());

        // Путь выводим, чтобы можно было открыть файл руками после запуска с --nocapture.
        eprintln!("screenshot saved to: {}", path.display());

        // Если не хочешь оставлять файл — раскомментируй:
        // let _ = std::fs::remove_file(&path);
    }   // smoke_capture_all_monitors_to_png()

    /// Описание: Дымовой тест "захват всех мониторов → clipboard", используя arboard.
    ///
    /// # Алгоритм работы
    /// - Вызывает `capture_all_monitors_to_clipboard_arboard()`.
    /// - Дальнейшая проверка выполняется вручную: вставь изображение (Ctrl+V) в любое приложение.
    ///
    /// # Паника
    /// Падает (panic), если захват/копирование в clipboard вернуло ошибку.
    ///
    /// # Побочные эффекты
    /// - Полностью перезаписывает системный буфер обмена (clipboard).
    #[test]
    #[ignore]
    fn smoke_capture_all_monitors_to_clipboard() {
        crate::screenshot::capture_all_monitors_to_clipboard()
            .unwrap_or_else(|e| panic!("capture_all_monitors_to_clipboard_arboard failed: {}", e));

        eprintln!("скриншот помещён в clipboard (arboard). Проверь вставкой Ctrl+V.");
    }   // smoke_capture_all_monitors_to_clipboard_arboard()

    /// Описание: Дымовой тест построения карты логических → физических индексов мониторов.
    ///
    /// # Алгоритм работы
    /// - Вызывает `build_logical_to_physical_map()`.
    /// - Выводит карту соответствия и координаты каждого монитора.
    ///
    /// # Паника
    /// Падает (panic), если не удалось построить карту.
    ///
    /// # Побочные эффекты
    /// - Пишет информацию в stderr.
    #[test]
    #[ignore]
    fn smoke_build_logical_to_physical_map() {
        use std::io::{self, Write};
        use xcap::Monitor;

        let mut out = io::stderr();

        // Получаем карту соответствия.
        let map = logical_to_physical_map().unwrap();

        // Получаем список мониторов для вывода координат.
        let monitors = Monitor::all()
            .unwrap_or_else(|e| panic!("Monitor::all() failed: {}", e));

        writeln!(out, "").unwrap();
        writeln!(out, "=== Карта соответствия: логический -> физический ===").unwrap();
        writeln!(out, "").unwrap();

        for (logical_idx, &physical_idx) in map.iter().enumerate() {
            let m = &monitors[physical_idx];

            // Извлекаем координаты и размеры монитора.
            let x = m.x().unwrap_or(-9999);
            let y = m.y().unwrap_or(-9999);
            let w = m.width().unwrap_or(0);
            let h = m.height().unwrap_or(0);

            writeln!(
                out,
                "  logical={} -> physical={}  (x={}, y={}, {}x{})",
                logical_idx, physical_idx, x, y, w, h
            ).unwrap();
        }   // for

        writeln!(out, "").unwrap();
        writeln!(out, "Всего мониторов: {}", map.len()).unwrap();
    }   // smoke_build_logical_to_physical_map()

    /// Описание: Дымовой тест захвата первого монитора (index=0) -> PNG.
    /// Помещает файл в текущий каталог.
    ///
    /// # Паника
    /// Падает (panic), если:
    /// - не удалось сделать скриншот или сохранить PNG;
    /// - файл не создан или имеет нулевой размер.
    ///
    /// # Побочные эффекты
    /// - Создает PNG-файл в текущей директории.
    /// - Пишет путь к файлу в stderr.
    #[test]
    #[ignore]
    fn smoke_capture_monitor_to_png() {
        use crate::screenshot::capture_monitor_to_png;

        // Генерируем уникальный суффикс имени файла.
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_millis();

        let path = PathBuf::from(format!("hobot_smoke_monitor_0_{}.png", ts));

        // Захватываем монитор по индексу.
        capture_monitor_to_png(1, &path)
            .unwrap_or_else(|e| panic!("capture_monitor_to_png failed: {}", e));

        // Проверяем, что файл создан и не пустой.
        let meta = std::fs::metadata(&path)
            .unwrap_or_else(|e| panic!("screenshot file not created ({}): {}", path.display(), e));

        assert!(meta.len() > 0, "screenshot file is empty: {}", path.display());

        eprintln!("screenshot saved to: {}", path.display());
    }   // smoke_capture_monitor_to_png()

    /// Описание: Дымовой тест захвата первого монитора (index=0) -> clipboard.
    ///
    /// # Алгоритм работы
    /// - Вызывает `capture_monitor_to_clipboard(0)`.
    /// - Проверка — вручную: вставь изображение (Ctrl+V) в любое приложение.
    ///
    /// # Паника
    /// Падает (panic), если захват или копирование в clipboard вернуло ошибку.
    ///
    /// # Побочные эффекты
    /// - Полностью перезаписывает системный буфер обмена.
    #[test]
    #[ignore]
    fn smoke_capture_monitor_to_clipboard() {
        use crate::screenshot::capture_monitor_to_clipboard;

        capture_monitor_to_clipboard(1)
            .unwrap_or_else(|e| panic!("capture_monitor_to_clipboard failed: {}", e));

        eprintln!("скриншот монитора 0 помещён в clipboard. Проверь вставкой Ctrl+V.");
    }   // smoke_capture_monitor_to_clipboard()

    /// Описание: Дымовой тест проверки ошибки при невалидном индексе монитора.
    ///
    /// # Алгоритм работы
    /// - Вызывает `capture_monitor_rgba(9999)` — заведомо несуществующий монитор.
    /// - Проверяет, что функция возвращает Err.
    ///
    /// # Побочные эффекты
    /// - Нет (clipboard не изменяется, файлы не создаются).
    #[test]
    #[ignore]
    fn smoke_capture_monitor_invalid_index() {
        use crate::screenshot::capture_monitor_rgba;

        let result = capture_monitor_rgba(9999);

        assert!(result.is_err(), "expected Err for invalid monitor index");
        eprintln!("корректно получена ошибка: {}", result.unwrap_err());
    }   // smoke_capture_monitor_invalid_index()

    //----------------------------------------------------------------------------------------------
    //                  Тесты захвата области экрана
    //----------------------------------------------------------------------------------------------

    /// Описание: Дымовой тест захвата области экрана -> PNG.
    ///
    /// # Алгоритм работы
    /// - Захватывает область экрана с координатами из констант REGION_*.
    /// - Сохраняет результат в PNG-файл в текущей директории.
    /// - Проверяет, что файл создан и не пустой.
    ///
    /// # Паника
    /// Падает (panic), если:
    /// - не удалось захватить область экрана;
    /// - не удалось сохранить PNG;
    /// - файл не создан или имеет нулевой размер.
    ///
    /// # Побочные эффекты
    /// - Создает PNG-файл в текущей директории.
    /// - Пишет путь к файлу в stderr.
    #[test]
    #[ignore]
    fn smoke_capture_region_to_png() {
        use crate::screenshot::capture_region_to_png;

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_millis();

        let path = PathBuf::from(format!("hobot_smoke_region_{}.png", ts));

        let cursor_info = capture_region_to_png(
            REGION_X,
            REGION_Y,
            REGION_WIDTH,
            REGION_HEIGHT,
            &path,
        ).unwrap_or_else(|e| panic!("capture_region_to_png failed: {}", e));

        let meta = std::fs::metadata(&path)
            .unwrap_or_else(|e| panic!("screenshot file not created ({}): {}", path.display(), e));

        assert!(meta.len() > 0, "screenshot file is empty: {}", path.display());

        eprintln!("screenshot saved to: {}", path.display());
        eprintln!("{}", cursor_info.report());
    }   // smoke_capture_region_to_png()

    /// Описание: Дымовой тест захвата области экрана -> clipboard.
    ///
    /// # Алгоритм работы
    /// - Захватывает область экрана с координатами из констант REGION_*.
    /// - Помещает результат в буфер обмена.
    /// - Проверка — вручную: вставь изображение (Ctrl+V) в любое приложение.
    ///
    /// # Паника
    /// Падает (panic), если захват или копирование в clipboard вернуло ошибку.
    ///
    /// # Побочные эффекты
    /// - Полностью перезаписывает системный буфер обмена.
    #[test]
    #[ignore]
    fn smoke_capture_region_to_clipboard() {
        use crate::screenshot::capture_region_to_clipboard;

        let cursor_info = capture_region_to_clipboard(
            REGION_X,
            REGION_Y,
            REGION_WIDTH,
            REGION_HEIGHT,
        ).unwrap_or_else(|e| panic!("capture_region_to_clipboard failed: {}", e));

        eprintln!(
            "скриншот области (x={}, y={}, {}x{}) помещён в clipboard. Проверь вставкой Ctrl+V.",
            REGION_X, REGION_Y, REGION_WIDTH, REGION_HEIGHT
        );
        eprintln!("{}", cursor_info.report());
    }   // smoke_capture_region_to_clipboard()

    //----------------------------------------------------------------------------------------------
    //                  Тесты захвата окна по HWND
    //----------------------------------------------------------------------------------------------

    /// Описание: Дымовой тест захвата окна по HWND -> PNG.
    ///
    /// # Алгоритм работы
    /// - Ищет окно по подстроке заголовка WINDOW_NEEDLE.
    /// - Захватывает скриншот найденного окна.
    /// - Сохраняет результат в PNG-файл в текущей директории.
    /// - Проверяет, что файл создан и не пустой.
    ///
    /// # Паника
    /// Падает (panic), если:
    /// - окно не найдено по заданной подстроке;
    /// - не удалось захватить скриншот окна;
    /// - не удалось сохранить PNG;
    /// - файл не создан или имеет нулевой размер.
    ///
    /// # Побочные эффекты
    /// - Создает PNG-файл в текущей директории.
    /// - Пишет путь к файлу в stderr.
    #[test]
    #[ignore]
    fn smoke_capture_window_to_png() {
        use crate::screenshot::capture_window_to_png;

        let hwnd = find_window_by_needle(WINDOW_NEEDLE)
            .unwrap_or_else(|e| panic!("окно не найдено: {}", e));

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_millis();

        let path = PathBuf::from(format!("hobot_smoke_window_{}.png", ts));

        let cursor_info = capture_window_to_png(hwnd, &path)
            .unwrap_or_else(|e| panic!("capture_window_to_png failed: {}", e));

        let meta = std::fs::metadata(&path)
            .unwrap_or_else(|e| panic!("screenshot file not created ({}): {}", path.display(), e));

        assert!(meta.len() > 0, "screenshot file is empty: {}", path.display());

        eprintln!("screenshot saved to: {}", path.display());
        eprintln!("{}", cursor_info.report());
    }   // smoke_capture_window_to_png()

    /// Описание: Дымовой тест захвата окна по HWND -> clipboard.
    ///
    /// # Алгоритм работы
    /// - Ищет окно по подстроке заголовка WINDOW_NEEDLE.
    /// - Захватывает скриншот найденного окна.
    /// - Помещает результат в буфер обмена.
    /// - Проверка — вручную: вставь изображение (Ctrl+V) в любое приложение.
    ///
    /// # Паника
    /// Падает (panic), если:
    /// - окно не найдено по заданной подстроке;
    /// - захват или копирование в clipboard вернуло ошибку.
    ///
    /// # Побочные эффекты
    /// - Полностью перезаписывает системный буфер обмена.
    #[test]
    #[ignore]
    fn smoke_capture_window_to_clipboard() {
        use crate::screenshot::capture_window_to_clipboard;

        let hwnd = find_window_by_needle(WINDOW_NEEDLE)
            .unwrap_or_else(|e| panic!("окно не найдено: {}", e));

        let cursor_info = capture_window_to_clipboard(hwnd)
            .unwrap_or_else(|e| panic!("capture_window_to_clipboard failed: {}", e));

        eprintln!(
            "скриншот окна '{}' помещён в clipboard. Проверь вставкой Ctrl+V.",
            WINDOW_NEEDLE
        );
        eprintln!("{}", cursor_info.report());
    }   // smoke_capture_window_to_clipboard()
}   // mod smoke_tests