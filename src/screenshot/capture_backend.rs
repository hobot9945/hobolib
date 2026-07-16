//! capture_backend.rs — Захват изображений мониторов (GDI, xcap) + курсор.
//!
//! # ОПИСАНИЕ
//! Модуль предоставляет функции захвата скриншотов на базе `xcap`:
//! - захват **всех мониторов** с их склейкой в единый холст в координатах виртуального рабочего стола;
//! - захват **одного монитора** по логическому индексу;
//! - получение **геометрии монитора** по логическому индексу;
//! - вычисление и **наложение курсора** мыши на полученное изображение с корректным hotspot и альфа-блендингом.
//!
//! Модуль также формирует структуру `CursorInfo`, которая описывает положение hotspot курсора относительно
//! захваченного изображения и позволяет вывести понятную строку в отчёт.
//!
//! # ПРИМЕЧАНИЕ ПРО КУРСОР
//! Код получения состояния курсора и извлечения RGBA-изображения курсора находится в этом файле
//! в виде локального модуля `mouse_tool` (а не в `library::mouse`).
//! Причина: не смешивать “эмуляцию ввода” и “снимок экрана” в одном модуле.
//!
//! # ОТВЕТСТВЕННОСТЬ
//! 1. Приведение источников координат к общей системе:
//!    - координаты мониторов: `xcap::Monitor::{x, y, width, height}`;
//!    - координаты курсора + видимость + hcursor: `mouse_tool::get_cursor_state()`
//!      (локальный модуль внизу файла);
//!    - изображение курсора (RGBA + hotspot): `mouse_tool::get_cursor_rgba()`.//! 2. Склейка мониторов в общий RGBA-холст (для режима “все мониторы”).
//! 3. Наложение растрового изображения курсора (`get_cursor_rgba`) на скриншот.
//! 4. Поддержка логической нумерации мониторов (logical_index) через кэшируемую карту соответствия
//!    `logical_index -> physical_index`.
//!
//! # ИНВАРИАНТЫ
//! - Логическая нумерация мониторов определяется сортировкой по (y, x): сверху вниз, затем слева направо
//!   (как строки в книге).
//! - Карта `logical_index -> physical_index` строится лениво и кэшируется на время жизни процесса.
//!   При изменении конфигурации мониторов (подключение/отключение/перестановка) требуется перезапуск агента.
//! - `CursorInfo::x/y` — это координаты **hotspot** относительно левого верхнего угла изображения.
//!   Они могут выходить за пределы изображения (тогда `is_in_image=false`), но всё равно возвращаются.
//! - Любые ошибки работы `xcap` и извлечения курсора возвращаются как `Err(String)` без паники.
//!
//! # ПРИМЕЧАНИЯ
//! - Корректность совпадения пиксельной сетки скриншота и координат курсора зависит от DPI-awareness процесса.
//!   Если масштабирование Windows ≠ 100% и DPI-awareness не установлен, возможны рассинхронизации координат.
//!   DPI-awareness должен быть настроен на уровне процесса до вызовов захвата/координат.
//! - Цвет фона холста при склейке мониторов задаётся явно (сейчас: непрозрачный чёрный).
use ::xcap::image::{Rgba, RgbaImage};
use ::xcap::Monitor;
use xcap::image::GenericImage;

use std::sync::LazyLock;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC,
                                    DeleteObject, GetDC, GetDIBits, GetWindowDC, ReleaseDC, 
                                    SelectObject, BITMAPINFO, BITMAPINFOHEADER, 
                                    BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, SRCCOPY};
use crate::screenshot::MonitorGeometry;
// Локальный курсорный код находится в этом же файле (mouse_tool),
// поэтому импортируем его явно для читаемости overlay_cursor().
use self::mouse_tool::{get_cursor_rgba, get_cursor_state};

/// Информация о курсоре на скриншоте.
///
/// Описывает положение hotspot курсора относительно захваченного изображения.
#[derive(Debug, Clone)]
pub struct CursorInfo {
    /// Курсор был видим в момент захвата.
    /// `false` — курсор скрыт (приложение вызвало `ShowCursor(FALSE)`).
    pub is_visible: bool,

    /// Hotspot курсора попадает в область изображения.
    /// `false` — курсор за пределами захваченной области (но координаты всё равно заполнены).
    pub is_in_image: bool,

    /// X-координата hotspot относительно левого верхнего угла изображения.
    /// Может быть отрицательной (курсор левее изображения) или больше ширины (правее).
    pub x: i32,

    /// Y-координата hotspot относительно левого верхнего угла изображения.
    /// Может быть отрицательной (курсор выше изображения) или больше высоты (ниже).
    pub y: i32,
}   // CursorInfo

impl CursorInfo {

    /// Создаёт CursorInfo для невидимого курсора.
    pub(super) fn hidden() -> Self {
        Self {
            is_visible: false,
            is_in_image: false,
            x: 0,
            y: 0,
        }
    }   // hidden()

    /// Создаёт CursorInfo из результата overlay_cursor().
    ///
    /// # Параметры
    /// - `is_in_image`: Попал ли hotspot в область изображения.
    /// - `x`, `y`: Координаты hotspot относительно изображения.
    pub(super) fn visible(is_in_image: bool, x: i32, y: i32) -> Self {
        Self {
            is_visible: true,
            is_in_image,
            x,
            y,
        }
    }   // visible()

    /// Форматирует информацию о курсоре для отчёта AI.
    ///
    /// # Примеры вывода
    /// - `"курсор: (150, 200)"` — курсор внутри изображения.
    /// - `"курсор: (-50, 100) [вне изображения]"` — курсор за пределами.
    /// - `"курсор: скрыт"` — курсор невидим.
    pub fn report(&self) -> String {
        if !self.is_visible {
            "Курсор: скрыт".to_string()
        } else if self.is_in_image {
            format!("Курсор: ({}, {})", self.x, self.y)
        } else {
            format!("Курсор: ({}, {}) [вне изображения]", self.x, self.y)
        }
    }   // report()
}   // impl CursorInfo

//--------------------------------------------------------------------------------------------------
//                  Карта logical_index -> physical_index (кэшируется лениво)
//--------------------------------------------------------------------------------------------------

/// Карта соответствия логических индексов мониторов физическим.
///
/// Логический индекс определяется сортировкой мониторов по (y, x): сверху вниз, затем слева направо
/// (как строки в книге).
///
/// ВАЖНО:
/// - Карта строится один раз на процесс.
/// - Если раскладка/набор мониторов изменились во время работы процесса — карта не обновится.
///   В этом случае агент нужно перезапустить.
pub(super) static LOGICAL_TO_PHYSICAL_MAP: LazyLock<Result<Vec<usize>, String>> = LazyLock::new(|| {
    _build_logical_to_physical_map()
});

//--------------------------------------------------------------------------------------------------
//                  Функции захвата
//--------------------------------------------------------------------------------------------------
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
pub(super) fn _capture_region_rgba(x: i32, y: i32, width: u32, height: u32)
                                   -> Result<(RgbaImage, CursorInfo), String>
{
    if width == 0 || height == 0 {
        return Err("screenshot: width и height должны быть > 0".to_string());
    }   // if

    // 1) Захват пикселей области.
    let mut img = _capture_screen_region_rgba(x, y, width, height)?;

    // 2) Наложение курсора (origin = левый верхний угол области).
    let cursor_info = _overlay_cursor(&mut img, x, y)?;

    Ok((img, cursor_info))
}   // capture_screen_region_rgba()

