//! window — Win32-утилиты поиска, фокусировки и инвентаризации окон.
//!
//! # ОПИСАНИЕ
//! Модуль содержит набор функций для работы с внешними top-level окнами Windows:
//! - получение заголовка окна (Win32 title bar text) через `GetWindowTextW`;
//! - перечисление видимых окон через `EnumWindows`;
//! - поиск окна по подстроке заголовка (needle);
//! - попытка вывести окно в foreground и дать ему фокус (best effort).
//!
//! Модуль используется агентом для:
//! - фокусировки вкладки/окна AI перед вставкой отчёта;
//! - поиска и фокусировки произвольных окон по запросу AI;
//! - диагностики: получение списка видимых окон.
//!
//! # ОТВЕТСТВЕННОСТЬ
//! 1. Перечисление окон:
//!    - `EnumWindows` + фильтр `IsWindowVisible`.
//!    - Опциональная фильтрация по `title.contains(needle)`.
//!
//! 2. Фокусировка окна:
//!    - восстановление свернутого окна (minimized) перед попыткой фокусировки;
//!    - вызов `SetForegroundWindow`;
//!    - проверка результата через `GetForegroundWindow` (polling с таймаутом).
//!
//! 3. Упаковка результата:
//!    - формирование структуры `WindowInfo` (hwnd, title и признаки состояния).
//!
//! # ИНВАРИАНТЫ
//! - Callback `EnumWindows` **не** делает “ранний выход” через `FALSE`, чтобы не ломать трактовку
//!   результата в `windows-rs`.
//! - Окна фильтруются по `IsWindowVisible`, чтобы не цеплять служебные/скрытые HWND.
//! - Фокусировка выполняется в цикле ретраев, т.к. окна (особенно браузеры) часто не готовы
//!   к foreground сразу после создания/обновления страницы.
//!
//! # ПРИМЕЧАНИЯ
//! - Успех `SetForegroundWindow` может блокироваться политикой Windows. Поэтому всегда есть верификация
//!   через `GetForegroundWindow` и повторы по таймеру.
//! - Заголовок окна может быть пустым — это нормальный случай для некоторых системных окон.mod test_window_test;

pub mod window_backend;
#[cfg(test)]
mod test_window_test;

use std::ffi::c_void;
use std::thread::sleep;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetForegroundWindow, GetGUIThreadInfo,
                                              GetWindowThreadProcessId, GUITHREADINFO,
                                              GUI_INMENUMODE, GUI_POPUPMENUMODE};
use crate::{clipboard, keyboard};
use crate::window::window_backend::{_FindWindowCtx, __find_by_needle_enum_windows_callback,
                                    _find_window_by_needle, _focus_window, _get_window_info,
                                    _get_window_title, _manual_attach_thread_input,
                                    _tail_matches_expected_ignore_ws};

/// В течение какого времени повторяем попытки найти/сфокусировать окно.
const TRYING_PERIOD_MS: u64 = 10_000;

/// Ожидание между попытками.
const RETRY_PERIOD_MS: u64 = 100;

/// Сводная информация о top-level окне Windows.
///
/// Значения — снимок состояния на момент вызова и могут устареть сразу после возврата.
///
/// # Назначение
/// Используется для:
/// - диагностики (список видимых окон),
/// - выдачи человеку понятного статуса (foreground/minimized),
/// - дальнейших действий по hwnd.
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub hwnd: HWND,            // Хэндл окна (HWND).
    pub title: String,         // Заголовок окна (Win32 title bar text).
    pub is_foreground: bool,   // Признак: окно сейчас в foreground.
    pub is_minimized: bool,    // Признак: окно свернуто (minimized).
    pub x: i32,                // X левого верхнего угла окна (виртуальный рабочий стол).
    pub y: i32,                // Y левого верхнего угла окна (виртуальный рабочий стол).
    pub width: u32,            // Ширина окна (в пикселях).
    pub height: u32,           // Высота окна (в пикселях).
}   // WindowInfo

