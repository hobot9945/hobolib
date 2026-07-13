//! window_backend.rs — Внутренние Win32-примитивы для `library::window`.
//!
//! # ОПИСАНИЕ
//! Модуль содержит низкоуровневые функции (WinAPI), вынесенные из `library::window`,
//! чтобы разделить публичный API и “грязную” платформенную реализацию.
//!
//! # ОТВЕТСТВЕННОСТЬ
//! 1) Перечисление и поиск top-level окон:
//!    - контекст и callback для `EnumWindows`,
//!    - поиск окна по подстроке заголовка (needle) с проверкой однозначности.
//!
//! 2) Сбор информации по окну:
//!    - получение заголовка через `GetWindowTextW`,
//!    - геометрия через `DwmGetWindowAttribute`,
//!    - признаки foreground/minimized.
//!
//! 3) Примитив фокусировки окна (одна попытка):
//!    - `SetForegroundWindow` + верификация через `GetForegroundWindow`,
//!    - восстановление свернутого окна (SW_RESTORE).
//!
//! 4) Best-effort поддержка фокусировки “чужого” окна:
//!    - ручная загрузка `AttachThreadInput` из `user32.dll`.
//!
//! 5) Верификация вставки текста в текущем фокусе:
//!    - запуск Ctrl+A / Ctrl+C,
//!    - ожидание “стабилизации” clipboard по длине,
//!    - сравнение хвоста буфера с ожидаемым текстом (игнорируя whitespace, с лимитом длины).
//!
//! # ИНВАРИАНТЫ
//! - Функции модуля не предназначены для прямого вызова извне `library::window` (internal backend).
//! - Callback `EnumWindows` не делает ранний выход через `FALSE`, чтобы не ломать трактовку результата.
//!
//! # ПРИМЕЧАНИЯ
//! - Логика “публичных” ретраев/таймаутов и политики ошибок находится в `library::window`.
//! - Верификация текстового ввода опирается на эвристики (тайминги/ожидание clipboard),
//!   т.к. момент применения Ctrl+V / Ctrl+C из реального UI точно определить нельзя.

use std::ffi::CString;
use std::thread::sleep;
use std::time::Duration;
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextLengthW,
                                              GetWindowTextW, IsIconic, IsWindowVisible,
                                              SetForegroundWindow, ShowWindow, SW_RESTORE};
use crate::window::{get_window_list, WindowInfo};
use crate::{clipboard, keyboard};

/// Контекст перечисления окон для callback `EnumWindows`.
///
/// # Поля
/// - `needle`:
///   - `Some("...")` => добавляем в список только окна, у которых title содержит needle;
///   - `None` => добавляем окна согласно флагам include_*.
/// - `include_invisible`:
///   - `true` => включать невидимые окна (IsWindowVisible == false),
///   - `false` => включать только видимые.
/// - `include_empty_title`:
///   - `true` => включать окна с пустым заголовком,
///   - `false` => исключать окна с пустым заголовком.
/// - `hwnd_vec`: накопленный список найденных HWND.
pub(super) struct _FindWindowCtx<'a> {
    pub(super) needle: Option<&'a str>,      // Подстрока фильтрации по заголовку (опционально).
    pub(super) include_invisible: bool,      // Включать невидимые окна?
    pub(super) include_empty_title: bool,    // Включать окна с пустым заголовком?
    pub(super) hwnd_vec: Vec<HWND>,          // Результат: список hwnd.
}   // _FindWindowCtx