/// Описание: Делает RGBA-скриншот окна по HWND и возвращает изображение + информацию о курсоре.
/// Скриншот захватывает только видимую часть окна (без DWM-теней). Границы определяются
/// через `DwmGetWindowAttribute` с `DWMWA_EXTENDED_FRAME_BOUNDS`. Пиксели захватываются
/// как область экрана через `GetDC(NULL)` + `BitBlt` (делегирование в `_capture_screen_region_rgba`).
///
/// # Ограничения
/// - Если окно перекрыто другими окнами или частично за пределами экрана — в этих местах
///   будут пиксели перекрывающих окон или фон рабочего стола.
/// - Некоторые приложения (игры, видеоплееры с hardware overlay) могут отдавать чёрный
///   прямоугольник вместо реального содержимого.
///
/// # Параметры
/// - `hwnd`: Дескриптор окна (HWND).
///
/// # Возвращаемое значение
/// Type: Result<(RgbaImage, CursorInfo), String>
/// - `Ok((img, cursor_info))`: `img` — RGBA изображение видимой части окна,
///   `cursor_info` — координаты hotspot курсора относительно левого верхнего угла изображения.
/// - `Err(String)`: Текст ошибки.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - `DwmGetWindowAttribute` не смог получить видимые границы окна;
/// - размеры окна некорректны (≤ 0 или не укладываются в u32);
/// - не удалось захватить пиксели области экрана;
/// - не удалось сформировать `RgbaImage`.
///
/// # Побочные эффекты
/// - Кратковременно создаёт и освобождает GDI-ресурсы (внутри `_capture_screen_region_rgba`).
pub(super) fn _capture_window_rgba(hwnd: HWND) -> Result<(RgbaImage, CursorInfo), String> {

    // 1) Видимые границы окна (без DWM-теней).
    let ext_rect = _get_extended_frame_bounds(hwnd)?;

    let w_i32 = ext_rect.right - ext_rect.left;
    let h_i32 = ext_rect.bottom - ext_rect.top;

    if w_i32 <= 0 || h_i32 <= 0 {
        return Err(format!(
            "screenshot: некорректный размер окна: left={}, top={}, right={}, bottom={}",
            ext_rect.left, ext_rect.top, ext_rect.right, ext_rect.bottom
        ));
    }   // if

    let width = u32::try_from(w_i32)
        .map_err(|_| format!("screenshot: width не укладывается в u32: {}", w_i32))?;

    let height = u32::try_from(h_i32)
        .map_err(|_| format!("screenshot: height не укладывается в u32: {}", h_i32))?;

    // 2) Захват области экрана по видимым границам окна.
    //    Делегируем в _capture_screen_region_rgba, которая использует GetDC(NULL) + BitBlt.
    let mut img = _capture_screen_region_rgba(ext_rect.left, ext_rect.top, width, height)?;

    // 3) Наложение курсора (origin = левый верх видимой части окна).
    let cursor_info = _overlay_cursor(&mut img, ext_rect.left, ext_rect.top)?;

    Ok((img, cursor_info))
}   // _capture_window_rgba()

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
pub(super) fn _capture_monitor_rgba(logical_index: usize) -> Result<(RgbaImage, CursorInfo), String> {

    // 1) logical -> physical
    let physical_index = _physical_index_by_logical_index(logical_index)?;

    // 2) Получаем список всех доступных мониторов.
    let monitors = Monitor::all()
        .map_err(|e| format!("xcap: Monitor::all() failed: {}", e))?;

    let monitor = monitors.get(physical_index).ok_or_else(|| {
        format!(
            "screenshot: physical_index={} выходит за пределы массива Monitor::all() (len={})",
            physical_index, monitors.len()
        )
    })?;

    // 3) Захватываем изображение.
    let mut img = monitor.capture_image()
        .map_err(|e| format!(
            "xcap: capture_image() failed (logical_index={}, physical_index={}): {}",
            logical_index, physical_index, e
        ))?;

    // 4) Координаты монитора на виртуальном экране.
    let mon_x = monitor.x().map_err(|e| format!(
        "xcap: Monitor::x() failed (logical_index={}, physical_index={}): {}",
        logical_index, physical_index, e
    ))?;

    let mon_y = monitor.y().map_err(|e| format!(
        "xcap: Monitor::y() failed (logical_index={}, physical_index={}): {}",
        logical_index, physical_index, e
    ))?;

    // 5) Курсор.
    let cursor_info = _overlay_cursor(&mut img, mon_x, mon_y)?;

    Ok((img, cursor_info))
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
pub(super) fn _capture_all_monitors_rgba() -> Result<(RgbaImage, CursorInfo), String> {

    let monitors = Monitor::all()
        .map_err(|e| format!("xcap: Monitor::all() failed: {}", e))?;

    if monitors.is_empty() {
        return Err("xcap: не найдено ни одного монитора".to_string());
    }   // if

    // --- 1) Bounding box в координатах “виртуального рабочего стола” ---
    // Важно: на Windows мониторы могут иметь отрицательные x/y.
    let mut min_x: i32 = i32::MAX;
    let mut min_y: i32 = i32::MAX;
    let mut max_x: i32 = i32::MIN;
    let mut max_y: i32 = i32::MIN;

    for (idx, m) in monitors.iter().enumerate() {

        let x = m.x().map_err(|e| format!("xcap: Monitor::x() failed (idx={}): {}", idx, e))?;
        let y = m.y().map_err(|e| format!("xcap: Monitor::y() failed (idx={}): {}", idx, e))?;
        let w = m.width().map_err(|e| format!("xcap: Monitor::width() failed (idx={}): {}", idx, e))? as i32;
        let h = m.height().map_err(|e| format!("xcap: Monitor::height() failed (idx={}): {}", idx, e))? as i32;

        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + w);
        max_y = max_y.max(y + h);
    }   // for

    // Ширина и высота виртуального монитора.
    let total_w_i32 = max_x - min_x;
    let total_h_i32 = max_y - min_y;

    if total_w_i32 <= 0 || total_h_i32 <= 0 {
        return Err(format!(
            "screenshot: некорректный bounding box: min=({},{}), max=({},{})",
            min_x, min_y, max_x, max_y
        ));
    }   // if

    let total_w = u32::try_from(total_w_i32).map_err(|_| {
        format!("screenshot: bounding box width не укладывается в u32: {}", total_w_i32)
    })?;

    let total_h = u32::try_from(total_h_i32).map_err(|_| {
        format!("screenshot: bounding box height не укладывается в u32: {}", total_h_i32)
    })?;

    // --- 2) Холст (фон) ---
    // Фон можно поменять. Сейчас — непрозрачный чёрный.
    let mut canvas = RgbaImage::from_pixel(total_w, total_h, Rgba([0, 0, 0, 255]));

    // --- 3) Склейка ---
    for (idx, m) in monitors.iter().enumerate() {

        let x = m.x().map_err(|e| format!("xcap: Monitor::x() failed (idx={}): {}", idx, e))?;
        let y = m.y().map_err(|e| format!("xcap: Monitor::y() failed (idx={}): {}", idx, e))?;

        let off_x = u32::try_from(x - min_x).map_err(|_| {
            format!("screenshot: off_x не укладывается в u32 (idx={}): x={}, min_x={}", idx, x, min_x)
        })?;

        let off_y = u32::try_from(y - min_y).map_err(|_| {
            format!("screenshot: off_y не укладывается в u32 (idx={}): y={}, min_y={}", idx, y, min_y)
        })?;

        let img = m.capture_image()
            .map_err(|e| format!("xcap: capture_image() failed (idx={}): {}", idx, e))?;

        canvas.copy_from(&img, off_x, off_y)
            .map_err(|e| format!("screenshot: copy_from failed (idx={}): {}", idx, e))?;
    }   // for

    // --- 4) Курсор ---
    let cursor_info = _overlay_cursor(&mut canvas, min_x, min_y)?;

    Ok((canvas, cursor_info))
}   // capture_all_monitors_rgba()

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
pub fn _get_monitor_geometry_by_logical_index(logical_index: usize)
                                                     -> Result<MonitorGeometry, String>
{
    let physical_index = _physical_index_by_logical_index(logical_index)?;

    let monitors = Monitor::all()
        .map_err(|e| format!("xcap: Monitor::all() failed: {}", e))?;

    let m = monitors.get(physical_index).ok_or_else(|| {
        format!(
            "screenshot: physical_index={} выходит за пределы массива Monitor::all() (len={})",
            physical_index, monitors.len()
        )
    })?;

    let x = m.x().map_err(|e| format!(
        "xcap: Monitor::x() failed (logical_index={}, physical_index={}): {}",
        logical_index, physical_index, e
    ))?;

    let y = m.y().map_err(|e| format!(
        "xcap: Monitor::y() failed (logical_index={}, physical_index={}): {}",
        logical_index, physical_index, e
    ))?;

    let width = m.width().map_err(|e| format!(
        "xcap: Monitor::width() failed (logical_index={}, physical_index={}): {}",
        logical_index, physical_index, e
    ))?;

    let height = m.height().map_err(|e| format!(
        "xcap: Monitor::height() failed (logical_index={}, physical_index={}): {}",
        logical_index, physical_index, e
    ))?;

    Ok(MonitorGeometry { x, y, width, height })
}   // get_monitor_geometry_by_logical_index()