/// Возвращает список top-level окон с фильтрацией по заголовку и видимости.
///
/// # Параметры
/// - `needle`: Опциональная подстрока для фильтрации по заголовку (contains).
/// - `include_invisible`: Включать невидимые окна.
/// - `include_empty_title`: Включать окна с пустым заголовком.
///
/// # Алгоритм работы
/// - Перечисляет окна через `EnumWindows`.
/// - Фильтрует окна согласно параметрам:
///   - по видимости (`IsWindowVisible`) если `include_invisible=false`,
///   - по пустому заголовку если `include_empty_title=false`,
///   - по `needle` если задан.
/// - Для каждого hwnd собирает `WindowInfo`.
///
/// # Возвращаемое значение
/// `Vec<WindowInfo>`: список окон (порядок — порядок `EnumWindows`).
///
/// # Ошибки
/// Возвращает `Err(String)`, если `EnumWindows` завершился с ошибкой.
pub fn get_window_list(needle: Option<&str>, include_invisible: bool, include_empty_title: bool)
    -> Result<Vec<WindowInfo>, String>
{
    // Контекст перечисления: callback использует его для фильтрации и накопления hwnd.
    let mut ctx = _FindWindowCtx {
        needle,
        include_invisible,
        include_empty_title,
        hwnd_vec: Vec::new(),
    };

    // EnumWindows заполнит ctx.hwnd_vec.
    unsafe {
        let lparam = LPARAM((&mut ctx as *mut _FindWindowCtx) as isize);
        EnumWindows(Some(__find_by_needle_enum_windows_callback), lparam)
            .map_err(|e| format!("{}, {}: EnumWindows() failed: {}", file!(), line!(), e))?;
    }   // unsafe

    // Превращаем HWND-ы в WindowInfo.
    let mut out: Vec<WindowInfo> = Vec::with_capacity(ctx.hwnd_vec.len());
    for hwnd in ctx.hwnd_vec {
        out.push(_get_window_info(hwnd)?);
    }   // for

    Ok(out)
}   // get_window_list()

/// Вставляет содержимое буфера обмена (Ctrl+V) в окно, найденное по `needle`.
///
/// # Алгоритм работы
/// - Находит окно по подстроке заголовка (needle).
/// - Пытается сфокусировать найденное окно.
/// - Отправляет комбинацию Ctrl+V в текущий фокус.
///
/// # Параметры
/// - `needle`: Подстрока заголовка окна (contains).
///
/// # Возвращаемое значение
/// Тип: `WindowInfo`: Информация об окне после фокусировки.
///
/// # Ошибки
/// Возвращает `Err(String)`, если окно не найдено/не удалось сфокусировать/не удалось послать Ctrl+V.
///
/// # Побочные эффекты
/// - Меняет foreground окно (фокус).
/// - Генерирует события клавиатуры (Ctrl+V).
pub fn paste_clipboard_into_window_by_needle(needle: &str) -> Result<WindowInfo, String> {

    // 1) Найти окно и гарантировать фокус.
    let wnd_info = find_window_by_needle_and_focus(needle)?;

    // 2) Вставить содержимое clipboard в текущий фокус.
    keyboard::send_ctrl_v()?;

    // 3) Небольшая пауза против гонок: UI может обработать Ctrl+V не мгновенно.
    sleep(Duration::from_millis(20));

    Ok(wnd_info)
}   // paste_clipboard_into_window_by_needle()

/// Кладёт `text` в буфер обмена и вставляет его (Ctrl+V) в окно, найденное по `needle`,
/// затем подтверждает вставку через Ctrl+A/Ctrl+C.
///
/// # Параметры
/// - `needle`: Подстрока заголовка окна (contains).
/// - `text`: Текст для вставки.
///
/// # Возвращаемое значение
/// Тип: `WindowInfo`: Информация об окне после фокусировки.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - окно не найдено / найдено более одного / не удалось сфокусировать,
/// - не удалось вставить текст или подтвердить вставку.
///
/// # Побочные эффекты
/// - Фокусирует целевое окно (best effort).
/// - Временно перезаписывает системный буфер обмена.
pub fn paste_text_into_window_by_needle(needle: &str, text: &str) -> Result<WindowInfo, String> {

    // 1) Найти окно и сфокусировать его.
    let wnd_info = find_window_by_needle_and_focus(needle)?;

    // 2) Вставить текст в текущее поле ввода (внутри этого окна) с верификацией.
    _paste_text_into_window_and_verify(wnd_info.hwnd, text, &wnd_info.title)?;

    Ok(wnd_info)
}   // paste_text_into_window_by_needle()

/// Кладёт `text` в буфер обмена и вставляет его (Ctrl+V) в окно по `hwnd`,
/// затем подтверждает вставку через Ctrl+A/Ctrl+C.
///
/// # Параметры
/// - `hwnd`: HWND целевого окна.
/// - `text`: Текст для вставки.
///
/// # Возвращаемое значение
/// Тип: `WindowInfo`: Информация об окне после фокусировки.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - не удалось сфокусировать окно,
/// - не удалось вставить текст или подтвердить вставку.
///
/// # Побочные эффекты
/// - Фокусирует целевое окно (best effort).
/// - Временно перезаписывает системный буфер обмена.
pub fn paste_text_into_window_by_hwnd(hwnd: HWND, text: &str) -> Result<WindowInfo, String> {

    // 1) Сфокусировать окно.
    let wnd_info = focus_window_with_retries(hwnd)?;

    // 2) Контекст для ошибок.
    let hwnd_dbg = format!("hwnd=0x{:X}", wnd_info.hwnd.0 as usize);

    // 3) Вставить текст в текущее поле ввода (внутри этого окна) с верификацией.
    _paste_text_into_window_and_verify(wnd_info.hwnd, text, &hwnd_dbg)?;

    Ok(wnd_info)
}   // paste_text_into_window_by_hwnd()

