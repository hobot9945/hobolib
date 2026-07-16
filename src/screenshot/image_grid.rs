//! image_grid.rs — Наложение координатной сетки на скриншот.
//!
//! # ОПИСАНИЕ
//! Модуль принимает готовое RGBA-изображение и возвращает новое изображение
//! с координатной сеткой и подписями осей. Предназначен для визуальной
//! навигации AI по скриншотам.
//!
//! # ОТВЕТСТВЕННОСТЬ
//! 1. Создание расширенного холста с белыми полями для подписей (сверху и слева).
//! 2. Наложение полупрозрачных линий сетки поверх изображения.
//! 3. Автоматический расчёт частоты подписей, чтобы надписи не перекрывались.
//! 4. Отрисовка координатных подписей встроенным bitmap-шрифтом 5×7.
//!
//! # ИНВАРИАНТЫ
//! - Подписи отражают пиксельные координаты внутри исходного изображения (не индексы линий).
//! - Шаг сетки одинаков по X и Y.
//! - Толщина линий: 1 пиксель.
//! - Подписи: сверху (ось X) и слева (ось Y).
//! - Фон полей: белый, цвет подписей: чёрный.

#![allow(unused)]
use xcap::image::{Rgba, RgbaImage};

// ─────────────────────────────────────────────────────────────────────────────
//                     Константы шрифта и оформления
// ─────────────────────────────────────────────────────────────────────────────

/// Ширина одного глифа bitmap-шрифта (пикселей).
const GLYPH_W: u32 = 5;

/// Высота одного глифа bitmap-шрифта (пикселей).
const GLYPH_H: u32 = 7;

/// Горизонтальный интервал между символами (пикселей).
const GLYPH_GAP: u32 = 1;

/// Внутренний отступ от края поля до текста/содержимого (пикселей).
const FRAME_PAD: u32 = 3;

/// Минимальный зазор между соседними подписями (пикселей).
/// Используется при автоматическом расчёте частоты подписей.
const LABEL_MIN_GAP: u32 = 6;

/// Цвет фона полей (белый, непрозрачный).
const FRAME_BG: Rgba<u8> = Rgba([255, 255, 255, 255]);

/// RGB-компоненты цвета текста подписей (чёрный).
const LABEL_RGB: [u8; 3] = [0, 0, 0];

// ─────────────────────────────────────────────────────────────────────────────
//                            Публичный API
// ─────────────────────────────────────────────────────────────────────────────

/// Цвет линии сетки (R, G, B, A).
#[derive(Debug, Clone, Copy)]
pub struct GridColor(pub u8, pub u8, pub u8, pub u8);