//--------------------------------------------------------------------------------------------------
//                  Внутренние утилиты
//--------------------------------------------------------------------------------------------------

/// Описание: Получает видимые границы окна через DWM API (без невидимых теней).
///
/// `GetWindowRect` возвращает границы с учётом невидимых теней (DWM shadows),
/// которые на Windows 10/11 обычно составляют 7-8 пикселей с каждой стороны (кроме верхней).
/// `DwmGetWindowAttribute` с флагом `DWMWA_EXTENDED_FRAME_BOUNDS` возвращает
/// только видимую часть окна, исключая тени.
///
/// # Параметры
/// - `hwnd`: Дескриптор окна.
///
/// # Возвращаемое значение
/// - `Ok(RECT)`: Видимые границы окна (без теней).
///
/// # Ошибки
/// Возвращает `Err(String)`, если DWM API недоступен или вызов не удался.
fn _get_extended_frame_bounds(hwnd: HWND) -> Result<RECT, String> {
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};

    let mut ext_rect = RECT::default();

    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut ext_rect as *mut RECT as *mut _,
            size_of::<RECT>() as u32,
        ).map_err(|e| format!(
            "{}, {}: DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS) failed: {}", file!(), line!(), e
        ))?;
    }   // unsafe

    Ok(ext_rect)
}   // _get_extended_frame_bounds()

/// Описание: Захватывает прямоугольную область экрана в RGBA (top-down), без курсора.
///
/// Использует `GetDC(NULL)` для получения DC всего виртуального рабочего стола,
/// затем `BitBlt` для копирования нужного прямоугольника.
///
/// # Параметры
/// - `x`: X-координата левого верхнего угла (в координатах виртуального рабочего стола).
/// - `y`: Y-координата левого верхнего угла.
/// - `width`: Ширина области в пикселях.
/// - `height`: Высота области в пикселях.
///
/// # Возвращаемое значение
/// Type: Result<RgbaImage, String>: RGBA-изображение области.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - не удалось получить DC экрана (`GetDC(NULL)` вернул NULL);
/// - не удалось создать совместимые GDI-объекты;
/// - `BitBlt` не смог скопировать пиксели;
/// - `GetDIBits` вернул 0.
///
/// # Побочные эффекты
/// - Кратковременно создаёт и освобождает GDI-ресурсы.
fn _capture_screen_region_rgba(x: i32, y: i32, width: u32, height: u32) -> Result<RgbaImage, String> {

    unsafe {

        let w_i32 = width as i32;
        let h_i32 = height as i32;

        // ─────────────────────────────────────────────────────────────────────────────────────
        // ШАГ 1: Получение DC всего виртуального рабочего стола.
        // ─────────────────────────────────────────────────────────────────────────────────────
        // GetDC(NULL) возвращает DC, покрывающий все мониторы как единое пространство.
        // Координаты (x, y) могут быть отрицательными, если есть мониторы слева/сверху
        // от "главного" (primary) монитора.
        let screen_dc: HDC = GetDC(None);
        if screen_dc.0.is_null() {
            return Err("screenshot: GetDC(NULL) вернул NULL".to_string());
        }   // if

        // ─────────────────────────────────────────────────────────────────────────────────────
        // ШАГ 2: Создание memory DC для off-screen рендеринга.
        // ─────────────────────────────────────────────────────────────────────────────────────
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        if mem_dc.0.is_null() {
            let _ = ReleaseDC(None, screen_dc);
            return Err("screenshot: CreateCompatibleDC() failed".to_string());
        }   // if

        // ─────────────────────────────────────────────────────────────────────────────────────
        // ШАГ 3: Создание bitmap для хранения пикселей.
        // ─────────────────────────────────────────────────────────────────────────────────────
        let bmp: HBITMAP = CreateCompatibleBitmap(screen_dc, w_i32, h_i32);
        if bmp.0.is_null() {
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);
            return Err("screenshot: CreateCompatibleBitmap() failed".to_string());
        }   // if

        // ─────────────────────────────────────────────────────────────────────────────────────
        // ШАГ 4: Выбор bitmap в memory DC.
        // ─────────────────────────────────────────────────────────────────────────────────────
        let old = SelectObject(mem_dc, bmp.into());
        if old.0.is_null() {
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);
            return Err("screenshot: SelectObject() failed".to_string());
        }   // if

        // ─────────────────────────────────────────────────────────────────────────────────────
        // ШАГ 5: Копирование пикселей области экрана в bitmap.
        // ─────────────────────────────────────────────────────────────────────────────────────
        // BitBlt копирует прямоугольник из screen_dc (весь экран) в mem_dc (наш bitmap).
        // Источник: координаты (x, y) на экране.
        // Назначение: координаты (0, 0) в bitmap.
        if BitBlt(mem_dc, 0, 0, w_i32, h_i32, Some(screen_dc), x, y, SRCCOPY).is_err() {
            let _ = SelectObject(mem_dc, old);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);

            return Err("screenshot: BitBlt не смог захватить область экрана".to_string());
        }   // if

        // ─────────────────────────────────────────────────────────────────────────────────────
        // ШАГ 6: Подготовка BITMAPINFO для извлечения пикселей.
        // ─────────────────────────────────────────────────────────────────────────────────────
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w_i32,
            biHeight: -h_i32, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };

        let mut bgra: Vec<u8> = vec![0u8; (width as usize) * (height as usize) * 4];

        // ─────────────────────────────────────────────────────────────────────────────────────
        // ШАГ 7: Извлечение пикселей из bitmap.
        // ─────────────────────────────────────────────────────────────────────────────────────
        let scanlines = GetDIBits(
            mem_dc,
            bmp,
            0,
            height,
            Some(bgra.as_mut_ptr() as *mut _),
            &mut bi,
            DIB_RGB_COLORS,
        );

        // ─────────────────────────────────────────────────────────────────────────────────────
        // ШАГ 8: Освобождение GDI-ресурсов.
        // ─────────────────────────────────────────────────────────────────────────────────────
        let _ = SelectObject(mem_dc, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, screen_dc);

        if scanlines == 0 {
            return Err("screenshot: GetDIBits() вернул 0".to_string());
        }   // if

        // ─────────────────────────────────────────────────────────────────────────────────────
        // ШАГ 9: Конвертация BGRA → RGBA + нормализация альфы.
        // ─────────────────────────────────────────────────────────────────────────────────────
        for px in bgra.chunks_exact_mut(4) {
            let b = px[0];
            let r = px[2];
            px[0] = r;
            px[2] = b;
            px[3] = 255;
        }   // for

        // ─────────────────────────────────────────────────────────────────────────────────────
        // ШАГ 10: Создание RgbaImage.
        // ─────────────────────────────────────────────────────────────────────────────────────
        let img = RgbaImage::from_raw(width, height, bgra)
            .ok_or_else(|| "screenshot: RgbaImage::from_raw() failed".to_string())?;

        Ok(img)
    }   // unsafe
}   // _capture_screen_region_rgba()