/// Извлекает текст из текущего сфокусированного поля ввода через `Ctrl+A` / `Ctrl+C`.
///
/// # Важно
/// Функция НЕ ищет окно и НЕ ставит фокус сама. Предполагается, что окно уже сфокусировано,
/// и курсор ввода находится в нужном поле с текстом.
///
/// # Алгоритм работы
/// 1. Получает информацию об активном (foreground) окне для контекста ошибок.
/// 2. Вызывает `window_backend::_extract_text_from_focused_input()`.
///
/// # Возвращаемое значение
/// `Result<String, String>`: `Ok(String)` с текстом или описание ошибки.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - нет активного окна;
/// - не удалось отправить клавиатурные команды (`Ctrl+A`, `Ctrl+C`);
/// - буфер обмена не обновился или длина текста не стабилизировалась.
///
/// # Побочные эффекты
/// - Сохраняет текстовый буфер обмена, но перетирает буфер любого другого типа.
/// - Генерирует события клавиатуры (`Ctrl+A`, `Ctrl+C`, стрелка вправо).
pub fn copy_text_from_focused_input() -> Result<String, String> {

    // Получаем контекст текущего окна (для диагностики)
    let wnd_info = get_foreground_window_info().map_err(|e| {
        format!("{}, {}: extract_text_from_focused_input: не удалось определить активное окно: {}",
                file!(), line!(), e)
    })?;

    // Сохраняем текущий clipboard (только текст, best effort).
    let prev_clip_text: Option<String> = clipboard::get_clipboard_text().ok();


    // Делегируем низкоуровневую логику бэкенду.
    let result = window_backend::_extract_text_from_focused_input().map_err(|e| {
        format!("{}, {}: ошибка извлечения текста в окне '{}' (hwnd=0x{:X}): {}",
                file!(), line!(), wnd_info.title, wnd_info.hwnd.0 as usize, e)
    });

    // Восстанавливаем clipboard (best effort, только текст).
    if let Some(prev) = prev_clip_text.as_deref() {
        let _ = clipboard::set_clipboard_text(prev);
    }   // if

    result
}   // copy_text_from_focused_input()

/// Кладёт `text` в буфер обмена и вставляет его (Ctrl+V) в текущее сфокусированное поле ввода,
/// затем подтверждает вставку через Ctrl+A/Ctrl+C.
///
/// # Важно
/// Функция НЕ ищет окно и НЕ ставит фокус сама. Предполагается, что окно уже сфокусировано,
/// и курсор ввода стоит в целевом поле.
///
/// # Параметры
/// - `text`: Текст для вставки.
///
/// # Возвращаемое значение
/// Тип: `WindowInfo`: Информация об активном окне, в которое была произведена вставка.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - нет активного окна;
/// - не удалось вставить текст или подтвердить вставку.
///
/// # Побочные эффекты
/// - Сохраняет текстовый буфер обмена, но перетирает буфер любого другого типа.
/// - Генерирует события клавиатуры (`Ctrl+V`, `Ctrl+A`, `Ctrl+C`, стрелка вправо).
pub fn paste_text_into_focused_input(text: &str) -> Result<WindowInfo, String> {

    // 1) Определяем активное окно.
    let wnd_info = get_foreground_window_info().map_err(|e| {
        format!("{}, {}: paste_text_into_focused_input: не удалось определить активное окно: {}",
                file!(), line!(), e)
    })?;

    let err_ctx = format!("{}, {}: title='{}', hwnd=0x{:X}", file!(), line!(),
                          wnd_info.title, wnd_info.hwnd.0 as usize);

    // 2) Сохраняем текущий clipboard (только текст, best effort).
    let prev_clip_text: Option<String> = clipboard::get_clipboard_text().ok();

    // 3) Вставляем текст с верификацией.
    let result = _paste_text_into_window_and_verify(wnd_info.hwnd, text, &err_ctx);

    // 4) Восстанавливаем clipboard (best effort, только текст).
    if let Some(prev) = prev_clip_text.as_deref() {
        let _ = clipboard::set_clipboard_text(prev);
    }   // if

    result.map(|_| wnd_info)
}   // paste_text_into_focused_input()

