//! mouse.rs — Эмуляция мыши (позиционирование и клики).
//!
//! # ОПИСАНИЕ
//! Модуль предоставляет низкоуровневые функции для:
//! - чтения текущей позиции курсора;
//! - перемещения курсора в абсолютные координаты виртуального рабочего стола;
//! - генерации кликов через `SendInput`.
//!
//! # ОТВЕТСТВЕННОСТЬ
//! - `get_cursor_position()` — координаты курсора (виртуальный рабочий стол).
//! - `set_cursor_position()` — позиционирование курсора через `SetCursorPos`.
//! - `send_*click()` — одиночные/двойные клики ЛКМ/ПКМ.
//! - `_send_left_down/_send_left_up` — low-level примитивы для drag-and-drop (внутреннее API).
//!
//! # ВАЖНО (про скриншоты и изображение курсора)
//! Извлечение изображения курсора (RGBA + hotspot) и определение видимости курсора
//! **вынесено в модуль захвата скриншотов**
//! `library::screenshot::capture_backend` (локальный `mod mouse_tool`).
//! Здесь этого кода намеренно нет, чтобы не смешивать “инпут-эмуляцию” и “графический захват”.

mod bezier;

use std::thread::sleep;
use std::time::Duration;
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
use windows::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEINPUT, MOUSE_EVENT_FLAGS, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL};
use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;
use windows::core::Error as WinError;
use windows::Win32::Foundation::GetLastError;

//--------------------------------------------------------------------------------------------------
//                  Эмуляция действий мыши
//--------------------------------------------------------------------------------------------------

/// Получает текущую позицию курсора мыши.
///
/// # Алгоритм работы
/// - Вызывает Win32 API `GetCursorPos`.
/// - Возвращает координаты в системе виртуального рабочего стола.
///
/// # Возвращаемое значение
/// Тип: `(i32, i32)` — координаты `(x, y)` курсора.
///
/// # Координатная система
/// - Координаты относительно виртуального рабочего стола (все мониторы).
/// - Могут быть отрицательными (мониторы слева/сверху от основного).
/// - Основной монитор обычно имеет координаты (0, 0) в левом верхнем углу.
///
/// # Ошибки
/// Возвращает `Err(String)`, если Win32 API вернул ошибку.
pub fn get_cursor_position() -> Result<(i32, i32), String> {
    let mut point = POINT::default();

    unsafe {
        GetCursorPos(&mut point)
            .map_err(|e| format!("GetCursorPos failed: {}", e))?;
    }

    Ok((point.x, point.y))
}   // get_cursor_position()

/// Плавно перемещает курсор в заданную точку (виртуальный рабочий стол).
///
/// # Алгоритм
/// Делегирует работу подмодулю `bezier`:
/// - строится кубическая кривая Безье от текущей позиции к цели;
/// - применяется easing (плавный разгон/торможение);
/// - перемещение выполняется серией маленьких шагов через `_set_cursor_position()`.
///
/// # Почему это отдельный API
/// - `_set_cursor_position()` — примитив (мгновенный “телепорт”).
/// - `move_cursor_to_position()` — пользовательский “комфортный” вариант для UI-наблюдения.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - не удалось прочитать позицию курсора (внутри bezier),
/// - не удалось сдвинуть курсор на каком-то шаге (внутри bezier/_set_cursor_position).
pub fn move_cursor_to_position(x: i32, y: i32) -> Result<(), String> {
    bezier::move_cursor_smooth_to(x, y)
}   // move_cursor_to_position()

/// Прокручивает колесо мыши (вертикальный скролл).
///
/// # Параметры
/// - `pos`: Опциональная позиция курсора.
///   - `None` => прокрутка выполняется в текущей позиции курсора.
///   - `Some((x, y))` => сначала курсор плавно перемещается в (x, y), затем выполняется прокрутка.
/// - `lines`: Количество “строк” прокрутки:
///   - `lines > 0` => прокрутка вверх (wheel delta положительная),
///   - `lines < 0` => прокрутка вниз.
///
/// # Примечание про “строки”
/// WinAPI оперирует величиной `WHEEL_DELTA = 120` (один “шаг” колеса).
/// Здесь мы трактуем `lines` как количество таких шагов.
/// Реальная прокрутка в строках/пикселях уже зависит от настроек Windows и приложения.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - не удалось переместить курсор (если pos задан),
/// - `SendInput` не смог отправить события.
pub fn scroll(pos: Option<(i32, i32)>, lines: i32) -> Result<(), String> {

    // 1) Если указана точка — сначала подводим курсор (плавно).
    move_cursor_to(pos)?;

    // 2) Ноль — ничего делать не надо.
    if lines == 0 {
        return Ok(());
    }   // if

    // Промотать.
    _scroll(lines)
}   // scroll()