/// Накладывает координатную сетку на изображение.
///
/// Создаёт новое изображение с белыми полями сверху и слева,
/// рисует полупрозрачные линии сетки поверх содержимого
/// и подписывает координаты (пиксельные, не индексы линий).
///
/// Размеры полей и частота подписей рассчитываются автоматически
/// на основе шага сетки, размеров изображения и параметров шрифта.
///
/// # Алгоритм работы
/// 1. Рассчитать максимальные координаты и ширину подписей.
/// 2. Рассчитать частоту подписей отдельно для каждой оси.
/// 3. Определить размеры полей (верхнее, левое) и запасы (правый, нижний).
/// 4. Создать холст, залить белым.
/// 5. Скопировать исходное изображение в область контента.
/// 6. Нарисовать линии сетки с альфа-смешиванием.
/// 7. Нарисовать подписи X (верхнее поле) и Y (левое поле).
///
/// # Параметры
/// - `src`: Исходное RGBA-изображение.
/// - `step_px`: Шаг сетки в пикселях (должен быть > 0).
/// - `line_color`: Цвет и прозрачность линий сетки.
///
/// # Возвращаемое значение
/// Тип: `Result<RgbaImage, String>`: Новое изображение с сеткой и подписями.
///
/// # Ошибки
/// - `step_px == 0`.
/// - Исходное изображение пустое (ширина или высота == 0).
/// - Размеры итогового холста переполняют `u32`.
pub fn add_grid(src: &RgbaImage, step_px: u32, line_color: GridColor) -> Result<RgbaImage, String> {

    if step_px == 0 {
        return Err("image_grid: step_px должен быть > 0".to_string());
    }   // if

    let src_w = src.width();
    let src_h = src.height();

    if src_w == 0 || src_h == 0 {
        return Err("image_grid: исходное изображение пустое".to_string());
    }   // if

    // --- 1. Геометрия подписей ---

    // Максимальные координаты на каждой оси (последние линии сетки, не выходящие
    // за пределы изображения).
    let max_x_coord = _last_grid_line_coord(src_w, step_px);
    let max_y_coord = _last_grid_line_coord(src_h, step_px);

    // Ширина самой длинной подписи (в пикселях) на каждой оси.
    let max_x_label_w = _text_width(_digit_count(max_x_coord));
    let max_y_label_w = _text_width(_digit_count(max_y_coord));

    // --- 2. Частота подписей (отдельно по осям) ---

    // Для X-подписей: расстояние между соседними подписями >= ширина текста + зазор.
    let label_every_x = _calc_label_every(max_x_label_w, step_px);

    // Для Y-подписей: расстояние >= высота шрифта + зазор.
    let label_every_y = _calc_label_every(GLYPH_H, step_px);

    // --- 3. Размеры полей ---

    // Верхнее поле: высота шрифта + отступы сверху и снизу от текста.
    let top_frame_h = GLYPH_H + FRAME_PAD * 2;

    // Левое поле: ширина самой длинной Y-подписи + отступы.
    let left_frame_w = max_y_label_w + FRAME_PAD * 2;

    // Небольшой запас справа и снизу, чтобы крайние подписи не обрезались
    // (подпись центрируется на линии и может выступать за край контента).
    let right_pad = max_x_label_w / 2 + 1;
    let bottom_pad = GLYPH_H / 2 + 1;

    // --- 4. Создание холста ---

    let canvas_w = left_frame_w.checked_add(src_w)
        .and_then(|v| v.checked_add(right_pad))
        .ok_or("image_grid: переполнение ширины холста")?;

    let canvas_h = top_frame_h.checked_add(src_h)
        .and_then(|v| v.checked_add(bottom_pad))
        .ok_or("image_grid: переполнение высоты холста")?;

    let mut canvas = RgbaImage::from_pixel(canvas_w, canvas_h, FRAME_BG);

    // --- 5. Копирование исходного изображения в область контента ---

    _blit(src, &mut canvas, left_frame_w, top_frame_h);

    // --- 6. Линии сетки ---

    // Вертикальные линии (проходят только по области контента).
    let mut x = 0u32;
    while x < src_w {
        _draw_vline(&mut canvas, left_frame_w + x, top_frame_h, src_h, &line_color);
        x += step_px;
    }   // while x

    // Горизонтальные линии.
    let mut y = 0u32;
    while y < src_h {
        _draw_hline(&mut canvas, left_frame_w, top_frame_h + y, src_w, &line_color);
        y += step_px;
    }   // while y

    // --- 7. Подписи ---

    // Вертикальная позиция X-подписей: центрирование по высоте в верхнем поле.
    let x_label_y = top_frame_h.saturating_sub(GLYPH_H) / 2;

    // X-подписи (верхнее поле, над соответствующими вертикальными линиями).
    let mut line_idx = 0u32;
    let mut x = 0u32;
    while x < src_w {
        if line_idx % label_every_x == 0 {
            let text = x.to_string();
            let tw = _text_width(text.len() as u32);
            // Центрируем подпись горизонтально над линией сетки.
            let lx = (left_frame_w + x).saturating_sub(tw / 2);
            _draw_text(&mut canvas, &text, lx, x_label_y);
        }   // if
        x += step_px;
        line_idx += 1;
    }   // while x

    // Y-подписи (левое поле, слева от соответствующих горизонтальных линий).
    line_idx = 0;
    let mut y = 0u32;
    while y < src_h {
        if line_idx % label_every_y == 0 {
            let text = y.to_string();
            let tw = _text_width(text.len() as u32);
            // Выравниваем подпись по правому краю левого поля (с отступом FRAME_PAD).
            let lx = left_frame_w.saturating_sub(FRAME_PAD + tw);
            // Центрируем вертикально на уровне горизонтальной линии.
            let ly = (top_frame_h + y).saturating_sub(GLYPH_H / 2);
            _draw_text(&mut canvas, &text, lx, ly);
        }   // if
        y += step_px;
        line_idx += 1;
    }   // while y

    Ok(canvas)
}   // add_grid()

// ─────────────────────────────────────────────────────────────────────────────
//                        Расчёт геометрии
// ─────────────────────────────────────────────────────────────────────────────

/// Координата последней линии сетки, не выходящей за пределы `size`.
///
/// Пример: size=1920, step=100 → 1900.
fn _last_grid_line_coord(size: u32, step: u32) -> u32 {
    if size == 0 || step == 0 { return 0; }
    ((size - 1) / step) * step
}   // _last_grid_coord()

/// Число десятичных цифр в числе (для 0 возвращает 1).
fn _digit_count(value: u32) -> u32 {
    if value == 0 { return 1; }
    let mut n = value;
    let mut count = 0u32;
    while n > 0 {
        count += 1;
        n /= 10;
    }   // while
    count
}   // _digit_count()