/// Извлекает текст из текущего сфокусированного поля ввода через `Ctrl+A` / `Ctrl+C`.
///
/// # Инварианты (обязанность вызывающего кода)
/// - Окно с целевым полем ввода уже сфокусировано.
/// - Поле ввода содержит непустой текст.
///
/// # Механика
/// Функция НЕ изменяет clipboard напрямую — только ставит в очередь клавиатурные команды
/// `Ctrl+A` / `Ctrl+C` и ждёт, пока clipboard обновится в результате их обработки приложением.
/// Это безопасно при наличии незавершённой вставки (`Ctrl+V`): клавиатурные события обрабатываются
/// строго последовательно, поэтому `Ctrl+A` и `Ctrl+C` выполнятся только после завершения `Ctrl+V`.
///
/// # Алгоритм работы
/// 1. Запоминает текущий номер изменения clipboard (`GetClipboardSequenceNumber`).
/// 2. Отправляет `Ctrl+A` (выделить всё) — встаёт в очередь.
/// 3. Отправляет `Ctrl+C` (скопировать выделение) — встаёт в очередь за `Ctrl+A`.
/// 4. Ожидает изменения номера clipboard (признак того, что `Ctrl+C` начал работать).
/// 5. После изменения номера читает clipboard в цикле, ожидая стабилизации длины текста.
/// 6. Снимает выделение стрелкой вправо (best effort).
/// 7. Возвращает считанный текст.
///
/// # Возвращаемое значение
/// `Result<String, String>`:
/// - `Ok(String)` — текст, считанный из поля ввода.
/// - `Err(String)` — описание ошибки.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - не удалось отправить `Ctrl+A` или `Ctrl+C`;
/// - за отведённое время clipboard не обновился (sequence number не изменился);
/// - длина текста в clipboard не стабилизировалась.
///
/// # Побочные эффекты
/// - **Не изменяет clipboard напрямую.** Clipboard обновляется только как результат
///   обработки `Ctrl+C` целевым приложением.
/// - Генерирует события клавиатуры (`Ctrl+A`, `Ctrl+C`, стрелка вправо).
/// - Деструктивна к выделению в поле (делает `Ctrl+A`).
pub(super) fn _extract_text_from_focused_input() -> Result<String, String> {

    /// Максимальное число итераций ожидания смены sequence number (500 × 10 мс = 5 с).
    const SEQ_POLL_ITERS: usize = 500;

    /// Максимальное число итераций ожидания стабилизации длины текста (200 × 10 мс = 2 с).
    const LEN_POLL_ITERS: usize = 200;

    /// Пауза между итерациями опроса (мс).
    const POLL_DELAY_MS: u64 = 10;

    /// Сколько подряд итераций длина текста должна оставаться неизменной,
    /// чтобы считать clipboard стабилизировавшимся.
    const STABLE_THRESHOLD: usize = 10;

    // 1) Запоминаем текущий номер изменения clipboard.
    let init_clip_seq: u32 = unsafe { GetClipboardSequenceNumber() };

    // 2) Выделить всё содержимое поля. Встаёт в очередь за возможным незавершённым Ctrl+V.
    keyboard::send_ctrl_a()
        .map_err(|e| format!("_extract_text: Ctrl+A не удался: {}", e))?;

    // 3) Скопировать выделение в clipboard. Встаёт в очередь за Ctrl+A.
    keyboard::send_ctrl_c()
        .map_err(|e| format!("_extract_text: Ctrl+C не удался: {}", e))?;

    // 4) Ждём, пока номер clipboard изменится — признак начала работы Ctrl+C.
    let mut seq_changed = false;
    for _ in 0..SEQ_POLL_ITERS {
        sleep(Duration::from_millis(POLL_DELAY_MS));
        let cur_clip_seq = unsafe { GetClipboardSequenceNumber() };
        if cur_clip_seq != init_clip_seq {
            seq_changed = true;
            break;
        }   // if
    }   // for

    if !seq_changed {
        // Clipboard не обновился за отведённое время.
        let _ = keyboard::send_vk_press(0x27); // VK_RIGHT — снять выделение (best effort)
        return Err("_extract_text: clipboard не обновился (sequence number не изменился)".to_string());
    }   // if

    // 5) Читаем clipboard в цикле, ожидая стабилизации длины текста.
    let mut result = String::new();
    let mut last_len: usize = 0;
    let mut stable_count: usize = 0;

    for _ in 0..LEN_POLL_ITERS {
        sleep(Duration::from_millis(POLL_DELAY_MS));

        // При ошибке чтения пропускаем итерацию.
        let cur = match clipboard::get_clipboard_text() {
            Ok(s) => s,
            Err(_) => continue,
        };

        let cur_len = cur.len();
        if cur_len == last_len {
            stable_count += 1;
            if stable_count >= STABLE_THRESHOLD {
                // Длина стабилизировалась — считаем текст полностью принятым.
                result = cur;
                break;
            }   // if
        } else {
            // Длина изменилась — сбрасываем счётчик стабильности.
            last_len = cur_len;
            stable_count = 1;
            result = cur;
        }   // if
    }   // for

    // 6) Снять выделение стрелкой вправо (best effort).
    let _ = keyboard::send_vk_press(0x27); // VK_RIGHT

    // Проверяем результат.
    if stable_count >= STABLE_THRESHOLD {
        Ok(result)
    } else {
        Err("_extract_text: длина текста в clipboard не стабилизировалась".to_string())
    }   // if
}   // _extract_text_from_focused_input()