/// Одиночный левый клик.
/// Если `pos = Some((x, y))` — сначала плавно переместиться, затем кликнуть.
pub fn left_click(pos: Option<(i32, i32)>) -> Result<(), String> {
    move_cursor_to(pos)?;
    left_button_click()
}   // left_click()

/// Двойной левый клик.
/// Если `pos = Some((x, y))` — сначала плавно переместиться, затем двойной клик.
pub fn left_double_click(pos: Option<(i32, i32)>) -> Result<(), String> {
    move_cursor_to(pos)?;
    left_button_double_click()
}   // left_double_click()

/// Одиночный правый клик (контекстное меню).
/// Если `pos = Some((x, y))` — сначала плавно переместиться, затем кликнуть.
pub fn right_click(pos: Option<(i32, i32)>) -> Result<(), String> {
    move_cursor_to(pos)?;
    right_button_click()
}   // right_click()

/// Двойной правый клик (редко используется).
/// Если `pos = Some((x, y))` — сначала плавно переместиться, затем двойной клик.
pub fn right_double_click(pos: Option<(i32, i32)>) -> Result<(), String> {
    move_cursor_to(pos)?;
    right_button_double_click()
}   // right_double_click()

/// Перетаскивает объект из точки `pos_from` в точку `pos_to` (drag-and-drop).
///
/// # Параметры
/// - `pos_from`: (x, y) — точка “захвата” (где нажимаем ЛКМ).
/// - `pos_to`: (x, y) — точка “сброса” (где отпускаем ЛКМ).
///
/// # Алгоритм работы (best effort)
/// 1) Плавно переместиться в `pos_from`.
/// 2) Нажать ЛКМ (`_send_left_down()`).
/// 3) Сделать небольшой “срыв” курсора на несколько пикселей в сторону `pos_to`,
///    чтобы превысить системный порог начала drag (Windows не всегда начинает drag,
///    если после down курсор вообще не сдвигался).
/// 4) Плавно переместиться в `pos_to` с зажатой ЛКМ.
/// 5) Отпустить ЛКМ (`_send_left_up()`).
///
/// # Почему нужен “срыв”
/// В Windows drag-and-drop обычно не стартует на `WM_LBUTTONDOWN` сразу.
/// Приложения часто ждут `WM_MOUSEMOVE` с зажатой кнопкой и смещением >
/// системного порога (DragThreshold).
///
/// # Ошибки
/// Возвращает `Err(String)`, если не удалось выполнить любое из действий (move/down/up).
///
/// # Побочные эффекты
/// - Двигает курсор.
/// - Генерирует события мыши (ЛКМ down/up).
pub fn drag(pos_from: (i32, i32), pos_to: (i32, i32)) -> Result<(), String> {

    // 1) Подвести курсор к точке захвата (плавно).
    move_cursor_to_position(pos_from.0, pos_from.1)?;

    // Небольшая пауза, чтобы GUI успел обработать наведении/hover.
    sleep(Duration::from_millis(40));

    // 2) Нажать ЛКМ и удерживать.
    left_button_down()?;

    // Пауза после down: некоторые UI (особенно тяжёлые) реагируют стабильнее.
    sleep(Duration::from_millis(35));

    // 3) “Сорвать” курсор на несколько пикселей, чтобы гарантированно начался drag.
    //    Делаем это мгновенно (не плавно), чтобы не размазывать начало жеста.
    let (nx, ny) = _calc_drag_nudge_point(pos_from, pos_to, 6.0);
    set_cursor_position(nx, ny)?;

    // Короткая пауза, чтобы приложение зафиксировало начало перетаскивания.
    sleep(Duration::from_millis(20));

    // 4) Протянуть до точки назначения (плавно, с зажатой ЛКМ).
    move_cursor_to_position(pos_to.0, pos_to.1)?;

    // Пауза перед отпусканием — иногда помогает, если target-элемент “подсвечивается” с задержкой.
    sleep(Duration::from_millis(25));

    // 5) Отпустить ЛКМ.
    left_button_up()?;

    Ok(())
}   // drag()

