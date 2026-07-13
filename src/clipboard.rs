//! clipboard.rs — Утилиты работы с буфером обмена.
//!
//! Модуль предоставляет унифицированный интерфейс для работы с системным буфером обмена
//! через библиотеку arboard.
//!
//! # ОТВЕТСТВЕННОСТЬ
//! - Запись текста в буфер обмена.
//! - Запись RGBA-изображений в буфер обмена.
//!
//! # ИНВАРИАНТЫ
//! - Все операции используют единый механизм (arboard).
//! - Каждая запись полностью заменяет предыдущее содержимое clipboard.

use arboard::{Clipboard, ImageData};
use xcap::image::RgbaImage;
use std::ffi::CString;
use std::mem::size_of;
use std::ptr;
use std::thread;
use std::time::Duration;
use windows::core::PCSTR;
use windows::Win32::Foundation::{HANDLE, HWND, GlobalFree};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatA, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT,
};
use windows::Win32::System::Ole::CF_HDROP;
use windows::Win32::UI::Shell::DROPFILES;

/// Описание: Читает текст из системного буфера обмена.
///
/// # Возвращаемое значение
/// Тип: Result<String, String>
/// - `Ok(text)` — текст из clipboard.
/// - `Err(msg)` — если не удалось открыть буфер обмена или в нём нет/не читается текст.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - не удалось инициализировать доступ к clipboard (`Clipboard::new`);
/// - `arboard` не смог извлечь текст (`get_text`) — например, в буфере сейчас картинка/файлы.
pub fn get_clipboard_text() -> Result<String, String> {

    // Инициализируем доступ к системному clipboard.
    let mut clipboard = Clipboard::new()
        .map_err(|e| format!("arboard: Clipboard::new() failed: {}", e))?;

    // Читаем текст.
    clipboard
        .get_text()
        .map_err(|e| format!("arboard: get_text() failed: {}", e))

}   // get_clipboard_text()

/// Описание: Записывает текст в системный буфер обмена.
///
/// # Параметры
/// - `text`: Текст для записи.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - не удалось инициализировать clipboard;
/// - не удалось записать текст.
///
/// # Побочные эффекты
/// - Полностью перезаписывает системный буфер обмена.
pub fn set_clipboard_text(text: &str) -> Result<(), String> {

    // Инициализируем доступ к системному clipboard.
    let mut clipboard = Clipboard::new()
        .map_err(|e| format!("arboard: Clipboard::new() failed: {}", e))?;

    // Записываем текст в clipboard.
    clipboard
        .set_text(text)
        .map_err(|e| format!("arboard: set_text() failed: {}", e))?;

    Ok(())
}   // set_clipboard_text()

/// Описание: Помещает RGBA-изображение в системный буфер обмена.
///
/// # Параметры
/// - `image`: RGBA-изображение для записи в clipboard.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - не удалось инициализировать clipboard;
/// - не удалось записать изображение.
///
/// # Побочные эффекты
/// - Полностью перезаписывает системный буфер обмена.
pub fn set_clipboard_image(image: RgbaImage) -> Result<(), String> {

    // Извлекаем размеры изображения до того, как заберём владение данными.
    let width = image.width() as usize;
    let height = image.height() as usize;

    // Забираем сырые RGBA-байты (width * height * 4).
    // into_raw() потребляет image, возвращая Vec<u8>.
    let bytes = image.into_raw();    // Копирование в вектор. Владение надо забирать, карман хочет.

    // Формируем структуру данных для arboard.
    // bytes.into() конвертирует Vec<u8> в Cow<'static, [u8]>.
    let image_data = ImageData {
        width,
        height,
        bytes: bytes.into(),
    };

    // Инициализируем доступ к системному clipboard.
    let mut clipboard = Clipboard::new()
        .map_err(|e| format!("arboard: Clipboard::new() failed: {}", e))?;

    // Записываем изображение в clipboard.
    clipboard
        .set_image(image_data)
        .map_err(|e| format!("arboard: set_image() failed: {}", e))?;

    Ok(())
}   // set_clipboard_image()