/// Эмулирует нажатие клавиши Enter в текущем сфокусированном поле ввода и верифицирует очистку поля.
///
/// # Важно
/// Функция НЕ ищет окно и НЕ ставит фокус сама. Предполагается, что окно уже сфокусировано,
/// и фокус ввода находится в нужном поле (которое должно очиститься после нажатия Enter).
///
/// # Алгоритм работы
/// 1. Сохраняет текущее текстовое содержимое буфера обмена.
/// 2. Захватывает начальное содержимое поля ввода (если не передано явно).
/// 3. Если поле пустое — возвращает ошибку (нечего отправлять).
/// 4. В цикле (в течение таймаута):
///    a. Нажимает Enter.
///    b. Нажимает Space (маркер для обнаружения пустого поля).
///    c. Извлекает текст из поля через `_extract_text_from_focused_input()`.
///    d. Если поле содержит только пробельные символы — Enter сработал:
///       нажимает Backspace (убрать маркерный пробел), восстанавливает clipboard, Ok.
///    e. Если поле всё ещё содержит текст — Enter не сработал:
///       убирает маркерный пробел (если добавился), ждёт, повторяет.
/// 5. Если таймаут исчерпан — восстанавливает clipboard и возвращает ошибку.
///
/// # Параметры
/// - `err_msg_context`: Контекст для текста ошибки (например, название окна).
/// - `initial_text`: Опциональное начальное содержимое поля ввода. Если `None`,
///   функция сама захватывает содержимое через Ctrl+A/Ctrl+C.
///   Передача `Some` позволяет избежать лишнего мигания поля ввода.
///
/// # Возвращаемое значение
/// `Result<(), String>`: `Ok(())` при успешной очистке поля, иначе текст ошибки.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - не удалось захватить начальное содержимое поля;
/// - поле изначально пустое (нечего отправлять);
/// - не удалось отправить нажатие клавиши;
/// - после всех попыток поле не очистилось.
///
/// # Побочные эффекты
/// - Временно перезаписывает системный буфер обмена (при верификации).
/// - Пытается восстановить буфер обмена, но только для текста.
/// - Генерирует события клавиатуры (Enter, Space, Backspace, Ctrl+A, Ctrl+C).
pub fn press_enter_and_verify(err_msg_context: &str, initial_text: Option<&str>)
    -> Result<(), String>
{
    /// Общий таймаут на все попытки нажатия Enter (мс).
    const TOTAL_TIMEOUT_MS: u64 = 30_000;

    /// Пауза между попытками (мс).
    const RETRY_DELAY_MS: u64 = 1000;

    // 1) Сохраняем текущий clipboard (только текст). Будет восстановлен при выходе.
    let prev_clip_text: Option<String> = clipboard::get_clipboard_text().ok();

    /// Восстановление текстового содержимого clipboard (best effort).
    fn _restore_clipboard(prev: &Option<String>) {
        if let Some(text) = prev.as_deref() {
            let _ = clipboard::set_clipboard_text(text);
        }   // if
    }   // _restore_clipboard()

    // 2) Захват начального содержимого поля ввода.
    //    Если передан явно — используем (экономим мигание Ctrl+A/Ctrl+C).
    //    Если не передан — разнюхиваем самостоятельно.
    let initial: String = match initial_text {
        Some(text) => text.to_string(),
        None => {
            match window_backend::_extract_text_from_focused_input() {
                Ok(text) => text,
                Err(e) => {
                    _restore_clipboard(&prev_clip_text);
                    return Err(format!(
                        "{}, {}: не удалось захватить начальное содержимое поля ввода ({}): {}",
                        err_msg_context, file!(), line!(), e
                    ));
                }
            }   // match
        }
    };  // match

    // 3) Если поле изначально пустое — нечего отправлять, нажимать Enter опасно.
    if initial.trim().is_empty() {
        _restore_clipboard(&prev_clip_text);
        return Err(format!(
            "{}, {}: поле ввода пустое, нажатие Enter отменено ({})",
            file!(), line!(), err_msg_context
        ));
    }   // if

    // 4) Цикл попыток: Enter → Space (маркер) → извлечение текста → анализ.
    let deadline = Instant::now() + Duration::from_millis(TOTAL_TIMEOUT_MS);

    while Instant::now() < deadline {

        // 4a) Нажимаем Enter.
        if let Err(e) = keyboard::send_enter() {
            _restore_clipboard(&prev_clip_text);
            return Err(format!(
                "{}, {}: не удалось отправить Enter ({}): {}", err_msg_context, file!(), line!(), e
            ));
        }   // if

        // 4b) Маркерный пробел. Нужен, чтобы _extract_text_from_focused_input смогла
        //     обнаружить изменение clipboard даже на очищенном поле.
        if let Err(e) = keyboard::send_vk_press(0x20) {  // VK_SPACE
            _restore_clipboard(&prev_clip_text);
            return Err(format!(
                "{}, {}: не удалось отправить маркерный пробел ({}): {}", err_msg_context, file!(), line!(), e
            ));
        }   // if

        // 4c) Извлекаем текст из поля ввода.
        let extract_result = window_backend::_extract_text_from_focused_input();

        match extract_result {

            Ok(current_text) if current_text.trim().is_empty() => {
                // 4d) Успех: поле содержит только пробельные символы — Enter сработал.
                //     Убираем маркерный пробел и выходим.
                let _ = keyboard::send_vk_press(0x08);  // VK_BACK (Backspace)
                _restore_clipboard(&prev_clip_text);
                return Ok(());
            }   // Ok, поле пустое

            Ok(current_text) => {
                // 4e) Enter не сработал: поле всё ещё содержит текст.
                //     Проверяем, добавился ли маркерный пробел: текущий текст оканчивается
                //     на пробел, а исходный — нет. Если добавился — убираем Backspace.
                let space_added = current_text.ends_with(' ') && !initial.ends_with(' ');
                if space_added {
                    let _ = keyboard::send_vk_press(0x08);  // VK_BACK (Backspace)
                }   // if

                // Пауза перед следующей попыткой.
                sleep(Duration::from_millis(RETRY_DELAY_MS));
            }   // Ok, поле не пустое

            Err(_) => {
                // 4f) Не удалось извлечь текст (возможно, фокус потерян).
                //     Ждём и пробуем снова.
                sleep(Duration::from_millis(RETRY_DELAY_MS));
            }   // Err
        }   // match
    }   // while

    // 5) Таймаут исчерпан: Enter так и не сработал за отведённое время.
    _restore_clipboard(&prev_clip_text);

    Err(format!(
        "{}, {}: не удалось подтвердить очистку поля ввода после нажатия Enter \
         в течение {} мс ({})", file!(), line!(), TOTAL_TIMEOUT_MS, err_msg_context
    ))
}   // press_enter_and_verify()