/// Описание: Проверяет совпадение хвоста `text` с хвостом `text_expected`,
/// сравнивая строки с конца и игнорируя все пробельные символы.
///
/// # Правило
/// - Идём с конца `text_expected`, пропуская `char::is_whitespace()`.
/// - Для каждого значимого символа expected берём следующий значимый символ из `text` и сравниваем.
/// - При первом несовпадении возвращаем `false`.
/// - Если `text_expected` длинный, достаточно совпадения последних `max_chars` значимых символов.
///
/// # Параметры
/// - `text`: проверяемый текст.
/// - `text_expected`: образец
/// - `max_chars`: максимальное число проверяемых (с конца) символов.
///
/// # Возвращаемое значение
/// - `true`: если совпали все значимые символы expected (если их < max_chars),
///          либо совпали последние max_chars значимых символов expected.
/// - `false`: если `text` закончился раньше или найдено несовпадение.
pub(super) fn _tail_matches_expected_ignore_ws(text: &str, text_expected: &str, max_chars: usize) -> bool {

    // Итераторы “с конца”.
    let mut exp_it = text_expected.chars().rev();
    let mut cop_it = text.chars().rev();

    let mut matched: usize = 0;

    // Берём очередной “значимый” символ expected.
    while let Some(e_ch) = exp_it.find(|c| !c.is_whitespace()) {

        // Берём очередной “значимый” символ copied.
        let Some(c_ch) = cop_it.find(|c| !c.is_whitespace()) else {
            return false;
        };

        // Сравнение.
        if c_ch != e_ch {
            return false;
        }   // if

        matched += 1;

        // Достигли лимита — считаем, что проверка успешна.
        if matched >= max_chars {
            return true;
        }   // if
    }   // while

    // expected закончился раньше лимита — значит весь expected совпал.
    true
}   // _tail_matches_expected_ignore_ws()

/// Находит окно по подстроке заголовка (needle) ровно в одном экземпляре.
///
/// # Логика
/// - Вызывает `get_window_list(Some(needle), include_invisible=false, include_empty_title=true)`.
/// - Если найдено:
///   - 0 окон => Err
///   - 1 окно => Ok(WindowInfo)
///   - >1 окон => Err (двусмысленность)
///
/// # Возвращаемое значение
/// Тип: `WindowInfo`: Информация о найденном окне.
///
/// # Ошибки
/// Возвращает `Err(String)`, если окно не найдено или найдено более одного окна.
pub(super) fn _find_window_by_needle(needle: &str) -> Result<WindowInfo, String> {

    // Защита: пустая needle — программная/логическая ошибка вызова.
    if needle.trim().is_empty() {
        return Err("needle пустой: нечего искать".to_string());
    }   // if

    // Поиск делаем среди видимых окон. Невидимые обычно не имеют смысла для фокуса/вставки.
    // Пустые заголовки не матчнут needle, но оставляем include_empty_title=true для нейтральности.
    let mut list = get_window_list(Some(needle), false, true)?;

    if list.len() == 1 {

        // Возвращаем WindowInfo найденного окна (изымаем из вектора).
        Ok(list.remove(0))

    } else if list.is_empty() {

        Err(format!(
            "{}, {}: окно не найдено по подстроке в заголовке: '{}'",
            file!(), line!(), needle
        ))

    } else {

        Err(format!(
            "{}, {}: найдено по подстроке '{}' более одного ({}) окна",
            file!(), line!(), needle, list.len()
        ))

    }   // if
}   // _find_window_by_needle()