/// Описание: Захватывает пиксели окна в RGBA-изображение (top-down), без наложения курсора.
///
/// Функция использует GDI для получения содержимого окна:
/// 1. Получает Device Context (DC) окна через `GetWindowDC` — это даёт доступ ко всей
///    области окна, включая non-client часть (рамки, заголовок, кнопки управления).
/// 2. Создаёт совместимый memory DC и bitmap для off-screen рендеринга.
/// 3. Копирует пиксели окна в bitmap через `BitBlt` (Bit Block Transfer).
/// 4. Извлекает сырые пиксели из bitmap через `GetDIBits` в формате BGRA (32 bit).
/// 5. Конвертирует BGRA → RGBA и нормализует альфа-канал.
///
/// # Ограничения
/// - `BitBlt` копирует только **видимую** часть окна. Если окно перекрыто другими окнами
///   или частично за пределами экрана — в этих местах будут пиксели перекрывающих окон
///   или мусор.
/// - Для захвата off-screen окон или DirectX/OpenGL контента нужен `PrintWindow`
///   (требует feature `Win32_Graphics_Printing`), который здесь не используется.
/// - Некоторые приложения (игры, видеоплееры с hardware overlay) могут отдавать чёрный
///   прямоугольник вместо реального содержимого.
///
/// # Параметры
/// - `hwnd`: Дескриптор окна (HWND). Должен быть валидным и существующим.
/// - `width`: Ширина области захвата в пикселях (обычно = ширина окна из `DwmGetWindowAttribute`).
/// - `height`: Высота области захвата в пикселях (обычно = высота окна из `DwmGetWindowAttribute`).
///
/// # Возвращаемое значение
/// Type: Result<RgbaImage, String>
/// - `Ok(RgbaImage)`: RGBA изображение размером `width × height`, top-down (первый пиксель —
///   левый верхний угол), альфа-канал = 255 для всех пикселей.
/// - `Err(String)`: Текстовое описание ошибки.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - `GetWindowDC` вернул NULL (окно уничтожено или невалидный HWND);
/// - `CreateCompatibleDC` или `CreateCompatibleBitmap` не смогли выделить ресурсы (нехватка GDI);
/// - `SelectObject` не смог связать bitmap с DC;
/// - `BitBlt` не смог скопировать пиксели (редко, обычно при проблемах с DC);
/// - `GetDIBits` вернул 0 (ошибка извлечения пикселей из bitmap);
/// - `RgbaImage::from_raw` не смог создать изображение (несоответствие размеров буфера).
///
/// # Побочные эффекты
/// - Кратковременно создаёт и уничтожает GDI-объекты (DC, bitmap).
/// - Все ресурсы корректно освобождаются даже при ошибках (RAII через явный cleanup).
fn _capture_hwnd_rgba(hwnd: HWND, width: u32, height: u32) -> Result<RgbaImage, String> {

    unsafe {

        let w_i32 = width as i32;
        let h_i32 = height as i32;

        // ─────────────────────────────────────────────────────────────────────────────────────
        // ШАГ 1: Получение Device Context окна.
        // ─────────────────────────────────────────────────────────────────────────────────────
        // GetWindowDC возвращает DC, который включает non-client область (рамки, заголовок).
        // В отличие от GetDC (только client area), это позволяет захватить окно целиком.
        // Важно: полученный DC нужно обязательно освободить через ReleaseDC.
        let win_dc: HDC = GetWindowDC(Some(hwnd));
        if win_dc.0.is_null() {
            return Err("screenshot: GetWindowDC() вернул NULL".to_string());
        }   // if

        // ─────────────────────────────────────────────────────────────────────────────────────
        // ШАГ 2: Создание memory DC для off-screen рендеринга.
        // ─────────────────────────────────────────────────────────────────────────────────────
        // Memory DC — это "виртуальный" контекст, не привязанный к физическому устройству.
        // В него можно рисовать (или копировать) без отображения на экране.
        // CreateCompatibleDC создаёт DC, совместимый с указанным (наследует цветовой формат).
        let mem_dc = CreateCompatibleDC(Some(win_dc));
        if mem_dc.0.is_null() {
            let _ = ReleaseDC(Some(hwnd), win_dc);
            return Err("screenshot: CreateCompatibleDC() failed".to_string());
        }   // if

        // ─────────────────────────────────────────────────────────────────────────────────────
        // ШАГ 3: Создание bitmap для хранения пикселей.
        // ─────────────────────────────────────────────────────────────────────────────────────
        // Bitmap — это GDI-объект, хранящий растровое изображение.
        // CreateCompatibleBitmap создаёт bitmap с тем же цветовым форматом, что и указанный DC.
        // Размер bitmap = размеру области захвата (width × height).
        let bmp: HBITMAP = CreateCompatibleBitmap(win_dc, w_i32, h_i32);
        if bmp.0.is_null() {
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(Some(hwnd), win_dc);
            return Err("screenshot: CreateCompatibleBitmap() failed".to_string());
        }   // if

        // ─────────────────────────────────────────────────────────────────────────────────────
        // ШАГ 4: Выбор bitmap в memory DC.
        // ─────────────────────────────────────────────────────────────────────────────────────
        // SelectObject "активирует" bitmap в DC: все операции рисования в этот DC
        // будут записываться в bitmap. Возвращает предыдущий объект (нужно восстановить потом).
        let old = SelectObject(mem_dc, bmp.into());
        if old.0.is_null() {
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(Some(hwnd), win_dc);
            return Err("screenshot: SelectObject() failed".to_string());
        }   // if

        // ─────────────────────────────────────────────────────────────────────────────────────
        // ШАГ 5: Копирование пикселей окна в bitmap.
        // ─────────────────────────────────────────────────────────────────────────────────────
        // BitBlt (Bit Block Transfer) — быстрое копирование прямоугольной области пикселей.
        // Параметры:
        //   - dst DC (mem_dc) и координаты назначения (0, 0)
        //   - размеры области (w_i32 × h_i32)
        //   - src DC (win_dc) и координаты источника (0, 0)
        //   - операция SRCCOPY = просто копировать пиксели без преобразований
        //
        // ОГРАНИЧЕНИЕ: BitBlt копирует только то, что физически нарисовано на экране.
        // Если окно частично перекрыто — в bitmap попадут пиксели перекрывающего окна.
        if BitBlt(mem_dc, 0, 0, w_i32, h_i32, Some(win_dc), 0, 0, SRCCOPY).is_err() {
            let _ = SelectObject(mem_dc, old);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(Some(hwnd), win_dc);

            return Err("screenshot: BitBlt не смог захватить окно".to_string());
        }   // if

        // ─────────────────────────────────────────────────────────────────────────────────────
        // ШАГ 6: Подготовка структуры BITMAPINFO для извлечения пикселей.
        // ─────────────────────────────────────────────────────────────────────────────────────
        // BITMAPINFOHEADER описывает формат, в котором мы хотим получить пиксели:
        //   - biWidth/biHeight: размеры изображения
        //   - biHeight < 0: top-down (первая строка = верх изображения); иначе bottom-up
        //   - biBitCount = 32: 4 байта на пиксель (BGRA)
        //   - biCompression = BI_RGB: без сжатия, сырые пиксели
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w_i32,
            biHeight: -h_i32, // Отрицательное значение = top-down (удобнее для работы).
            biPlanes: 1,      // Всегда 1 для bitmap.
            biBitCount: 32,   // 32 бита = BGRA (4 байта на пиксель).
            biCompression: BI_RGB.0, // Без сжатия.
            ..Default::default()
        };

        // Буфер для сырых пикселей: width × height пикселей × 4 байта (BGRA).
        let mut bgra: Vec<u8> = vec![0u8; (width as usize) * (height as usize) * 4];

        // ─────────────────────────────────────────────────────────────────────────────────────
        // ШАГ 7: Извлечение пикселей из bitmap.
        // ─────────────────────────────────────────────────────────────────────────────────────
        // GetDIBits копирует пиксели из GDI bitmap в пользовательский буфер.
        // Возвращает количество скопированных строк (scanlines); 0 = ошибка.
        let scanlines = GetDIBits(
            mem_dc,
            bmp,
            0,                                   // Начальная строка (0 = первая).
            height,                              // Количество строк для копирования.
            Some(bgra.as_mut_ptr() as *mut _),   // Буфер назначения.
            &mut bi,                             // Описание формата.
            DIB_RGB_COLORS,                      // Цвета = RGB (не палитра).
        );

        // ─────────────────────────────────────────────────────────────────────────────────────
        // ШАГ 8: Освобождение GDI-ресурсов.
        // ─────────────────────────────────────────────────────────────────────────────────────
        // Порядок важен: сначала восстанавливаем старый объект в DC, потом удаляем bitmap,
        // потом удаляем DC, потом освобождаем оконный DC.
        // Ошибки игнорируем — это cleanup, и мы уже получили (или не получили) пиксели.
        let _ = SelectObject(mem_dc, old);      // Восстановить предыдущий объект.
        let _ = DeleteObject(bmp.into());       // Удалить bitmap.
        let _ = DeleteDC(mem_dc);               // Удалить memory DC.
        let _ = ReleaseDC(Some(hwnd), win_dc);  // Освободить оконный DC.

        if scanlines == 0 {
            return Err("screenshot: GetDIBits() вернул 0".to_string());
        }   // if

        // ─────────────────────────────────────────────────────────────────────────────────────
        // ШАГ 9: Конвертация BGRA → RGBA.
        // ─────────────────────────────────────────────────────────────────────────────────────
        // Windows GDI возвращает пиксели в формате BGRA (Blue, Green, Red, Alpha).
        // Большинство графических библиотек (включая xcap::image) ожидают RGBA.
        // Также нормализуем альфа-канал в 255: Windows часто возвращает 0 или мусор в альфе,
        // что приводит к "невидимому" изображению при сохранении в PNG с прозрачностью.
        for px in bgra.chunks_exact_mut(4) {
            // px[0]=B, px[1]=G, px[2]=R, px[3]=A → px[0]=R, px[1]=G, px[2]=B, px[3]=255
            let b = px[0];
            let r = px[2];
            px[0] = r;      // Red на место Blue.
            px[2] = b;      // Blue на место Red.
            px[3] = 255;    // Alpha = полностью непрозрачный.
        }   // for

        // ─────────────────────────────────────────────────────────────────────────────────────
        // ШАГ 10: Создание RgbaImage из сырых пикселей.
        // ─────────────────────────────────────────────────────────────────────────────────────
        // RgbaImage::from_raw ожидает буфер размером width × height × 4 байт.
        // Если размер не совпадает — вернёт None (не должно случиться при корректных входных).
        let img = RgbaImage::from_raw(width, height, bgra)
            .ok_or_else(|| "screenshot: RgbaImage::from_raw() failed".to_string())?;

        Ok(img)
    }   // unsafe
}   // _capture_hwnd_rgba()

