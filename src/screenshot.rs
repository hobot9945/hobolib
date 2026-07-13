//! screenshot.rs — Захват скриншотов через xcap.
//!
//! ОПИСАНИЕ:
//! Модуль предоставляет функции для захвата изображения всего “виртуального рабочего стола”
//! (все мониторы) в один RGBA-кадр.
//!
//! ОТВЕТСТВЕННОСТЬ:
//! - Получить список мониторов через xcap.
//! - Рассчитать общий bounding box по (x, y, width, height) всех мониторов.
//! - Захватить кадр каждого монитора и “склеить” их в один холст.
//! - (Опционально) сохранить результат в PNG.

pub mod image_grid;
mod capture_backend;
#[cfg(test)]
mod test_screenshot_test;

use std::path::Path;
use windows::Win32::Foundation::HWND;
use xcap::image::RgbaImage;
use crate::screenshot::capture_backend::{CursorInfo, MonitorGeometry};

/// Описание: Возвращает количество мониторов в логической нумерации.
///
/// # Возвращаемое значение
/// `usize`: количество мониторов.
///
/// # Ошибки
/// Возвращает `Err(String)`, если карта не построена.
pub fn logical_monitors_count() -> Result<usize, String> {
    Ok(logical_to_physical_map()?.len())
}   // logical_monitors_count()

/// Описание: Возвращает геометрию монитора по логическому индексу.
///
/// Логический индекс определяется сортировкой мониторов по (y, x): сверху вниз, затем слева направо
/// (как строки в книге).
///
/// # Параметры
/// - `logical_index`: Логический индекс монитора (начиная с 0).
///
/// # Возвращаемое значение
/// Тип: `MonitorGeometry`: (x, y, width, height) монитора.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - не удалось построить карту соответствия мониторов;
/// - `logical_index` выходит за пределы количества мониторов;
/// - не удалось получить x/y/width/height из xcap.
pub fn get_monitor_geometry(logical_index: usize)
    -> Result<MonitorGeometry, String>
{
    capture_backend::_get_monitor_geometry_by_logical_index(logical_index)
}

/// Описание: Возвращает ссылку на карту logical->physical.
///
/// # Возвращаемое значение
/// `&'static [usize]`: срез, где `map[logical] = physical`.
///
/// # Ошибки
/// Возвращает `Err(String)`, если не удалось получить список мониторов или координаты.
pub fn logical_to_physical_map() -> Result<&'static [usize], String> {
    match capture_backend::LOGICAL_TO_PHYSICAL_MAP.as_ref() {
        Ok(v) => Ok(v.as_slice()),
        Err(e) => Err(e.clone()),
    }
}   // logical_to_physical_map()

/// Описание: Захватывает прямоугольную область виртуального рабочего стола в RGBA-изображение.
///
/// Координаты задаются в системе виртуального рабочего стола (могут быть отрицательными,
/// если левый/верхний монитор имеет отрицательные координаты).
///
/// # Параметры
/// - `x`: X-координата левого верхнего угла области.
/// - `y`: Y-координата левого верхнего угла области.
/// - `width`: Ширина области в пикселях (должна быть > 0).
/// - `height`: Высота области в пикселях (должна быть > 0).
///
/// # Возвращаемое значение
/// Type: Result<(RgbaImage, CursorInfo), String>
/// - `Ok((img, cursor_info))`: RGBA-изображение области, `cursor_info` — положение hotspot
///   курсора относительно левого верхнего угла захваченной области.
/// - `Err(String)`: Текст ошибки.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - `width` или `height` равны 0;
/// - не удалось получить DC экрана или создать GDI-объекты;
/// - `BitBlt` не смог скопировать пиксели;
/// - `GetDIBits` вернул 0.
///
/// # Побочные эффекты
/// - Кратковременно создаёт и освобождает GDI-ресурсы.
pub fn capture_region_rgba(x: i32, y: i32, width: u32, height: u32)
                                  -> Result<(RgbaImage, CursorInfo), String>
{
    capture_backend::_capture_region_rgba(x, y, width, height)
}   // capture_region_rgba()

/// Описание: Делает RGBA-скриншот окна по HWND и возвращает изображение + информацию о курсоре.
///
/// Скриншот включает non-client область (рамки/заголовок), но не включает невидимые тени.
///
/// # Параметры
/// - `hwnd`: Дескриптор окна (HWND).
///
/// # Возвращаемое значение
/// Type: Result<(RgbaImage, CursorInfo), String>
/// - `Ok((img, cursor_info))`: `img` — RGBA изображение окна, `cursor_info` — координаты hotspot курсора
///   относительно левого верхнего угла окна.
/// - `Err(String)`: Текст ошибки.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - `DwmGetWindowAttribute` вернул некорректную геометрию;
/// - не удалось захватить пиксели через `PrintWindow`/`BitBlt`;
/// - не удалось сформировать `RgbaImage`.
///
/// # Побочные эффекты
/// - Нет.
pub fn capture_window_rgba(hwnd: HWND) -> Result<(RgbaImage, CursorInfo), String> {
    capture_backend::_capture_window_rgba(hwnd)
}   // capture_window_rgba()