/// Ширина строки текста в пикселях для заданного числа символов.
///
/// Учитывает ширину глифов и межсимвольные интервалы.
fn _text_width(char_count: u32) -> u32 {
    if char_count == 0 { return 0; }
    char_count * GLYPH_W + (char_count - 1) * GLYPH_GAP
}   // _text_width()

/// Рассчитывает, через сколько линий сетки ставить подпись,
/// чтобы подписи не перекрывались.
///
/// # Параметры
/// - `label_extent`: размер подписи вдоль оси
///   (ширина текста для X-оси, высота шрифта для Y-оси).
/// - `step_px`: шаг сетки в пикселях.
///
/// # Возвращаемое значение
/// Кратность подписей (минимум 1): подписывается каждая N-я линия.
fn _calc_label_every(label_extent: u32, step_px: u32) -> u32 {
    let required = label_extent + LABEL_MIN_GAP;
    // ceil(required / step_px), но не менее 1.
    let every = (required + step_px - 1) / step_px;
    every.max(1)
}   // _calc_label_every()

// ─────────────────────────────────────────────────────────────────────────────
//                       Примитивы рисования
// ─────────────────────────────────────────────────────────────────────────────

/// Копирует `src` в `dst` по смещению `(dx, dy)`.
///
/// Пиксели, выходящие за границы `dst`, игнорируются.
fn _blit(src: &RgbaImage, dst: &mut RgbaImage, dx: u32, dy: u32) {
    let dw = dst.width();
    let dh = dst.height();
    for sy in 0..src.height() {
        let ty = dy + sy;
        if ty >= dh { break; }
        for sx in 0..src.width() {
            let tx = dx + sx;
            if tx >= dw { break; }
            dst.put_pixel(tx, ty, *src.get_pixel(sx, sy));
        }   // for sx
    }   // for sy
}   // _blit()

/// Рисует вертикальную линию толщиной 1 px с альфа-смешиванием.
///
/// # Параметры
/// - `x`: горизонтальная позиция линии на холсте.
/// - `y_start`: начало линии по вертикали.
/// - `length`: длина линии в пикселях.
/// - `color`: цвет линии с альфой.
fn _draw_vline(canvas: &mut RgbaImage, x: u32, y_start: u32, length: u32, color: &GridColor) {
    if x >= canvas.width() { return; }
    let y_end = (y_start + length).min(canvas.height());
    for y in y_start..y_end {
        _blend_pixel(canvas, x, y, color);
    }   // for y
}   // _draw_vline()

/// Рисует горизонтальную линию толщиной 1 px с альфа-смешиванием.
///
/// # Параметры
/// - `x_start`: начало линии по горизонтали.
/// - `y`: вертикальная позиция линии на холсте.
/// - `length`: длина линии в пикселях.
/// - `color`: цвет линии с альфой.
fn _draw_hline(canvas: &mut RgbaImage, x_start: u32, y: u32, length: u32, color: &GridColor) {
    if y >= canvas.height() { return; }
    let x_end = (x_start + length).min(canvas.width());
    for x in x_start..x_end {
        _blend_pixel(canvas, x, y, color);
    }   // for x
}   // _draw_hline()

/// Смешивает один пиксель (src-over композиция).
///
/// Формула: out = src_rgb × α + dst_rgb × (1 − α).
/// Итоговый альфа-канал фиксирован: 255.
fn _blend_pixel(canvas: &mut RgbaImage, x: u32, y: u32, color: &GridColor) {
    let a = color.3 as u32;

    // Полностью прозрачный — ничего не делаем.
    if a == 0 { return; }

    // Полностью непрозрачный — простая замена пикселя.
    if a == 255 {
        canvas.put_pixel(x, y, Rgba([color.0, color.1, color.2, 255]));
        return;
    }   // if

    let dst = canvas.get_pixel(x, y);
    let inv_a = 255 - a;

    let r = ((color.0 as u32 * a + dst[0] as u32 * inv_a) / 255) as u8;
    let g = ((color.1 as u32 * a + dst[1] as u32 * inv_a) / 255) as u8;
    let b = ((color.2 as u32 * a + dst[2] as u32 * inv_a) / 255) as u8;

    canvas.put_pixel(x, y, Rgba([r, g, b, 255]));
}   // _blend_pixel()

// ─────────────────────────────────────────────────────────────────────────────
//                        Bitmap-шрифт 5×7
// ─────────────────────────────────────────────────────────────────────────────

/// Рисует строку текста (цифры 0–9) на холсте.
///
/// Неизвестные символы пропускаются без ошибки.
/// Пиксели, выходящие за границы холста, не рисуются.
///
/// # Параметры
/// - `text`: строка для отрисовки.
/// - `x`, `y`: координаты левого верхнего угла первого символа на холсте.
fn _draw_text(canvas: &mut RgbaImage, text: &str, x: u32, y: u32) {
    let clip_w = canvas.width();
    let clip_h = canvas.height();
    let mut cx = x;
    for ch in text.chars() {
        _draw_char(canvas, ch, cx, y, clip_w, clip_h);
        cx += GLYPH_W + GLYPH_GAP;
    }   // for ch
}   // _draw_text()

