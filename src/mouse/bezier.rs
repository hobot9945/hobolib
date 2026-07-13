//! bezier.rs
//! Алгоритм Безье для плавного перемещения указателя мыши.

use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use crate::mouse::{get_cursor_position, set_cursor_position};

// 1.0 — текущая “базовая” скорость (как сейчас).
// Если поставить 0.5 — движение станет в 2 раза медленнее.
// Если поставить 2.0 — в 2 раза быстрее.
const MOUSE_MOVE_SPEED: f64 = 1.0;

/// Плавно перемещает курсор в заданную точку.
///
/// # Идея алгоритма
/// - Траектория: кубическая кривая Безье P0->P3 с двумя контрольными точками P1/P2.
///   Контрольные точки ставим вдоль линии движения + небольшой перпендикулярный “увод”
///   (чтобы движение не выглядело как идеальная линейка).
/// - Скорость: easing (smoothstep) — плавный разгон в начале и плавное торможение в конце.
/// - Тайминг: длительность зависит от расстояния (чем дальше — тем дольше), но с клампом.
///
/// # Параметры
/// - `target_x`, `target_y`: координаты назначения в пикселях (виртуальный рабочий стол).
///
/// # Ошибки
/// Возвращает `Err(String)`, если не удалось получить текущую позицию или переместить курсор.
pub(super) fn move_cursor_smooth_to(target_x: i32, target_y: i32) -> Result<(), String> {

    // 1) Стартовая позиция.
    let (start_x, start_y) = get_cursor_position()?;

    // 2) Если уже почти на месте — не мучаем таймингом.
    let dx = (target_x - start_x) as f64;
    let dy = (target_y - start_y) as f64;
    let dist = (dx * dx + dy * dy).sqrt();

    if dist < 1.5 {
        set_cursor_position(target_x, target_y)?;
        return Ok(());
    }   // if

    // 3) Оценка длительности движения.
    //
    // Это НЕ “физика” и не строгий закон: просто удобный хак.
    // Смысл:
    // - маленькие расстояния: быстро (но не мгновенно, чтобы глаз успевал увидеть движение),
    // - большие расстояния: заметно дольше, но с верхним пределом.
    //
    // При желании позже можно заменить на модель в стиле Fitts’s law (время ~ a + b*log2(D/W + 1)).
    let duration_ms = _calc_move_duration_ms(dist);
    let duration = Duration::from_millis(duration_ms);

    // 4) Количество шагов.
    //
    // Чем больше шагов — тем плавнее, но тем больше вызовов SetCursorPos.
    // Здесь целимся примерно в 120 Гц максимум (шаг ~8мс), но не меньше минимума.
    let mut steps = (duration_ms / 8).max(20) as u32;

    // Если замедляем движение, duration_ms растёт, а ограничение 400 начинает “душить” плавность.
    // Масштабируем max_steps тем же коэффициентом.
    let speed = MOUSE_MOVE_SPEED.max(0.01);
    let max_steps = (400.0 / speed).round() as u32;

    // не даём max_steps стать меньше базовых 400 при speed>1
    steps = steps.min(max_steps.max(400));

    // 5) Подготовка контрольных точек кубической Безье.
    // P0 = старт, P3 = цель.
    let p0 = (start_x as f64, start_y as f64);
    let p3 = (target_x as f64, target_y as f64);

    // Сид для “рандома” без rand crate: достаточно для лёгкого разнообразия траектории.
    let mut rng = _seed_u32(start_x, start_y, target_x, target_y);

    let (p1, p2) = _make_bezier_control_points(p0, p3, dist, &mut rng);

    // 6) Тайминг: двигаемся по расписанию, чтобы суммарная длительность совпала с duration.
    let t0 = Instant::now();

    for i in 1..=steps {

        // t — прогресс по времени/шагам (0..1).
        let t = (i as f64) / (steps as f64);

        // easing — “плавный” прогресс (разгон/торможение).
        //
        // smoothstep: t*t*(3 - 2*t)
        // - 0..1
        // - производная = 0 в t=0 и t=1 (нет “рывка”).
        let te = _smoothstep(t);

        // Точка на кривой.
        let (x, y) = _cubic_bezier(p0, p1, p2, p3, te);

        // Переводим в i32. Тут важен round, чтобы на длинных траекториях не копилась ошибка.
        let xi = x.round() as i32;
        let yi = y.round() as i32;

        // Двигаем курсор.
        set_cursor_position(xi, yi)?;

        // Пауза до следующего шага.
        //
        // Важно: sleep делаем от “идеального расписания” (t0 + i/steps*duration),
        // чтобы не накапливать дрейф из-за неточности sleep/планировщика.
        let target_elapsed = _mul_duration_f64(duration, t);
        let elapsed = t0.elapsed();

        if target_elapsed > elapsed {
            sleep(target_elapsed - elapsed);
        }   // if

    }   // for

    // 7) Финальная фиксация в точку (на случай округлений/дребезга тайминга).
    set_cursor_position(target_x, target_y)?;

    Ok(())
}   // move_cursor_smooth_to()

//--------------------------------------------------------------------------------------------------
//                  Внутренние утилиты: тайминг, псевдо-rng, математика траектории
//--------------------------------------------------------------------------------------------------