/// Описание: Возвращает расширенную информацию об окне.
///
/// # Параметры
/// - `hwnd`: Хэндл окна.
///
/// # Ошибки
/// Возвращает `Err(String)`, если hwnd == NULL (защита от программной ошибки).
pub(super) fn _get_window_info(hwnd: HWND) -> Result<WindowInfo, String> {

    if hwnd.0.is_null() {
        return Err("hwnd == NULL".to_string());
    }   // if

    // 1) Заголовок окна (может быть пустым — это нормально).
    let title = _get_window_title(hwnd);

    // 2) Признак foreground.
    let fg_hwnd = unsafe { GetForegroundWindow() };
    let is_foreground = !fg_hwnd.0.is_null() && fg_hwnd == hwnd;

    // 3) Признак minimized.
    let is_minimized = unsafe { IsIconic(hwnd).as_bool() };

    // 4) Геометрия окна в координатах виртуального рабочего стола, без теней.
    let mut rect = RECT::default();
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut _ as *mut _,
            size_of::<RECT>() as u32,
        ).map_err(|e| format!(
            "{}, {}: DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS) failed: {}", file!(), line!(), e
        ))?;
    }   // unsafe

    let w_i32 = rect.right - rect.left;
    let h_i32 = rect.bottom - rect.top;

    // Защита от мусорной геометрии/переполнений.
    if w_i32 < 0 || h_i32 < 0 {
        return Err(format!(
            "некорректный RECT: left={}, top={}, right={}, bottom={}",
            rect.left, rect.top, rect.right, rect.bottom
        ));
    }   // if

    let width = u32::try_from(w_i32)
        .map_err(|_| format!("width не укладывается в u32: {}", w_i32))?;

    let height = u32::try_from(h_i32)
        .map_err(|_| format!("height не укладывается в u32: {}", h_i32))?;

    Ok(WindowInfo {
        hwnd,
        title,
        is_foreground,
        is_minimized,
        x: rect.left,
        y: rect.top,
        width,
        height,
    })
}   // _get_window_info()

/// Читает заголовок окна (Win32 title bar text) как `String`.
///
/// # Алгоритм работы
/// - Получает длину заголовка через `GetWindowTextLengthW`.
/// - Выделяет буфер UTF-16 длиной `len + 1` (под завершающий `\0`).
/// - Читает текст через `GetWindowTextW`.
/// - Конвертирует в `String` через `from_utf16_lossy`.
///
/// # Возвращаемое значение
/// Заголовок окна. Может быть пустым.
///
/// # Побочные эффекты
/// - Нет.
pub(super) fn _get_window_title(hwnd: HWND) -> String {
    unsafe {
        // 1) Длина заголовка в UTF-16 код-юнитах (без завершающего '\0').
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }   // if

        // 2) Буфер +1 под '\0', который пишет GetWindowTextW.
        let mut buf: Vec<u16> = vec![0u16; (len as usize) + 1];

        // 3) copied — количество записанных символов без '\0'.
        let copied = GetWindowTextW(hwnd, &mut buf);
        if copied <= 0 {
            return String::new();
        }   // if

        // 4) Конвертация UTF-16 -> String.
        String::from_utf16_lossy(&buf[..(copied as usize)])
    }   // unsafe
}   // _get_window_title()

/// Одна попытка фокусировки окна (без циклических ретраев).
///
/// # Алгоритм работы
/// - Если окно уже foreground — успех.
/// - Если окно свернуто — восстанавливает (SW_RESTORE) и ждёт небольшую паузу.
/// - Вызывает `SetForegroundWindow`.
/// - Проверяет, что `GetForegroundWindow() == hwnd`.
///
/// # Ошибки
/// Возвращает `Err(String)`, если `SetForegroundWindow` не сработал или окно не стало foreground.
pub(super) fn _focus_window(hwnd: HWND) -> Result<(), String> {
    unsafe {
        // // Быстрый путь: окно уже в foreground.
        if GetForegroundWindow() == hwnd {
            return Ok(());
        }   // if

        // Если окно свернуто — восстановить. Это повышает шанс корректной фокусировки.
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);

            // Ждём, чтобы окно успело развернуться и обработать изменения.
            sleep(Duration::from_millis(200));
        }   // if

        // Попытаться перевести окно в foreground (может быть запрещено политикой Windows).
        let ok = SetForegroundWindow(hwnd);
        if !ok.as_bool() {
            return Err("SetForegroundWindow вернул FALSE (Windows мог запретить перевод в foreground)".to_string());
        }   // if
    }   // unsafe

    unsafe {
        // Верификация: foreground действительно поменялся на нужный hwnd.
        if GetForegroundWindow() == hwnd {
            Ok(())
        } else {
            Err(format!("окно \"{}\" не получило фокус", hwnd.0 as isize))
        }   // if
    }   // unsafe
}   // _focus_window()