/// Рисует один символ bitmap-шрифта на холсте.
///
/// # Параметры
/// - `ch`: символ для отрисовки.
/// - `x`, `y`: координаты левого верхнего угла глифа на холсте.
/// - `clip_w`, `clip_h`: границы отсечения (пиксели за ними не рисуются).
fn _draw_char(
    canvas: &mut RgbaImage,
    ch: char,
    x: u32, y: u32,
    clip_w: u32, clip_h: u32,
) {
    let glyph = match _glyph(ch) {
        Some(g) => g,
        None => return, // Неподдерживаемый символ — пропускаем.
    };

    for row in 0..GLYPH_H {
        let py = y + row;
        if py >= clip_h { break; }

        for col in 0..GLYPH_W {
            let px = x + col;
            if px >= clip_w { break; }

            // Бит глифа: бит 4 (MSB из нижних 5) = левый пиксель,
            // бит 0 (LSB) = правый пиксель.
            if (glyph[row as usize] >> (GLYPH_W - 1 - col)) & 1 != 0 {
                canvas.put_pixel(
                    px, py,
                    Rgba([LABEL_RGB[0], LABEL_RGB[1], LABEL_RGB[2], 255]),
                );
            }   // if
        }   // for col
    }   // for row
}   // _draw_char()

/// Возвращает bitmap-глиф символа (5 пикселей × 7 строк).
///
/// Каждый элемент массива — одна строка глифа (7 строк, сверху вниз).
/// В байте используются биты 4..0: бит 4 = левый пиксель, бит 0 = правый.
///
/// Поддерживаемые символы: '0'–'9'.
fn _glyph(ch: char) -> Option<[u8; 7]> {
    match ch {
        '0' => Some([
            0b01110,  //  .XXX.
            0b10001,  //  X...X
            0b10011,  //  X..XX
            0b10101,  //  X.X.X
            0b11001,  //  XX..X
            0b10001,  //  X...X
            0b01110,  //  .XXX.
        ]),
        '1' => Some([
            0b00100,  //  ..X..
            0b01100,  //  .XX..
            0b00100,  //  ..X..
            0b00100,  //  ..X..
            0b00100,  //  ..X..
            0b00100,  //  ..X..
            0b01110,  //  .XXX.
        ]),
        '2' => Some([
            0b01110,  //  .XXX.
            0b10001,  //  X...X
            0b00001,  //  ....X
            0b00110,  //  ..XX.
            0b01000,  //  .X...
            0b10000,  //  X....
            0b11111,  //  XXXXX
        ]),
        '3' => Some([
            0b01110,  //  .XXX.
            0b10001,  //  X...X
            0b00001,  //  ....X
            0b00110,  //  ..XX.
            0b00001,  //  ....X
            0b10001,  //  X...X
            0b01110,  //  .XXX.
        ]),
        '4' => Some([
            0b00010,  //  ...X.
            0b00110,  //  ..XX.
            0b01010,  //  .X.X.
            0b10010,  //  X..X.
            0b11111,  //  XXXXX
            0b00010,  //  ...X.
            0b00010,  //  ...X.
        ]),
        '5' => Some([
            0b11111,  //  XXXXX
            0b10000,  //  X....
            0b11110,  //  XXXX.
            0b00001,  //  ....X
            0b00001,  //  ....X
            0b10001,  //  X...X
            0b01110,  //  .XXX.
        ]),
        '6' => Some([
            0b00110,  //  ..XX.
            0b01000,  //  .X...
            0b10000,  //  X....
            0b11110,  //  XXXX.
            0b10001,  //  X...X
            0b10001,  //  X...X
            0b01110,  //  .XXX.
        ]),
        '7' => Some([
            0b11111,  //  XXXXX
            0b00001,  //  ....X
            0b00010,  //  ...X.
            0b00100,  //  ..X..
            0b01000,  //  .X...
            0b01000,  //  .X...
            0b01000,  //  .X...
        ]),
        '8' => Some([
            0b01110,  //  .XXX.
            0b10001,  //  X...X
            0b10001,  //  X...X
            0b01110,  //  .XXX.
            0b10001,  //  X...X
            0b10001,  //  X...X
            0b01110,  //  .XXX.
        ]),
        '9' => Some([
            0b01110,  //  .XXX.
            0b10001,  //  X...X
            0b10001,  //  X...X
            0b01111,  //  .XXXX
            0b00001,  //  ....X
            0b00010,  //  ...X.
            0b01100,  //  .XX..
        ]),
        _ => None,
    }   // match
}   // _glyph()