/// Описание: Оценка длительности движения мыши по расстоянию.
///
/// Возвращаемое значение: миллисекунды (кламп).
fn _calc_move_duration_ms(dist: f64) -> u64 {

    // Базовая длительность, чтобы даже короткие движения были заметны глазом.
    let base = 90.0;

    // Масштаб: чем больше расстояние — тем больше прибавка.
    // sqrt даёт “затухающую” зависимость (рост не слишком резкий).
    let k = 28.0;

    let ms = base + k * dist.sqrt();

    // Ограничения: слишком быстро/слишком долго — неприятно.
    let ms = ms.clamp(80.0, 900.0);

    // Масштабируем время движения через единую “скорость”.
    let speed = MOUSE_MOVE_SPEED.max(0.01); // защита от 0 и очень малых значений
    let ms = ms / speed;

    ms.round() as u64
}   // _calc_move_duration_ms()

/// Описание: smoothstep easing.
///
/// Вход/выход: 0..1 -> 0..1.
fn _smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}   // _smoothstep()

/// Описание: Кубическая кривая Безье.
///
/// P(t) = (1-t)^3 * P0 + 3(1-t)^2 t * P1 + 3(1-t) t^2 * P2 + t^3 * P3
fn _cubic_bezier(
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
    t: f64
) -> (f64, f64) {

    let t = t.clamp(0.0, 1.0);
    let u = 1.0 - t;

    let b0 = u * u * u;
    let b1 = 3.0 * u * u * t;
    let b2 = 3.0 * u * t * t;
    let b3 = t * t * t;

    let x = b0 * p0.0 + b1 * p1.0 + b2 * p2.0 + b3 * p3.0;
    let y = b0 * p0.1 + b1 * p1.1 + b2 * p2.1 + b3 * p3.1;

    (x, y)
}   // _cubic_bezier()

/// Описание: Строит контрольные точки P1/P2 для траектории.
///
/// Мы берём точки примерно на 1/3 и 2/3 пути и добавляем перпендикулярный увод.
/// Это даёт “мягкую дугу”, визуально похожую на человеческую.
fn _make_bezier_control_points(
    p0: (f64, f64),
    p3: (f64, f64),
    dist: f64,
    rng: &mut u32
) -> ((f64, f64), (f64, f64)) {

    let (x0, y0) = p0;
    let (x3, y3) = p3;

    // Вектор направления движения.
    let vx = x3 - x0;
    let vy = y3 - y0;

    // Нормализованный перпендикуляр (для “увода”).
    // Перпендикуляр к (vx, vy) — это (-vy, vx).
    let inv_len = if dist > 0.0001 { 1.0 / dist } else { 0.0 };
    let px = -vy * inv_len;
    let py =  vx * inv_len;

    // Амплитуда бокового увода.
    // Чем дальше цель — тем больше можно “дугу”, но с клампом.
    let amp = (dist * 0.12).clamp(6.0, 80.0);

    // Случайный знак и небольшая вариативность амплитуды.
    let sign = if _rand_f64_0_1(rng) < 0.5 { -1.0 } else { 1.0 };
    let amp_scale = 0.6 + 0.8 * _rand_f64_0_1(rng); // 0.6..1.4

    // Сама величина увода.
    let offset = sign * amp * amp_scale;

    // Дополнительно: немного смещаем “доли” 1/3 и 2/3, чтобы траектория не была идеально одинаковой.
    let t1 = 0.32 + 0.10 * _rand_f64_0_1(rng); // 0.32..0.42
    let t2 = 0.58 + 0.10 * _rand_f64_0_1(rng); // 0.58..0.68

    // Базовые точки на прямой.
    let base1 = (x0 + vx * t1, y0 + vy * t1);
    let base2 = (x0 + vx * t2, y0 + vy * t2);

    // Добавляем перпендикулярный увод.
    let p1 = (base1.0 + px * offset, base1.1 + py * offset);
    let p2 = (base2.0 + px * offset, base2.1 + py * offset);

    (p1, p2)
}   // _make_bezier_control_points()

/// Описание: seed для псевдо-rng без внешних crate.
///
/// Важно: это не криптография и не “настоящий random”, тут задача простая:
/// чтобы траектории были чуть разными между вызовами.
fn _seed_u32(sx: i32, sy: i32, tx: i32, ty: i32) -> u32 {

    let time_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_nanos() as u64;

    // Простая мешалка: координаты + время.
    let mut v = (time_ns as u32)
        ^ (sx as u32).rotate_left(7)
        ^ (sy as u32).rotate_left(13)
        ^ (tx as u32).rotate_left(19)
        ^ (ty as u32).rotate_left(23);

    // Ноль для xorshift плохой — подкручиваем.
    if v == 0 {
        v = 0xA5A5_1234;
    }   // if

    v
}   // _seed_u32()

/// Описание: xorshift32.
///
/// Дешёвый псевдо-rng, хватает для “джиттера” траектории.
fn _xorshift32(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}   // _xorshift32()

/// Описание: псевдо-рандом 0..1.
fn _rand_f64_0_1(state: &mut u32) -> f64 {
    let v = _xorshift32(state);
    (v as f64) / (u32::MAX as f64)
}   // _rand_f64_0_1()

/// Описание: Умножает Duration на коэффициент 0..1.
///
/// Нужна для тайминга “по расписанию” внутри цикла.
fn _mul_duration_f64(d: Duration, k: f64) -> Duration {
    let k = k.clamp(0.0, 1.0);
    let ns = d.as_nanos() as f64;
    let out = (ns * k).round();

    // Защита от отрицательных/NaN не нужна: k клампнут.
    Duration::from_nanos(out as u64)
}   // _mul_duration_f64()