/// Описание: Помещает список файлов в системный буфер обмена в формате `CF_HDROP`.
///
/// Функция эмулирует операцию "Копировать" или "Вырезать" для файлов в Проводнике Windows.
///
/// # Алгоритм работы
/// 1. Подготавливает единый буфер памяти, содержащий структуру `DROPFILES` и список путей.
/// 2. Пути кодируются в UTF-16 и разделяются нуль-терминаторами. Весь список завершается двойным нулём.
/// 3. Выделяет глобальную память (`GlobalAlloc`) с флагом `GMEM_MOVEABLE`.
/// 4. Копирует подготовленный буфер в глобальную память.
/// 5. Открывает буфер обмена с ретраями (защита от кратковременных блокировок другими приложениями).
/// 6. Очищает буфер обмена (`EmptyClipboard`).
/// 7. Передает владение глобальной памятью системе через `SetClipboardData(CF_HDROP)`.
/// 8. Если запрошена операция "Вырезать" (`is_cut=true`), дополнительно устанавливает формат
///    `Preferred DropEffect` со значением `DROPEFFECT_MOVE`.
/// 9. Закрывает буфер обмена.
///
/// # Параметры
/// - `paths`: Список абсолютных путей к файлам или каталогам.
/// - `is_cut`: Флаг операции перемещения.
///   - `true`: файлы будут "вырезаны" (при вставке произойдет перемещение).
///   - `false`: файлы будут скопированы.
///
/// # Возвращаемое значение
/// Тип: `Result<(), String>`
/// - `Ok(())` — файлы успешно помещены в буфер.
/// - `Err(String)` — описание ошибки (сбой выделения памяти, буфер занят и т.д.).
///
/// # Важные примечания по памяти
/// - Функция использует `unsafe` блоки для работы с WinAPI и сырыми указателями.
/// - Если `SetClipboardData` выполняется успешно, система забирает владение памятью (`HGLOBAL`).
///   В этом случае вызывать `GlobalFree` **НЕЛЬЗЯ**.
/// - В случае любой ошибки до успешного `SetClipboardData` память должна быть освобождена вручную через `GlobalFree`.
pub fn set_clipboard_files(paths: &[String], is_cut: bool) -> Result<(), String> {

    // 1. Валидация входных данных.
    if paths.is_empty() {
        return Err("set_clipboard_files: список путей пуст.".to_string());
    }   // if

    // 2. Подготовка буфера данных для CF_HDROP.
    // Структура в памяти: [DROPFILES struct] [path1\0] [path2\0] ... [pathN\0] [\0]

    // 2.1. Резервируем место под заголовок DROPFILES.
    let header_size = size_of::<DROPFILES>();
    let mut buffer: Vec<u8> = Vec::new();
    buffer.resize(header_size, 0);

    // 2.2. Записываем пути в конец буфера (UTF-16 LE).
    for path in paths {
        for u in path.encode_utf16() {
            // Добавляем 2 байта символа (little-endian).
            buffer.extend_from_slice(&u.to_ne_bytes());
        }
        // Нуль-терминатор после каждого пути.
        buffer.extend_from_slice(&0u16.to_ne_bytes());
    }   // for

    // 2.3. Финальный нуль-терминатор (double-null termination списка).
    buffer.extend_from_slice(&0u16.to_ne_bytes());

    unsafe {
        // 3. Аллокация глобальной памяти.
        // GMEM_MOVEABLE обязателен для clipboard.
        // GMEM_ZEROINIT для надежности (обнуляет память).
        let size = buffer.len();
        let h_global = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, size)
            .map_err(|e| format!("GlobalAlloc(CF_HDROP) failed: {}", e))?;

        // 4. Блокировка памяти для записи.
        let ptr = GlobalLock(h_global);
        if ptr.is_null() {
            let _ = GlobalFree(Some(h_global));
            return Err("GlobalLock(CF_HDROP) returned NULL".to_string());
        }   // if

        // 5. Заполнение структуры DROPFILES.
        let p_dropfiles = ptr as *mut DROPFILES;
        (*p_dropfiles).pFiles = header_size as u32; // Смещение к началу списка путей (сразу за заголовком).
        (*p_dropfiles).fWide = true.into();         // Указываем, что пути в Unicode (UTF-16).
        // Остальные поля (pt, fNC, fWide) оставляем 0/false (инициализированы GlobalAlloc).

        // 6. Копирование байтов путей.
        // Смещаемся на header_size и пишем данные путей.
        // Используем copy_nonoverlapping, так как области не пересекаются.
        let data_ptr = (ptr as *mut u8).add(header_size);
        let paths_data = &buffer[header_size..];
        ptr::copy_nonoverlapping(paths_data.as_ptr(), data_ptr, paths_data.len());

        // 7. Разблокировка памяти. Игнорируем результат разблокировки.
        let _ = GlobalUnlock(h_global);

        // 8. Открытие буфера обмена.
        // Делаем несколько попыток, так как буфер может быть кратковременно занят другим приложением.
        let mut opened = false;
        for _ in 0..5 {
            if OpenClipboard(Some(HWND::default())).is_ok() {
                opened = true;
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }   // for

        if !opened {
            let _ = GlobalFree(Some(h_global));
            return Err("Не удалось открыть буфер обмена (занят другим процессом)".to_string());
        }   // if

        // 9. Очистка буфера (обязательно для владения).
        if EmptyClipboard().is_err() {
            let _ = CloseClipboard();
            let _ = GlobalFree(Some(h_global));
            return Err("EmptyClipboard failed".to_string());
        }   // if

        // 10. Передача данных системе.
        // Если успешно — система забирает владение h_global. Мы его больше не освобождаем.
        let set_res = SetClipboardData(CF_HDROP.0 as u32, Some(HANDLE(h_global.0)));
        if set_res.is_err() {
            let _ = CloseClipboard();
            let _ = GlobalFree(Some(h_global)); // При ошибке освобождаем сами!
            return Err("SetClipboardData(CF_HDROP) failed".to_string());
        }   // if

        // 11. Установка Preferred DropEffect (опционально).
        // Если это операция "Вырезать", нужно добавить специальный формат.
        if is_cut {
            if let Err(e) = _set_preferred_drop_effect_move() {
                // Ошибка установки эффекта не фатальна для копирования файлов,
                // но операция "вырезать" превратится в "копировать".
                // Логируем в stderr (попадет в лог агента).
                eprintln!("Warning: Failed to set Preferred DropEffect: {}", e);
            }   // if
        }   // if is_cut

        // 12. Закрытие буфера.
        let _ = CloseClipboard();
    }   // unsafe

    Ok(())
}   // set_clipboard_files()