/// Описание: Возвращает RGBA-изображение монитора с указанным логическим индексом.
///
/// Логический индекс определяется сортировкой мониторов по (y, x): сверху вниз, затем слева направо
/// (как строки в книге).
///
/// # Параметры
/// - `logical_index`: Логический индекс монитора (начиная с 0).
///
/// # Алгоритм работы
/// - Преобразует логический индекс в физический через ленивую карту.
/// - Захватывает изображение монитора по физическому индексу.
/// - Накладывает курсор и возвращает `CursorInfo`, положение курсора относительно верхнего левого
///   угла экрана.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - не удалось построить карту соответствия мониторов;
/// - логический индекс выходит за пределы количества мониторов;
/// - не удалось захватить изображение монитора.
pub fn capture_monitor_rgba(logical_index: usize) -> Result<(RgbaImage, CursorInfo), String> {
    capture_backend::_capture_monitor_rgba(logical_index)
}   // capture_monitor_rgba()

/// Описание: Возвращает RGBA-изображение всего видимого пространства (union всех мониторов).
///
/// # Алгоритм работы
/// - Берёт `Monitor::all()`.
/// - Считает bounding box по x/y/width/height (учитывая отрицательные координаты).
/// - Создаёт холст нужного размера.
/// - Для каждого монитора делает `capture_image()` и вставляет в холст по смещению.
/// - Накладывает курсор и возвращает `CursorInfo` относительно верхнего левого угла виртуального
///   экрана.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - не найдено ни одного монитора;
/// - не удалось получить геометрию монитора;
/// - не удалось захватить изображение монитора;
/// - bounding box некорректен/не укладывается в u32-размеры.
pub fn capture_all_monitors_rgba() -> Result<(RgbaImage, CursorInfo), String> {
    capture_backend::_capture_all_monitors_rgba()
}   // capture_all_monitors_rgba()

/// Описание: Снимает скриншот прямоугольной области экрана и помещает в буфер обмена.
///
/// Координаты задаются в системе виртуального рабочего стола (могут быть отрицательными,
/// если левый/верхний монитор имеет отрицательные координаты).
///
/// # Параметры
/// - `x`: X-координата левого верхнего угла области.
/// - `y`: Y-координата левого верхнего угла области.
/// - `width`: Ширина области в пикселях (должна быть > 0).
/// - `height`: Высота области в пикселях (должна быть > 0).
///
/// # Возвращаемое значение
/// Тип: Result<CursorInfo, String>: Информация о положении курсора относительно области.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - `width` или `height` равны 0;
/// - не удалось захватить область экрана;
/// - не удалось записать изображение в clipboard.
///
/// # Побочные эффекты
/// - Полностью перезаписывает системный буфер обмена.
pub fn capture_region_to_clipboard(x: i32, y: i32, width: u32, height: u32) -> Result<CursorInfo, String> {

    let (img, cursor_info) = capture_region_rgba(x, y, width, height)?;

    crate::clipboard::set_clipboard_image(img)?;

    Ok(cursor_info)
}   // capture_region_to_clipboard()

/// Описание: Снимает скриншот прямоугольной области экрана и сохраняет в PNG.
///
/// Координаты задаются в системе виртуального рабочего стола (могут быть отрицательными,
/// если левый/верхний монитор имеет отрицательные координаты).
///
/// # Параметры
/// - `x`: X-координата левого верхнего угла области.
/// - `y`: Y-координата левого верхнего угла области.
/// - `width`: Ширина области в пикселях (должна быть > 0).
/// - `height`: Высота области в пикселях (должна быть > 0).
/// - `path`: Путь для сохранения PNG-файла.
///
/// # Возвращаемое значение
/// Тип: Result<CursorInfo, String>: Информация о положении курсора относительно области.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - `width` или `height` равны 0;
/// - не удалось захватить область экрана;
/// - не удалось сохранить PNG.
///
/// # Побочные эффекты
/// - Пишет файл на диск.
pub fn capture_region_to_png(x: i32, y: i32, width: u32, height: u32, path: impl AsRef<Path>)
                             -> Result<CursorInfo, String>
{
    let (img, cursor_info) = capture_region_rgba(x, y, width, height)?;

    img.save(path.as_ref())
        .map_err(|e| format!(
            "screenshot: не удалось сохранить PNG '{}': {}",
            path.as_ref().display(), e
        ))?;

    Ok(cursor_info)
}   // capture_region_to_png()

/// Описание: Снимает скриншот окна по HWND и помещает в буфер обмена.
///
/// Скриншот включает non-client область (рамки, заголовок).
///
/// # Параметры
/// - `hwnd`: Дескриптор окна (HWND).
///
/// # Возвращаемое значение
/// Тип: Result<CursorInfo, String>: Информация о положении курсора относительно окна.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - `hwnd` невалидный или окно уничтожено;
/// - не удалось захватить скриншот окна;
/// - не удалось записать изображение в clipboard.
///
/// # Побочные эффекты
/// - Полностью перезаписывает системный буфер обмена.
pub fn capture_window_to_clipboard(hwnd: HWND) -> Result<CursorInfo, String> {

    let (img, cursor_info) = capture_window_rgba(hwnd)?;

    crate::clipboard::set_clipboard_image(img)?;

    Ok(cursor_info)
}   // capture_window_to_clipboard()