/// Находит окно по `needle` и пытается сфокусировать его.
///
/// # Алгоритм работы
/// - Ищет окно по `needle` (с ретраями).
/// - Пытается сфокусировать найденный hwnd (с ретраями).
///
/// # Параметры
/// - `needle`: Подстрока заголовка окна.
///
/// # Возвращаемое значение
/// Тип: `WindowInfo`: Информация об окне после попытки фокусировки.
///
/// # Ошибки
/// Возвращает `Err(String)`, если окно не найдено или фокусировка не удалась.
pub fn find_window_by_needle_and_focus(needle: &str) -> Result<WindowInfo, String> {

    // 1) Найти окно (с ретраями)
    let wnd_info = find_window_by_needle(needle)?;

    // 2) Сфокусировать найденное окно (с ретраями внутри focus_window)
    // Возвращаем результат именно функции focus_window, так как она собирает свежий WindowInfo.
    let res = focus_window_with_retries(wnd_info.hwnd);

    res

}   // find_window_by_needle_and_focus()

/// Находит окно по подстроке заголовка (needle) с ретраями по таймеру.
///
/// # Параметры
/// - `needle`: Подстрока заголовка окна.
///
/// # Возвращаемое значение
/// Тип: `WindowInfo`: Информация о найденном окне.
///
/// # Ошибки
/// Возвращает `Err(String)`, если окно не найдено за `TRYING_PERIOD_MS`.
pub fn find_window_by_needle(needle: &str) -> Result<WindowInfo, String> {

    let mut find_res = Err(format!("{}, {}: программная ошибка - ни одного цикла поиска окна.",
                                   file!(), line!()));

    // Цикл ретраев нужен, потому что окно может появиться не сразу (особенно браузер/веб).
    for _ in 0..TRYING_PERIOD_MS / RETRY_PERIOD_MS {

        // Пытаемся один проход перечисления окон.
        match _find_window_by_needle(needle) {
            Ok(wnd_info) => {

                // Успех: выходим.
                find_res = Ok(wnd_info);
                break;
            },
            Err(e) => {

                // Не нашли: ждём и пробуем снова.
                sleep(Duration::from_millis(RETRY_PERIOD_MS));
                find_res = Err(e);
            }
        }   // match
    }   // for

    find_res
}   // find_window_by_needle()