/// Строит карту соответствия logical_index -> physical_index.
///
/// Логический индекс определяется сортировкой по (y, x): сверху вниз, затем слева направо.
/// Физический индекс — это порядковый номер монитора в массиве `Monitor::all()`.
fn _build_logical_to_physical_map() -> Result<Vec<usize>, String> {

    let monitors = Monitor::all()
        .map_err(|e| format!("xcap: Monitor::all() failed: {}", e))?;

    if monitors.is_empty() {
        return Err("xcap: не найдено ни одного монитора".to_string());
    }   // if

    // Собираем пары (physical_index, x, y) для каждого монитора.
    let mut indexed_positions: Vec<(usize, i32, i32)> = Vec::with_capacity(monitors.len());

    for (physical_idx, m) in monitors.iter().enumerate() {

        let x = m.x().map_err(|e| format!(
            "xcap: Monitor::x() failed (physical_idx={}): {}", physical_idx, e
        ))?;

        let y = m.y().map_err(|e| format!(
            "xcap: Monitor::y() failed (physical_idx={}): {}", physical_idx, e
        ))?;

        indexed_positions.push((physical_idx, x, y));
    }   // for

    // Сортируем по (y, x): сначала по вертикали (сверху вниз), затем по горизонтали (слева направо).
    indexed_positions.sort_by(|a, b| {
        match a.2.cmp(&b.2) {
            std::cmp::Ordering::Equal => a.1.cmp(&b.1),
            other => other,
        }
    });

    Ok(indexed_positions.iter().map(|(phys, _, _)| *phys).collect())
}   // _build_logical_to_physical_map()