/// Описание: Снимает скриншот окна по HWND и сохраняет в PNG.
///
/// Скриншот включает non-client область (рамки, заголовок).
///
/// # Параметры
/// - `hwnd`: Дескриптор окна (HWND).
/// - `path`: Путь для сохранения PNG-файла.
///
/// # Возвращаемое значение
/// Тип: Result<CursorInfo, String>: Информация о положении курсора относительно окна.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - `hwnd` невалидный или окно уничтожено;
/// - не удалось захватить скриншот окна;
/// - не удалось сохранить PNG.
///
/// # Побочные эффекты
/// - Пишет файл на диск.
pub fn capture_window_to_png(hwnd: HWND, path: impl AsRef<Path>) -> Result<CursorInfo, String> {

    let (img, cursor_info) = capture_window_rgba(hwnd)?;

    img.save(path.as_ref())
        .map_err(|e| format!(
            "screenshot: не удалось сохранить PNG '{}': {}",
            path.as_ref().display(), e
        ))?;

    Ok(cursor_info)
}   // capture_window_to_png()

/// Описание: Снимает скриншот монитора с указанным логическим индексом и помещает в буфер обмена.
///
/// Логический индекс определяется позицией монитора на виртуальном рабочем столе:
/// мониторы упорядочиваются слева направо, затем сверху вниз.
///
/// # Параметры
/// - `logical_index`: Логический индекс монитора (начиная с 0).
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - не удалось захватить скриншот монитора;
/// - не удалось записать изображение в clipboard.
///
/// # Побочные эффекты
/// - Полностью перезаписывает системный буфер обмена.
pub fn capture_monitor_to_clipboard(monitor_index: usize) -> Result<CursorInfo, String> {

    // Захватываем изображение указанного монитора.
    let (img, cursor_info) = capture_monitor_rgba(monitor_index)?;

    // Помещаем в clipboard через общую функцию.
    crate::clipboard::set_clipboard_image(img)?;

    Ok(cursor_info)
}   // capture_monitor_to_clipboard()

/// Описание: Снимает скриншот монитора с указанным логическим индексом и сохраняет в PNG.
///
/// Логический индекс определяется позицией монитора на виртуальном рабочем столе:
/// мониторы упорядочиваются слева направо, затем сверху вниз.
///
/// # Параметры
/// - `logical_index`: Логический индекс монитора (начиная с 0).
/// - `path`: Путь для сохранения PNG-файла.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - не удалось захватить скриншот монитора;
/// - не удалось сохранить PNG.
///
/// # Побочные эффекты
/// - Пишет файл на диск.
pub fn capture_monitor_to_png(monitor_index: usize, path: impl AsRef<Path>) -> Result<CursorInfo, String> {

    // Захватываем изображение указанного монитора.
    let (img, cursor_info) = capture_monitor_rgba(monitor_index)?;

    // Сохраняем в PNG.
    img.save(path.as_ref())
        .map_err(|e| format!(
            "screenshot: не удалось сохранить PNG '{}': {}",
            path.as_ref().display(), e
        ))?;

    Ok(cursor_info)
}   // capture_monitor_to_png()

/// Описание: Делает скриншот всех мониторов и кладёт его в буфер обмена.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - не удалось захватить скриншот через xcap;
/// - не удалось записать изображение в clipboard.
///
/// # Побочные эффекты
/// - Полностью перезаписывает системный буфер обмена.
pub fn capture_all_monitors_to_clipboard() -> Result<CursorInfo, String> {
    let (img, cursor_info) = capture_all_monitors_rgba()?;
    crate::clipboard::set_clipboard_image(img)?;

    Ok(cursor_info)
}   // capture_all_monitors_to_clipboard_arboard()

/// Описание: Снимает скриншот всего видимого пространства (все мониторы) и сохраняет PNG.
///
/// Итоговое изображение — это общий прямоугольник (bounding box) всех мониторов.
/// Если между мониторами есть “пустоты”, они будут залиты фоном.
///
/// # Параметры
/// - `path`: Путь для сохранения PNG.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - xcap не смог получить мониторы или захватить изображение;
/// - не удалось склеить изображения;
/// - не удалось сохранить PNG.
///
/// # Побочные эффекты
/// - Пишет файл на диск.
pub fn capture_all_monitors_to_png(path: impl AsRef<Path>) -> Result<CursorInfo, String> {

    let (img, cursor_info) = capture_all_monitors_rgba()?;

    img.save(path.as_ref())
        .map_err(|e| format!(
            "screenshot: не удалось сохранить PNG '{}': {}",
            path.as_ref().display(), e
        ))?;

    Ok(cursor_info)
}   // capture_all_monitors_to_png()