/// Пытается вывести окно в foreground и дать ему фокус (best effort) с ретраями.
///
/// # Алгоритм работы
/// - Читает заголовок окна (для сообщений об ошибках).
/// - Временно присоединяет ввод текущего потока к потоку окна (AttachThreadInput).
/// - В цикле ретраев вызывает `_focus_window(hwnd)`.
/// - При успехе заново собирает актуальный `WindowInfo` (геометрия могла измениться).
/// - Обязательно выполняет detach.
///
/// # Параметры
/// - `hwnd`: Хэндл окна.
///
/// # Возвращаемое значение
/// Тип: `WindowInfo`: Обновленная информация об окне после фокусировки.
///
/// # Ошибки
/// Возвращает `Err(String)`, если окно не стало foreground за `TRYING_PERIOD_MS`.
pub fn focus_window_with_retries(hwnd: HWND) -> Result<WindowInfo, String> {

    // Заголовок используем для диагностики (даже если он пустой).
    let win_title = _get_window_title(hwnd);

    // Подготовка AttachThreadInput: повышаем шанс фокусировки для чужого потока окна.
    let need_attach;
    let current_thread_id;
    let window_thread_id;
    unsafe {
        current_thread_id = GetCurrentThreadId();
        window_thread_id = GetWindowThreadProcessId(hwnd, None);
        need_attach = current_thread_id != window_thread_id;

        if need_attach {
            _manual_attach_thread_input(current_thread_id, window_thread_id, true);
        }   // if
    }   // unsafe

    // Цикл ретраев: окно может быть не готово к foreground немедленно.
    let mut focus_res = Err(format!("{}, {}: не удалось сфокусировать окно '{}' за отведенное время.",
                                    file!(), line!(), win_title));

    // Отдаем команду фокусировки. Мгновенного результата не ожидаем.
    let _ = _focus_window(hwnd);

    // Ждем перед окончательной фокусировкой. Если этого не делать, то окно будет сфокусировано, но
    // потом фокус может убежать.
    sleep(Duration::from_millis(300));

    // Окончательная  фокусировка.
    for _ in 0..TRYING_PERIOD_MS / RETRY_PERIOD_MS {
        match _focus_window(hwnd) {
            // Удалось. Собираем свежую информацию об окне и выходим.
            Ok(()) => {
                match _get_window_info(hwnd) {
                    Ok(info) => focus_res = Ok(info),
                    Err(e) => focus_res = Err(format!("{}, {}: фокус получен, но ошибка чтения инфо: {}",
                                                      file!(), line!(), e)),
                }
                break;
            },
            // Не удалось. Выжидаем и повторяем.
            Err(e) => {
                sleep(Duration::from_millis(RETRY_PERIOD_MS));
                focus_res = Err(e);
            }
        }   // match
    }   // for

    // Важно: detach делаем всегда, даже если фокусировка не удалась.
    unsafe {
        if need_attach {
            _manual_attach_thread_input(current_thread_id, window_thread_id, false);
        }   // if
    }   // unsafe

    focus_res
}   // focus_window()

/// Возвращает информацию об окне, которое сейчас находится в foreground.
///
/// # Алгоритм работы
/// - Вызывает `GetForegroundWindow()`.
/// - Если HWND == NULL — возвращает ошибку.
/// - Собирает расширенную информацию через `_get_window_info(hwnd)`.
///
/// # Возвращаемое значение
/// `WindowInfo`: hwnd, title и признаки состояния.
///
/// # Ошибки
/// Возвращает `Err(String)`, если foreground окно отсутствует (NULL).
pub fn get_foreground_window_info() -> Result<WindowInfo, String> {

    // 1) Получить HWND окна в foreground (может быть NULL).
    let hwnd = unsafe { GetForegroundWindow() };

    // 2) NULL означает, что foreground окно не определено.
    if hwnd.0.is_null() {
        return Err("GetForegroundWindow вернул NULL (foreground окно отсутствует)".to_string());
    }   // if

    // 3) Собрать расширенную информацию.
    let win_info = _get_window_info(hwnd);

    win_info
}   // get_foreground_window_info()

/// Описание: Парсит HWND из строки.
///
/// Поддерживает:
/// - hex: `"0x..."` / `"0X..."`
/// - decimal: `"123456"`
///
/// # Зачем это в library/window
/// HWND — это “оконная” сущность. Парсер нужен в нескольких хэндлерах:
/// - `capture_window_by_hwnd`
/// - `focus_window_by_hwnd`
/// - любые будущие команды, работающие с HWND.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - строка не является числом (hex/decimal),
/// - значение не укладывается в `isize` (формат HWND в windows-rs).
pub fn parse_hwnd(hwnd_str: &str) -> Result<HWND, String> {

    // 1) Убираем пробелы по краям — команды часто приходят “чуть грязными”.
    // Например: " 0x1234 " или "1234\r\n".
    let s = hwnd_str.trim();

    // 2) Парсим в u64.
    // - Если есть префикс 0x/0X — считаем, что это hex.
    // - Иначе — decimal.
    //
    // Почему u64:
    // - это удобный промежуточный тип для parse,
    // - позволяет корректно обработать как hex, так и decimal,
    // - после парсинга мы уже приводим к isize (формат HWND в windows-rs).
    let val_u64 = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {

        // 2.1) Hex формат: "0x1A2B3C"
        // strip_prefix возвращает только часть после "0x".
        u64::from_str_radix(hex, 16)
            .map_err(|e| format!("{}, {}: HWND: не удалось распарсить hex '{}': {}",
                                 file!(), line!(), s, e))?

    } else {

        // 2.2) Decimal формат: "123456"
        s.parse::<u64>()
            .map_err(|e| format!("{}, {}: HWND: не удалось распарсить decimal '{}': {}",
                                 file!(), line!(), s, e))?

    };   // if hex/decimal

    // 3) Приводим к isize.
    //
    // HWND в windows-rs представлен как HWND(isize).
    // На 64-bit Windows это обычно ок, но мы всё равно проверяем переполнение.
    let val_isize = isize::try_from(val_u64)
        .map_err(|_| format!("{}, {}: HWND: значение '{}' не укладывается в isize", file!(), line!(), s))?;

    // 4) Формируем HWND.
    Ok(HWND(val_isize as *mut c_void))
}   // parse_hwnd()