/// Накладывает текущий курсор мыши на изображение и возвращает `CursorInfo`.
///
/// # Параметры
/// - `image`: Изображение, на которое накладывается курсор.
/// - `image_origin_x`: X левого верхнего угла изображения в координатах виртуального рабочего стола.
/// - `image_origin_y`: Y левого верхнего угла изображения в координатах виртуального рабочего стола.
///
/// # Возвращаемое значение
/// `CursorInfo`: описание положения hotspot курсора относительно изображения.
fn _overlay_cursor(
    image: &mut RgbaImage,
    image_origin_x: i32,
    image_origin_y: i32,
) -> Result<CursorInfo, String> {

    // --- 1. Получаем состояние курсора ---
    let cursor_state = get_cursor_state()?;

    // Если курсор скрыт — возвращаем hidden.
    if !cursor_state.is_visible {
        return Ok(CursorInfo::hidden());
    }   // if

    // --- 2. Извлекаем изображение курсора ---
    let (cursor_w, cursor_h, hotspot_x, hotspot_y, cursor_pixels) =
        get_cursor_rgba(cursor_state.hcursor)?;

    // --- 3. Hotspot относительно изображения ---
    // cursor_state.x/y — координаты hotspot в экранных координатах.
    let hotspot_rel_x = cursor_state.x - image_origin_x;
    let hotspot_rel_y = cursor_state.y - image_origin_y;

    // Размеры целевого изображения.
    let img_w = image.width() as i32;
    let img_h = image.height() as i32;

    // Попадает ли hotspot в область изображения.
    let hotspot_in_image =
        hotspot_rel_x >= 0 && hotspot_rel_x < img_w &&
            hotspot_rel_y >= 0 && hotspot_rel_y < img_h;

    // --- 4. Координаты отрисовки курсора ---
    let draw_x = hotspot_rel_x - hotspot_x as i32;
    let draw_y = hotspot_rel_y - hotspot_y as i32;

    // Если курсор полностью за пределами — не рисуем, но координаты возвращаем.
    if draw_x + cursor_w as i32 <= 0 || draw_x >= img_w ||
        draw_y + cursor_h as i32 <= 0 || draw_y >= img_h
    {
        return Ok(CursorInfo::visible(hotspot_in_image, hotspot_rel_x, hotspot_rel_y));
    }   // if

    // --- 5. Альфа-блендинг ---
    for cy in 0..cursor_h as i32 {
        for cx in 0..cursor_w as i32 {

            let tx = draw_x + cx;
            let ty = draw_y + cy;

            if tx < 0 || tx >= img_w || ty < 0 || ty >= img_h {
                continue;
            }   // if

            let cursor_idx = ((cy as u32 * cursor_w + cx as u32) * 4) as usize;

            let c_r = cursor_pixels[cursor_idx] as u32;
            let c_g = cursor_pixels[cursor_idx + 1] as u32;
            let c_b = cursor_pixels[cursor_idx + 2] as u32;
            let c_a = cursor_pixels[cursor_idx + 3] as u32;

            if c_a == 0 {
                continue;
            }   // if

            let bg_pixel = image.get_pixel(tx as u32, ty as u32);
            let bg_r = bg_pixel[0] as u32;
            let bg_g = bg_pixel[1] as u32;
            let bg_b = bg_pixel[2] as u32;

            let alpha = c_a;
            let inv_alpha = 255 - alpha;

            let out_r = ((c_r * alpha + bg_r * inv_alpha) / 255) as u8;
            let out_g = ((c_g * alpha + bg_g * inv_alpha) / 255) as u8;
            let out_b = ((c_b * alpha + bg_b * inv_alpha) / 255) as u8;

            image.put_pixel(tx as u32, ty as u32, Rgba([out_r, out_g, out_b, 255]));
        }   // for cx
    }   // for cy

    Ok(CursorInfo::visible(hotspot_in_image, hotspot_rel_x, hotspot_rel_y))
}   // _overlay_cursor()

/// Описание: Возвращает физический индекс монитора по логическому индексу.
///
/// # Параметры
/// - `logical_index`: Логический индекс (0..N-1).
///
/// # Возвращаемое значение
/// `usize`: Физический индекс в массиве `Monitor::all()`.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - карта не построена (ошибка построения);
/// - `logical_index` выходит за пределы.
fn _physical_index_by_logical_index(logical_index: usize) -> Result<usize, String> {
    let map = LOGICAL_TO_PHYSICAL_MAP.as_ref()?;

    if logical_index >= map.len() {
        return Err(format!(
            "screenshot: logical_index={} выходит за пределы (доступно {} мониторов)",
            logical_index, map.len()
        ));
    }   // if

    Ok(map[logical_index])
}   // physical_index_by_logical_index()

mod mouse_tool {
    use windows::Win32::Graphics::Gdi::{CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC};
    use windows::Win32::UI::WindowsAndMessaging::{GetCursorInfo, GetIconInfo, CURSORINFO, CURSOR_SHOWING, HCURSOR, ICONINFO};

    /// Информация о текущем состоянии курсора мыши.
    #[derive(Debug)]
    pub(super) struct CursorState {
        /// Позиция курсора по оси X в экранных координатах (виртуальный рабочий стол).
        /// Может быть отрицательной, если основной монитор не в левом верхнем углу.
        pub x: i32,

        /// Позиция курсора по оси Y в экранных координатах.
        pub y: i32,

        /// Флаг видимости курсора.
        /// `false` — курсор скрыт (например, при полноэкранном видео или в некоторых играх).
        pub is_visible: bool,

        /// Хендл текущего курсора.
        /// Используется для извлечения изображения через `get_cursor_rgba()`.
        /// Хендл валиден только в момент получения — форма курсора может измениться.
        pub hcursor: HCURSOR,
    }   // CursorState

    /// Получает полную информацию о текущем состоянии курсора.
    ///
    /// # Алгоритм работы
    /// - Инициализирует структуру `CURSORINFO` с корректным размером.
    /// - Вызывает Win32 API `GetCursorInfo`.
    /// - Извлекает позицию, флаг видимости и хендл курсора.
    ///
    /// # Возвращаемое значение
    /// Тип: `CursorState` — структура с позицией, видимостью и хендлом.
    ///
    /// # Когда курсор невидим (`is_visible = false`)
    /// - Приложение скрыло курсор через `ShowCursor(FALSE)`.
    /// - Полноэкранный режим видео/игры.
    /// - Курсор за пределами всех мониторов (редко).
    ///
    /// # Ошибки
    /// Возвращает `Err(String)`, если Win32 API вернул ошибку.
    pub(super) fn get_cursor_state() -> Result<CursorState, String> {
        // Инициализация структуры. ВАЖНО: cbSize должен быть установлен до вызова API.
        let mut cursor_info = CURSORINFO {
            cbSize: size_of::<CURSORINFO>() as u32,
            ..Default::default()
        };

        unsafe {
            GetCursorInfo(&mut cursor_info)
                .map_err(|e| format!("GetCursorInfo failed: {}", e))?;
        }

        // Проверка флага видимости. CURSOR_SHOWING = 0x00000001.
        let is_visible = (cursor_info.flags.0 & CURSOR_SHOWING.0) != 0;

        Ok(CursorState {
            x: cursor_info.ptScreenPos.x,
            y: cursor_info.ptScreenPos.y,
            is_visible,
            hcursor: cursor_info.hCursor,
        })
    }   // get_cursor_state()