/// Callback `EnumWindows`: собирает hwnd окон, фильтруя по `needle` и флагам include_*.
///
/// # ВАЖНО
/// Callback всегда возвращает TRUE, чтобы `EnumWindows` завершался штатно.
/// Ранний выход через FALSE в `windows-rs` может интерпретироваться как ошибка перечисления.
pub(super) extern "system" fn __find_by_needle_enum_windows_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {

    // Восстанавливаем указатель на контекст из LPARAM.
    let ctx_ptr = lparam.0 as *mut _FindWindowCtx;

    #[cfg(debug_assertions)]
    if ctx_ptr.is_null() {
        // В debug это сигнализирует о программной ошибке.
        return BOOL(0);
    }   // if

    // UNSAFE: разыменование указателя контекста.
    let ctx = unsafe { &mut *ctx_ptr };

    // 1) Фильтр по видимости.
    if !ctx.include_invisible {
        if !unsafe { IsWindowVisible(hwnd).as_bool() } {
            return BOOL(1);
        }   // if
    }   // if

    // 2) Заголовок нужен если:
    // - задан needle (нужно title.contains),
    // - или запрещены пустые заголовки (нужно проверить title.is_empty()).
    let need_title_check = ctx.needle.is_some() || !ctx.include_empty_title;

    if need_title_check {

        // Берём заголовок окна (может быть пустым).
        let title = _get_window_title(hwnd);

        // 2.1) Отсекаем пустые заголовки, если они запрещены.
        if !ctx.include_empty_title && title.is_empty() {
            return BOOL(1);
        }   // if

        // 2.2) Фильтрация по needle.
        if let Some(needle) = ctx.needle {
            if !title.contains(needle) {
                return BOOL(1);
            }   // if
        }   // if let Some

    }   // if need_title_check

    // 3) Окно удовлетворило фильтрам — добавляем в список.
    ctx.hwnd_vec.push(hwnd);

    // Продолжаем перечисление.
    BOOL(1)
}   // __find_by_needle_enum_windows_callback()

// Определение типа функции AttachThreadInput
type AttachThreadInputFn = unsafe extern "system" fn(u32, u32, bool) -> bool;

/// Ручная загрузка AttachThreadInput для обхода конфликтов версий.
/// Позволяет прицепиться к потоку окна для управления его фокусом.
pub(super) unsafe fn _manual_attach_thread_input(id_attach: u32, id_attach_to: u32, attach: bool) {
    // 1. Готовим имена библиотеки и функции в формате C-строк (с нулем на конце)
    let lib_name = CString::new("user32.dll").unwrap();
    let func_name = CString::new("AttachThreadInput").unwrap();

    // 2. LoadLibraryA: Просим Windows загрузить системную библиотеку user32.dll в память.
    // Если она уже загружена (а она загружена всегда), мы просто получаем её адрес (h_module).
    if let Ok(h_module) = unsafe { LoadLibraryA(windows::core::PCSTR(lib_name.as_ptr() as *const u8)) } {

        // 3. GetProcAddress: Ищем внутри этой библиотеки адрес нужной нам функции по имени.
        // Windows возвращает нам просто адрес в памяти (число).
        if let Some(addr) =
            unsafe { GetProcAddress(h_module, windows::core::PCSTR(func_name.as_ptr() as *const u8)) } {

            // 4. std::mem::transmute: Самая опасная магия.
            // Мы говорим Rust'у: "Смотри, вот этот адрес памяти `addr` — это на самом деле функция,
            // которая принимает (u32, u32, bool). Верь мне!"
            // Мы превращаем адрес в вызываемую функцию `func`.
            let func: AttachThreadInputFn = unsafe { std::mem::transmute(addr) };

            // 5. Вызываем функцию!
            unsafe { func(id_attach, id_attach_to, attach); }
        }   // if
    }   // if
}   // manual_attach_thread_input()