//--------------------------------------------------------------------------------------------------
//                  Внутренний интерфейс
//--------------------------------------------------------------------------------------------------

/// Кладёт `text` в буфер обмена и вставляет его (Ctrl+V) в текущее сфокусированное окно,
/// затем пытается подтвердить вставку через Ctrl+A/Ctrl+C.
///
/// # Важно
/// Функция НЕ ищет окно и НЕ ставит фокус сама.
/// Предполагается, что окно уже сфокусировано, и фокус ввода стоит в нужном поле.
///
/// # Механика вставки/копирования буфера.
/// При эмуляции нажатий клавиш они ставятся в очередь и выполняются асинхронно, но строго последовательно.
///
/// #Важно
/// Во время выполнения клавишных операций содержимое буфера менять нельзя, чтобы избежать гонки и
/// не нарушить их работу. Поэтому, например, функцию `_extract_text_from_focused_input()`
/// написана так, чтобы изменять буфер только через нажатия клавиш.
///
/// Алгоритм.
/// 1. Убирает pop-up, если он перекрывает поле ввода.
/// 2. Сохраняет текущее текстовое содержимое clipboard.
/// 3. Кладёт `text` в clipboard.
/// 4. Отправляет `Ctrl+V` (вставка).
/// 5. Извлекает текст из поля ввода через `_extract_text_from_focused_input()`.
/// 6. Сравнивает хвост извлечённого текста с `text` (с нормализацией пробельных символов).
/// 7. Восстанавливает прежнее текстовое содержимое clipboard.
///
/// # Параметры
/// - `foreground_hwnd`: хэндлер окна переднего плана.
/// - `text`: Текст для вставки.
/// - `err_msg_context`: Контекст для текста ошибки (например, title или hwnd=0x...).
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - не удалось записать текст в clipboard,
/// - не удалось отправить Ctrl+V,
/// - не удалось извлечь текст из поля ввода;
/// - извлечённый текст не совпал с ожидаемым.
///
/// # Побочные эффекты
/// - Временно перезаписывает системный буфер обмена.
/// - Пытается восстановить clipboard, но ТОЛЬКО в части текста:
///   если в clipboard было изображение/файлы/HTML, восстановить “как было” через arboard нельзя.
/// - Генерирует события клавиатуры (`Ctrl+V`, `Ctrl+A`, `Ctrl+C`, стрелка вправо).
fn _paste_text_into_window_and_verify(foreground_hwnd: HWND, text: &str, err_msg_context: &str)
    -> Result<(), String>
{
    /// Максимальное число значимых символов для сравнения хвостов.
    const VERIFY_TAIL_CHARS: usize = 100;

    // Убрать, если есть, pop-up поверх нашего поля ввода.
    _check_and_close_popup(foreground_hwnd)?;

    // 0) Сохраняем текущий clipboard (только текст).
    //
    // Важно:
    // - Если в буфере был НЕ текст (например, файлы/картинка), arboard может вернуть Err.
    // - В этом случае мы НЕ сможем восстановить clipboard “как было”.
    let prev_clip_text: Option<String> = clipboard::get_clipboard_text().ok();

    /// Локальная best-effort функция восстановления clipboard (только текст).
    fn _restore_clipboard_text(prev_clip_text: &Option<String>) {
        if let Some(prev) = prev_clip_text.as_deref() {
            let _ = clipboard::set_clipboard_text(prev);
        }   // if
    }   // _restore_clipboard_text()

    // 1) Кладём текст для вставки в clipboard.
    if let Err(e) = clipboard::set_clipboard_text(text) {
        _restore_clipboard_text(&prev_clip_text);
        return Err(e);
    }   // if

    // 2) Вставляем текст.
    if let Err(e) = keyboard::send_ctrl_v() {
        _restore_clipboard_text(&prev_clip_text);
        return Err(e);
    }   // if


    // 3) Извлекаем текст из поля ввода для верификации.
    //    _extract_text_from_focused_input() перезапишет clipboard (пустой строкой, затем Ctrl+C),
    //    поэтому восстановление clipboard делаем после неё.
    let extract_result = window_backend::_extract_text_from_focused_input();
    match extract_result {
        Ok(extracted) => {
            // 4) Сравниваем хвосты: извлечённый текст должен заканчиваться на `text`.
            let matched = _tail_matches_expected_ignore_ws(&extracted, text, VERIFY_TAIL_CHARS);

            _restore_clipboard_text(&prev_clip_text);

            if matched {
                Ok(())
            } else {
                Err(format!(
                    "{}, {}: не удалось подтвердить вставку текста в поле ввода окна ({})",
                    file!(), line!(), err_msg_context
                ))
            }   // if
        },

        Err(e) => {
            _restore_clipboard_text(&prev_clip_text);
            Err(format!(
                "{}, {}: не удалось извлечь текст из поля ввода для верификации ({}): {}",
                file!(), line!(), err_msg_context, e
            ))
        },
    }   // match
}   // _paste_text_into_window_and_verify()