    /// Извлекает изображение курсора как RGBA-буфер.
    ///
    /// # Описание
    /// Функция извлекает растровое изображение курсора из его хендла.
    /// Результат готов для наложения на скриншот с учётом прозрачности.
    ///
    /// # Алгоритм работы
    /// 1. Вызывает `GetIconInfo` для получения bitmap-ов курсора (цвет + маска).
    /// 2. Создаёт совместимый DC для работы с bitmap.
    /// 3. Определяет размеры курсора через первый вызов `GetDIBits` (query mode).
    /// 4. Определяет тип курсора:
    ///    - **Цветной (32-bit BGRA)**: есть `hbmColor`, маска `hbmMask` имеет ту же высоту.
    ///      Альфа-канал берётся из цветного bitmap или из маски.
    ///    - **Монохромный (AND/XOR)**: `hbmColor` отсутствует, маска `hbmMask` имеет
    ///      двойную высоту: верхняя половина — AND-маска, нижняя — XOR-маска.
    /// 5. Извлекает пиксели и формирует RGBA-буфер с корректной прозрачностью.
    /// 6. Освобождает все GDI-ресурсы.
    ///
    /// # Параметры
    /// - `hcursor`: Хендл курсора, полученный из `CursorState::hcursor`.
    ///
    /// # Возвращаемое значение
    /// Кортеж `(width, height, hotspot_x, hotspot_y, rgba_pixels)`:
    /// - `width`, `height` — размеры изображения курсора в пикселях (обычно 32×32).
    /// - `hotspot_x`, `hotspot_y` — смещение "горячей точки" от левого верхнего угла.
    ///   Горячая точка — это пиксель, который считается "позицией" курсора.
    ///   Для стрелки — кончик, для перекрестия — центр.
    /// - `rgba_pixels` — вектор пикселей в формате RGBA (4 байта на пиксель).
    ///   Длина: `width * height * 4` байт.
    ///
    /// # Горячая точка (hotspot)
    /// При наложении курсора на скриншот нужно учитывать hotspot:
    /// ```ignore
    /// let draw_x = cursor_screen_x - hotspot_x;
    /// let draw_y = cursor_screen_y - hotspot_y;
    /// ```
    ///
    /// # Форматы курсоров Windows
    /// ## Цветной курсор (большинство современных курсоров)
    /// - `hbmColor` содержит 32-bit BGRA пиксели.
    /// - `hbmMask` содержит маску прозрачности (та же высота, что и hbmColor).
    /// - Если в цветном bitmap альфа > 0 — используется она; иначе альфа берётся из маски.
    ///
    /// ## Монохромный курсор (текстовый I-beam, resize-стрелки старого стиля)
    /// - `hbmColor` отсутствует (invalid handle).
    /// - `hbmMask` имеет **двойную высоту**: `height_mask = 2 * height_cursor`.
    /// - Верхняя половина — AND-маска, нижняя — XOR-маска.
    /// - Логика отрисовки:
    ///   | AND | XOR | Результат                    |
    ///   |-----|-----|------------------------------|
    ///   |  0  |  0  | Чёрный (непрозрачный)        |
    ///   |  0  |  1  | Белый (непрозрачный)         |
    ///   |  1  |  0  | Прозрачный (фон виден)       |
    ///   |  1  |  1  | Инверсия фона                |
    ///
    /// Инверсию фона в статическом скриншоте реализовать корректно невозможно (фон неизвестен
    /// на момент формирования RGBA). Используется полупрозрачный чёрный как компромисс.
    ///
    /// # Ошибки
    /// Возвращает `Err(String)`, если:
    /// - `GetIconInfo` не смог получить информацию о курсоре.
    /// - Не удалось создать DC (`CreateCompatibleDC`).
    /// - `GetDIBits` не смог извлечь пиксели.
    ///
    /// # Побочные эффекты
    /// - Создаёт и удаляет GDI-объекты (DC, HBITMAP).
    /// - Все ресурсы освобождаются до возврата.
    pub(super) fn get_cursor_rgba(hcursor: HCURSOR) -> Result<(u32, u32, u32, u32, Vec<u8>), String> {
        unsafe {
            // --- 1. Получаем информацию о курсоре (bitmaps + hotspot) ---
            let mut icon_info = ICONINFO::default();
            GetIconInfo(hcursor.into(), &mut icon_info)
                .map_err(|e| format!("GetIconInfo failed: {}", e))?;

            // Hotspot — смещение "активной точки" курсора от левого верхнего угла.
            let hotspot_x = icon_info.xHotspot;
            let hotspot_y = icon_info.yHotspot;

            // hbmColor — цветной bitmap (NULL для монохромных курсоров).
            // hbmMask — маска прозрачности (всегда присутствует).
            let hbm_color = icon_info.hbmColor;
            let hbm_mask = icon_info.hbmMask;

            // Монохромный курсор: нет цветного bitmap, маска содержит AND + XOR.
            let is_monochrome = hbm_color.is_invalid();

            // --- 2. Создаём Device Context для работы с bitmap ---
            // CreateCompatibleDC(None) создаёт DC, совместимый с экраном.
            let hdc = CreateCompatibleDC(None);
            if hdc.is_invalid() {

                // Освобождаем bitmap-ы при ошибке.
                if !hbm_color.is_invalid() { let _ = DeleteObject(hbm_color.into()); }
                if !hbm_mask.is_invalid() { let _ = DeleteObject(hbm_mask.into()); }
                return Err("CreateCompatibleDC failed".to_string());
            }   // if

            // --- 3. Определяем размеры курсора ---
            // Для query используем цветной bitmap (если есть), иначе маску.
            let bitmap_to_query = if !hbm_color.is_invalid() { hbm_color } else { hbm_mask };
            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    ..Default::default()
                },
                ..Default::default()
            };

            // Первый вызов GetDIBits с NULL буфером — режим запроса размеров.
            let result = GetDIBits(hdc, bitmap_to_query, 0, 0, None, &mut bmi, DIB_RGB_COLORS);
            if result == 0 {
                let _ = DeleteDC(hdc);
                if !hbm_color.is_invalid() { let _ = DeleteObject(hbm_color.into()); }
                if !hbm_mask.is_invalid() { let _ = DeleteObject(hbm_mask.into()); }
                return Err("GetDIBits (query) failed".to_string());
            }   // if

            let width = bmi.bmiHeader.biWidth as u32;
            // biHeight может быть отрицательным (top-down DIB), берём абсолютное значение.
            let raw_height = bmi.bmiHeader.biHeight.unsigned_abs();

            // Для монохромных курсоров маска содержит AND + XOR друг под другом,
            // поэтому реальная высота курсора = половина высоты bitmap.
            let height = if is_monochrome { raw_height / 2 } else { raw_height };

            // --- 4. Формируем RGBA-пиксели в зависимости от типа курсора ---
            let rgba_pixels = if is_monochrome {
                _extract_monochrome_cursor(hdc, hbm_mask, &mut bmi, width, height, raw_height)?
            } else {
                _extract_color_cursor(hdc, hbm_color, hbm_mask, &mut bmi, width, height)?
            };

            // --- 5. Освобождаем GDI-ресурсы ---
            let _ = DeleteDC(hdc);
            if !hbm_color.is_invalid() { let _ = DeleteObject(hbm_color.into()); }
            if !hbm_mask.is_invalid() { let _ = DeleteObject(hbm_mask.into()); }