/// Перемещает курсор мыши в указанные экранные координаты.
///
/// # Параметры
/// - `x`: Координата X в пикселях (виртуальный рабочий стол).
/// - `y`: Координата Y в пикселях (виртуальный рабочий стол).
///
/// # Координатная система
/// - Координаты относительно виртуального рабочего стола (все мониторы).
/// - Могут быть отрицательными (мониторы слева/сверху от основного).
/// - (0, 0) — левый верхний угол основного монитора.
///
/// # Ошибки
/// Возвращает `Err(String)`, если Win32 API `SetCursorPos` вернул ошибку.
///
/// # Побочные эффекты
/// - Мгновенно перемещает курсор (без анимации).
/// - Может вызвать события WM_MOUSEMOVE в окнах под курсором.
pub fn set_cursor_position(x: i32, y: i32) -> Result<(), String> {
    unsafe {
        SetCursorPos(x, y)
            .map_err(|e| format!("SetCursorPos({}, {}) failed: {}", x, y, e))
    }
}   // set_cursor_position()

/// Если точка задана — плавно перемещаем курсор в неё.
/// Если None — ничего не делаем.
///
/// # Зачем отдельная функция
/// Чтобы не дублировать один и тот же шаблон:
/// `if let Some((x, y)) { move_cursor_smooth_to(x, y)?; }`
/// во всех кликах/действиях.
///
/// # Ошибки
/// - Пробрасывает ошибки чтения/движения курсора из bezier/mouse.
pub fn move_cursor_to(pos: Option<(i32, i32)>) -> Result<(), String> {
    if let Some((x, y)) = pos {
        // Плавное движение по Безье (реализация в mod bezier).
        // Внутри уже есть:
        // - early-exit, если расстояние маленькое;
        // - финальная фиксация в точку.
        bezier::move_cursor_smooth_to(x, y)?;
    }   // if

    Ok(())
}   // _move_to_if_requested()

/// Выполняет одиночный клик левой кнопкой мыши в текущей позиции курсора.
///
/// # Алгоритм работы
/// - Отправляет событие LEFTDOWN (нажатие).
/// - Отправляет событие LEFTUP (отпускание).
/// - Оба события отправляются одним вызовом SendInput для атомарности.
///
/// # Ошибки
/// Возвращает `Err(String)`, если SendInput не смог отправить события.
pub fn left_button_click() -> Result<(), String> {
    let inputs = [
        _make_mouse_input(MOUSEEVENTF_LEFTDOWN),
        _make_mouse_input(MOUSEEVENTF_LEFTUP),
    ];
    _send_mouse_inputs(&inputs)
}   // send_left_click()

/// Выполняет двойной клик левой кнопкой мыши в текущей позиции курсора.
///
/// # Алгоритм работы
/// - Отправляет два последовательных клика (DOWN+UP, DOWN+UP).
/// - Windows автоматически распознаёт двойной клик по интервалу между событиями.
/// - Интервал определяется системной настройкой (обычно 500 мс, см. `GetDoubleClickTime`).
///
/// # Когда использовать
/// - Открытие файлов/папок в проводнике.
/// - Выделение слова в текстовом редакторе.
/// - Активация элементов интерфейса с double-click семантикой.
///
/// # Ошибки
/// Возвращает `Err(String)`, если SendInput не смог отправить события.
pub fn left_button_double_click() -> Result<(), String> {
    let inputs = [
        _make_mouse_input(MOUSEEVENTF_LEFTDOWN),
        _make_mouse_input(MOUSEEVENTF_LEFTUP),
        _make_mouse_input(MOUSEEVENTF_LEFTDOWN),
        _make_mouse_input(MOUSEEVENTF_LEFTUP),
    ];
    _send_mouse_inputs(&inputs)
}   // send_left_double_click()

/// Выполняет одиночный клик правой кнопкой мыши в текущей позиции курсора.
///
/// # Типичное использование
/// - Вызов контекстного меню.
///
/// # Ошибки
/// Возвращает `Err(String)`, если SendInput не смог отправить события.
pub fn right_button_click() -> Result<(), String> {
    let inputs = [
        _make_mouse_input(MOUSEEVENTF_RIGHTDOWN),
        _make_mouse_input(MOUSEEVENTF_RIGHTUP),
    ];
    _send_mouse_inputs(&inputs)
}   // send_right_click()