/// Проверяет наличие активного pop-up и пытается закрыть его (ESC) с ожиданием результата.
///
/// # Алгоритм
/// 1. Проверяет `_is_popup_active`. Если `false` — выходит сразу.
/// 2. Отправляет нажатие `ESC`.
/// 3. В цикле (до 500мс) проверяет, исчезли ли признаки pop-up.
///
/// # Параметры
/// - `foreground_hwnd`: HWND главного окна.
///
/// # Ошибки
/// Возвращает `Err(String)` только при сбое отправки клавиш или WinAPI.
fn _check_and_close_popup(foreground_hwnd: HWND) -> Result<(), String> {

    // 1. Быстрая проверка
    if !_is_popup_active(foreground_hwnd)? {
        return Ok(());
    }   // if

    // 2. Отправка ESC
    keyboard::send_esc()?;

    // 3. Ожидание закрытия (Polling)
    // 10 попыток по 50 мс = 500 мс макс.
    for _ in 0..10 {
        sleep(Duration::from_millis(50));

        if !_is_popup_active(foreground_hwnd)? {
            // Успех: состояние нормализовалось
            return Ok(());
        }   // if
    }   // for

    Ok(())
}   // _check_and_close_popup_wait()

/// Вспомогательная внутренняя функция для проверки состояния GUI потока.
/// Проверяет, перехвачен ли фокус ввода или захват мыши всплывающим окном (pop-up) или меню.
///
/// Используется для диагностики состояния "паразитных" окон (контекстные меню, комбобоксы),
/// которые блокируют ввод в главное окно.
///
/// # Параметры
/// - `foreground_hwnd`: HWND окна, которое ожидается активным (главное окно).
///
/// # Возвращаемое значение
/// - `true`: Обнаружены признаки активного меню или захвата фокуса другим окном.
/// - `false`: Фокус и захват мыши принадлежат `foreground_hwnd` (или отсутствуют).
///
/// # Ошибки
/// Возвращает `Err(String)` при сбое WinAPI (например, `GetGUIThreadInfo`).
fn _is_popup_active(foreground_hwnd: HWND) -> Result<bool, String> {
    unsafe {
        // Защита от NULL
        if foreground_hwnd.0.is_null() {
            return Ok(false);
        }   // if

        // Получаем ID потока
        let thread_id = GetWindowThreadProcessId(foreground_hwnd, None);
        if thread_id == 0 {
            // Окно могло быть закрыто в процессе
            return Ok(false);
        }   // if

        let mut gui_info = GUITHREADINFO::default();
        gui_info.cbSize = size_of::<GUITHREADINFO>() as u32;

        if GetGUIThreadInfo(thread_id, &mut gui_info).is_err() {
            return Err("GetGUIThreadInfo failed".to_string());
        }   // if

        // 1. Флаги меню (стандартные Win32 меню)
        let in_menu_flag = (gui_info.flags & (GUI_INMENUMODE | GUI_POPUPMENUMODE)).0 != 0;

        // 2. Фокус ввода "угнан" (находится не в главном окне)
        let focus_hijacked = !gui_info.hwndFocus.0.is_null() && gui_info.hwndFocus != foreground_hwnd;

        // 3. Мышь захвачена другим окном (критично для кастомных меню, например в Chrome)
        let mouse_captured = !gui_info.hwndCapture.0.is_null() && gui_info.hwndCapture != foreground_hwnd;

        Ok(in_menu_flag || focus_hijacked || mouse_captured)
    }   // unsafe
}   // _is_popup_active()