            Ok((width, height, hotspot_x, hotspot_y, rgba_pixels))
        }   // unsafe
    }   // get_cursor_rgba()

    /// Описание: Извлекает RGBA-пиксели монохромного курсора из AND/XOR масок.
    ///
    /// Монохромный курсор не имеет цветного bitmap. Его маска (`hbmMask`) содержит
    /// две половины одна под другой:
    /// - верхняя половина (строки 0..height) — AND-маска,
    /// - нижняя половина (строки height..raw_height) — XOR-маска.
    ///
    /// # Параметры
    /// - `hdc`: Совместимый Device Context.
    /// - `hbm_mask`: Хендл маски курсора.
    /// - `bmi`: Структура BITMAPINFO (будет модифицирована для извлечения).
    /// - `width`: Ширина курсора в пикселях.
    /// - `height`: Высота курсора в пикселях (половина raw_height).
    /// - `raw_height`: Полная высота bitmap маски (= 2 * height).
    ///
    /// # Возвращаемое значение
    /// `Vec<u8>`: RGBA-буфер размером `width * height * 4`.
    ///
    /// # Ошибки
    /// Возвращает `Err(String)`, если `GetDIBits` не смог извлечь пиксели маски.
    fn _extract_monochrome_cursor(
        hdc: HDC,
        hbm_mask: HBITMAP,
        bmi: &mut BITMAPINFO,
        width: u32,
        height: u32,
        raw_height: u32,
    ) -> Result<Vec<u8>, String> {

        // Настройка BITMAPINFO для извлечения полной маски (AND + XOR) как 32-bit BGRA top-down.
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB.0;
        bmi.bmiHeader.biHeight = -(raw_height as i32); // top-down
        bmi.bmiHeader.biSizeImage = width * raw_height * 4;

        let mut mask_pixels: Vec<u8> = vec![0u8; (width * raw_height * 4) as usize];

        let _old = unsafe { SelectObject(hdc, hbm_mask.into()) };

        let result = unsafe { GetDIBits(
            hdc,
            hbm_mask,
            0,
            raw_height,
            Some(mask_pixels.as_mut_ptr() as *mut _),
            bmi,
            DIB_RGB_COLORS,
        ) };

        if result == 0 {
            return Err("GetDIBits (monochrome mask) failed".to_string());
        }   // if

        // Разбираем AND-маску (верхняя половина) и XOR-маску (нижняя половина).
        let row_bytes = (width * 4) as usize;
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);

        for y in 0..height as usize {
            for x in 0..width as usize {
                let and_idx = y * row_bytes + x * 4;
                let xor_idx = (y + height as usize) * row_bytes + x * 4;

                // Берём любой канал (R/G/B одинаковые для монохромных масок).
                let and_val = mask_pixels[and_idx];   // 0 или 255
                let xor_val = mask_pixels[xor_idx];   // 0 или 255

                let (r, g, b, a) = match (and_val, xor_val) {
                    // AND=0, XOR=0: чёрный, непрозрачный.
                    (0, 0) => (0u8, 0u8, 0u8, 255u8),

                    // AND=0, XOR=255: белый, непрозрачный.
                    (0, 255) => (255, 255, 255, 255),

                    // AND=255, XOR=0: прозрачный (фон виден).
                    (255, 0) => (0, 0, 0, 0),

                    // AND=255, XOR=255: инверсия фона.
                    // В статическом скриншоте инверсия невозможна. Используем
                    // полупрозрачный чёрный — даёт заметный контраст
                    // на большинстве фонов.
                    (255, 255) => (0, 0, 0, 128),

                    // Промежуточные значения (не должны встречаться в монохромных масках,
                    // но обрабатываем на всякий случай): полупрозрачный серый.
                    _ => (128, 128, 128, 128),
                };   // match

                rgba.push(r);
                rgba.push(g);
                rgba.push(b);
                rgba.push(a);
            }   // for x
        }   // for y

        Ok(rgba)
    }   // _extract_monochrome_cursor()

    /// Описание: Извлекает RGBA-пиксели цветного курсора из цветного bitmap и маски.
    ///
    /// # Параметры
    /// - `hdc`: Совместимый Device Context.
    /// - `hbm_color`: Хендл цветного bitmap курсора.
    /// - `hbm_mask`: Хендл маски прозрачности курсора.
    /// - `bmi`: Структура BITMAPINFO (будет модифицирована для извлечения).
    /// - `width`: Ширина курсора в пикселях.
    /// - `height`: Высота курсора в пикселях.
    ///
    /// # Возвращаемое значение
    /// `Vec<u8>`: RGBA-буфер размером `width * height * 4`.
    ///
    /// # Ошибки
    /// Возвращает `Err(String)`, если `GetDIBits` не смог извлечь пиксели.
    fn _extract_color_cursor(
        hdc: HDC,
        hbm_color: HBITMAP,
        hbm_mask: HBITMAP,
        bmi: &mut BITMAPINFO,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, String> {

        // Настройка BITMAPINFO для извлечения цветных пикселей как 32-bit BGRA top-down.
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB.0;
        bmi.bmiHeader.biHeight = -(height as i32); // top-down
        bmi.bmiHeader.biSizeImage = width * height * 4;

        let pixel_count = (width * height * 4) as usize;

        // --- Извлекаем цветные пиксели ---
        let mut color_pixels: Vec<u8> = vec![0u8; pixel_count];

        let _old = unsafe { SelectObject(hdc, hbm_color.into()) };

        let result = unsafe { GetDIBits(
            hdc,
            hbm_color,
            0,
            height,
            Some(color_pixels.as_mut_ptr() as *mut _),
            bmi,
            DIB_RGB_COLORS,
        ) };

        if result == 0 {
            return Err("GetDIBits (color) failed".to_string());
        }   // if

        // --- Извлекаем маску прозрачности (если есть) ---
        let has_mask = !hbm_mask.is_invalid();
        let mut mask_pixels: Vec<u8> = vec![0u8; pixel_count];

        if has_mask {
            let mut mask_bmi = *bmi;
            mask_bmi.bmiHeader.biHeight = -(height as i32);

            let _old = unsafe { SelectObject(hdc, hbm_mask.into()) };

            let result = unsafe { GetDIBits(
                hdc,
                hbm_mask,
                0,
                height,
                Some(mask_pixels.as_mut_ptr() as *mut _),
                &mut mask_bmi,
                DIB_RGB_COLORS,
            ) };

            if result == 0 {
                return Err("GetDIBits (color mask) failed".to_string());
            }   // if
        }   // if has_mask

        // --- Конвертация BGRA → RGBA с применением маски ---
        let mut rgba = Vec::with_capacity(pixel_count);

        for i in 0..(width * height) as usize {
            let idx = i * 4;

            // Windows хранит пиксели в формате BGRA.
            let b = color_pixels[idx];
            let g = color_pixels[idx + 1];
            let r = color_pixels[idx + 2];
            let a = color_pixels[idx + 3];

            // Определяем альфа-канал.
            // Маска определяет прозрачность:
            // - Белый (255) в маске → пиксель полностью прозрачный.
            // - Чёрный (0) в маске → пиксель непрозрачный.
            // Для 32-bit курсоров с альфа-каналом используем альфу из цветного bitmap.
            let alpha = if has_mask {
                let mask_val = mask_pixels[idx]; // Любой канал маски (они одинаковые).
                if mask_val == 255 {
                    0       // Маска белая → прозрачный.
                } else if a > 0 {
                    a       // Альфа из цветного bitmap (32-bit курсоры).
                } else {
                    255     // Маска чёрная, альфа 0 → непрозрачный (старые курсоры).
                }
            } else {
                // Нет маски: используем альфу из цветного bitmap.
                if a > 0 { a } else { 255 }
            };

            rgba.push(r);
            rgba.push(g);
            rgba.push(b);
            rgba.push(alpha);
        }   // for

        Ok(rgba)
    }   // _extract_color_cursor()
}