/// Выполняет двойной клик правой кнопкой мыши в текущей позиции курсора.
///
/// # Примечание
/// Двойной правый клик редко используется в стандартных приложениях Windows.
/// Некоторые приложения (например, файловые менеджеры) могут назначать ему
/// специальные действия.
///
/// # Ошибки
/// Возвращает `Err(String)`, если SendInput не смог отправить события.
pub fn right_button_double_click() -> Result<(), String> {
    let inputs = [
        _make_mouse_input(MOUSEEVENTF_RIGHTDOWN),
        _make_mouse_input(MOUSEEVENTF_RIGHTUP),
        _make_mouse_input(MOUSEEVENTF_RIGHTDOWN),
        _make_mouse_input(MOUSEEVENTF_RIGHTUP),
    ];
    _send_mouse_inputs(&inputs)
}   // send_right_double_click()

/// Отправляет событие нажатия левой кнопки мыши (без отпускания).
///
/// # Назначение
/// Используется для реализации операции **drag-and-drop** (перетаскивания).
///
/// # Как работает drag-and-drop на низком уровне
/// Перетаскивание объекта из точки A в точку B состоит из трёх шагов:
///
/// 1. **Захват объекта**: Переместить курсор в точку A, вызвать `_send_left_down()`.
///    - Система переводит окно под курсором в режим "захвата" (capture).
///    - Окно начинает получать все события мыши, даже если курсор выходит за его границы.
///
/// 2. **Перемещение**: Переместить курсор в точку B через `set_cursor_position()`.
///    - Система генерирует события WM_MOUSEMOVE с флагом MK_LBUTTON.
///    - Приложение отрисовывает "призрак" перетаскиваемого объекта.
///
/// 3. **Отпускание**: Вызвать `_send_left_up()`.
///    - Система отправляет WM_LBUTTONUP.
///    - Приложение завершает операцию (перемещает файл, переупорядочивает элемент и т.п.).
///
/// # Важные замечания
/// - Между `_send_left_down()` и `_send_left_up()` можно вызывать `set_cursor_position()`
///   многократно для плавного перемещения или промежуточных точек.
/// - Если `_send_left_up()` не вызван, система остаётся в режиме захвата, что может
///   привести к "залипанию" кнопки мыши.
/// - Некоторые приложения требуют небольшую задержку между down и первым move
///   (порядка 50-100 мс) для корректной инициализации drag-and-drop.
///
/// # Ошибки
/// Возвращает `Err(String)`, если SendInput не смог отправить событие.
pub fn left_button_down() -> Result<(), String> {
    let inputs = [_make_mouse_input(MOUSEEVENTF_LEFTDOWN)];
    _send_mouse_inputs(&inputs)
}   // _send_left_down()

/// Отправляет событие отпускания левой кнопки мыши.
///
/// # Назначение
/// Завершает операцию захвата, начатую `_send_left_down()`.
/// См. документацию `_send_left_down()` для полного описания drag-and-drop.
///
/// # Поведение
/// - Если левая кнопка не была нажата, событие всё равно отправляется.
/// - Большинство приложений игнорируют "лишние" UP без предшествующего DOWN.
///
/// # Ошибки
/// Возвращает `Err(String)`, если SendInput не смог отправить событие.
pub fn left_button_up() -> Result<(), String> {
    let inputs = [_make_mouse_input(MOUSEEVENTF_LEFTUP)];
    _send_mouse_inputs(&inputs)
}   // _send_left_up()

//--------------------------------------------------------------------------------------------------
//                  Внутренний интерфейс, низкоуровневые функции: формирование и отправка INPUT
//--------------------------------------------------------------------------------------------------

/// Формирует структуру INPUT для события мыши.
///
/// # Параметры
/// - `flags`: Флаги события (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP и т.п.).
///
/// # Возвращаемое значение
/// Тип: INPUT: Структура события для SendInput.
///
/// # Примечание
/// Поля dx, dy = 0 означают "текущая позиция курсора".
/// Для перемещения курсора через SendInput нужен флаг MOUSEEVENTF_MOVE
/// и/или MOUSEEVENTF_ABSOLUTE, но мы используем SetCursorPos вместо этого.
fn _make_mouse_input(flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}   // _make_mouse_input()