/// Вспомогательная функция: устанавливает формат `Preferred DropEffect` в `DROPEFFECT_MOVE`.
///
/// Вызывается только внутри `set_clipboard_files`, когда буфер обмена уже открыт.
///
/// # Алгоритм
/// 1. Регистрирует (или получает ID) формата буфера `"Preferred DropEffect"`.
/// 2. Выделяет глобальную память под 4 байта (`DWORD`).
/// 3. Записывает значение `2` (`DROPEFFECT_MOVE`).
/// 4. Передает память в буфер обмена через `SetClipboardData`.
fn _set_preferred_drop_effect_move() -> Result<(), String> {

    // 1. Получаем ID формата.
    let fmt_name = CString::new("Preferred DropEffect").unwrap();
    let cf_effect = unsafe { RegisterClipboardFormatA(PCSTR(fmt_name.as_ptr() as *const u8)) };
    if cf_effect == 0 {
        return Err("RegisterClipboardFormatA failed".to_string());
    }   // if

    // 2. Аллоцируем память (4 байта).
    let h_global = unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, size_of::<u32>()) }
        .map_err(|e| format!("GlobalAlloc(DropEffect) failed: {}", e))?;

    let ptr = unsafe { GlobalLock(h_global) };
    if ptr.is_null() {
        let _ = unsafe { GlobalFree(Some(h_global)) };
        return Err("GlobalLock(DropEffect) returned NULL".to_string());
    }   // if

    // 3. Записываем значение DROPEFFECT_MOVE (2).
    unsafe { *(ptr as *mut u32) = 2 };

    // Игнорируем результат разблокировки.
    let _ = unsafe { GlobalUnlock(h_global) };

    // 4. Передаем в clipboard.
    let set_res = unsafe { SetClipboardData(cf_effect, Some(HANDLE(h_global.0))) };
    if set_res.is_err() {
        let _ = unsafe { GlobalFree(Some(h_global)) }; // Освобождаем при неудаче.
        return Err("SetClipboardData(Preferred DropEffect) failed".to_string());
    }   // if

    Ok(())
}   // _set_preferred_drop_effect_move()