/// Отправляет массив событий мыши через SendInput.
///
/// # Параметры
/// - `inputs`: Срез структур INPUT с событиями мыши.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - SendInput вернул 0 (ошибка, читаем GetLastError).
/// - SendInput отправил не все события (частичная отправка).
fn _send_mouse_inputs(inputs: &[INPUT]) -> Result<(), String> {
    let sent = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };

    if sent == 0 {
        let win32 = unsafe { GetLastError() };
        let e = WinError::from(win32);
        return Err(format!("SendInput (mouse) вернул 0: {}", e));
    }   // if

    if sent as usize != inputs.len() {
        return Err(format!(
            "SendInput (mouse) отправил не все события: {}/{}",
            sent,
            inputs.len()
        ));
    }   // if

    Ok(())
}   // _send_mouse_inputs()

/// Вычисляет точку небольшого смещения от `from` в сторону `to`.
///
/// # Параметры
/// - `from`: точка старта.
/// - `to`: точка назначения.
/// - `nudge_px`: величина смещения в пикселях.
///
/// # Зачем
/// Нужен минимальный move, чтобы Windows/приложение распознали drag.
///
/// # Возвращаемое значение
/// `(x, y)` — точка для “срыва” курсора.
fn _calc_drag_nudge_point(from: (i32, i32), to: (i32, i32), nudge_px: f64) -> (i32, i32) {

    let dx = (to.0 - from.0) as f64;
    let dy = (to.1 - from.1) as f64;

    // Если from==to или очень близко, двигаем вправо-вниз, чтобы точно было смещение.
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        return (from.0 + nudge_px.round() as i32, from.1 + nudge_px.round() as i32);
    }   // if

    // Нормализуем направление и делаем смещение на nudge_px.
    let ux = dx / len;
    let uy = dy / len;

    let nx = (from.0 as f64 + ux * nudge_px).round() as i32;
    let ny = (from.1 as f64 + uy * nudge_px).round() as i32;

    (nx, ny)
}   // _calc_drag_nudge_point()

/// Формирует INPUT для прокрутки колеса мыши.
///
/// # Параметры
/// - `wheel_delta`: signed delta прокрутки.
///   - `+120` = один “шаг” вверх,
///   - `-120` = один “шаг” вниз.
///
/// # Важно про mouseData
/// В структуре `MOUSEINPUT` поле `mouseData` — `u32`, а wheel delta — знаковое.
/// WinAPI ожидает двухкомплементарное представление, поэтому используем `as u32`:
/// отрицательные значения корректно превращаются в DWORD.
fn _make_mouse_wheel_input(wheel_delta: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: wheel_delta as u32, // signed -> DWORD (two's complement)
                dwFlags: MOUSEEVENTF_WHEEL,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}   // _make_mouse_wheel_input()

/// Прокручивает колесо мыши (вертикальный скролл) в текущей позиции курсора.
///
/// # Параметры
/// - `lines`: Количество “шагов” колеса:
///   - `lines > 0` => прокрутка вверх,
///   - `lines < 0` => прокрутка вниз,
///   - `lines = 0` => ничего не делает.
///
/// # Примечание
/// В WinAPI один “шаг” колеса — `WHEEL_DELTA = 120`.
/// Здесь `lines` трактуется как количество таких шагов.
///
/// # Ошибки
/// Возвращает `Err(String)`, если `SendInput` не смог отправить события.
fn _scroll(lines: i32) -> Result<(), String> {

    // 0) Ноль — ничего делать не надо.
    if lines == 0 {
        return Ok(());
    }   // if

    // 3) В WinAPI направление задаётся знаком wheel delta:
    //    +delta => обычно “вверх”, -delta => “вниз”.
    const WHEEL_DELTA_I32: i32 = 120;

    let dir = if lines > 0 { 1 } else { -1 };
    let mut remaining = lines.abs() as u32;

    // 4) Важно: некоторые приложения/контролы лучше реагируют на серию “малых” wheel-событий,
    //    чем на одно событие с огромным delta. Поэтому делаем серию событий по 1 шагу.
    //
    //    Чтобы не посылать гигантские массивы INPUT, шлём батчами.
    while remaining > 0 {

        // Размер пачки. Можно менять. 32 — компромисс между скоростью и “человечностью”.
        let batch = remaining.min(32);

        // Собираем пачку wheel-событий.
        let mut inputs: Vec<INPUT> = Vec::with_capacity(batch as usize);
        for _ in 0..batch {
            inputs.push(_make_mouse_wheel_input(dir * WHEEL_DELTA_I32));
        }   // for

        // Отправляем пачку одним SendInput.
        _send_mouse_inputs(&inputs)?;

        remaining -= batch;
    }   // while

    Ok(())
}   // _scroll()
