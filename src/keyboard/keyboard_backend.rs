//! keyboard.rs
//!
//! ОПИСАНИЕ:
//! Низкоуровневая эмуляция клавиатуры через WinAPI.
//!
//! ОТВЕТСТВЕННОСТЬ:
//! - Отправка одиночных нажатий и комбинаций клавиш в текущий foreground/focused ввод через SendInput.
//!
//! ПРИМЕЧАНИЯ:
//! - Реализация использует виртуальные коды клавиш (VK). Для "обычных окон" этого достаточно.
//! - Если когда-нибудь понадобится "как физическая клавиатура независимо от layout", это делается через scancode
//!   и флаг KEYEVENTF_SCANCODE (см. KEYBDINPUT).

#![allow(unused)]
use windows::core::Error as WinError;
use windows::Win32::Foundation::GetLastError;
use windows::Win32::UI::Input::KeyboardAndMouse::{MapVirtualKeyW, SendInput, INPUT, INPUT_0,
                                                  INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
                                                  KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAPVK_VK_TO_VSC,
                                                  VIRTUAL_KEY};

/// Описание: Нажимает и отпускает клавишу (press) по виртуальному коду.
///
/// # Параметры
/// - `vk`: Виртуальный код клавиши.
///
/// # Ошибки
/// Возвращает `Err(String)`, если SendInput не смог отправить события.
pub(super) fn send_key_press(vk: VIRTUAL_KEY) -> Result<(), String> {
    let inputs = [_make_key_input(vk, false), _make_key_input(vk, true)];
    _send_inputs(&inputs)
}   // send_key_press()

/// Описание: Нажимает клавишу (key down) по виртуальному коду.
///
/// # Параметры
/// - `vk`: Виртуальный код клавиши.
///
/// # Ошибки
/// Возвращает `Err(String)`, если SendInput не смог отправить событие.
pub(super) fn send_key_down(vk: VIRTUAL_KEY) -> Result<(), String> {
    let inputs = [_make_key_input(vk, false)];
    _send_inputs(&inputs)
}   // send_key_down()

/// Описание: Отпускает клавишу (key up) по виртуальному коду.
///
/// # Параметры
/// - `vk`: Виртуальный код клавиши.
///
/// # Ошибки
/// Возвращает `Err(String)`, если SendInput не смог отправить событие.
pub(super) fn send_key_up(vk: VIRTUAL_KEY) -> Result<(), String> {
    let inputs = [_make_key_input(vk, true)];
    _send_inputs(&inputs)
}   // send_key_up()

/// Описание: Отправляет комбинацию вида "модификаторы + основная клавиша".
///
/// Пример: Ctrl+V, Alt+F4, Ctrl+Shift+S и т.п.
///
/// # Алгоритм работы
/// - Нажимаем все модификаторы в указанном порядке.
/// - Нажимаем основную клавишу.
/// - Отпускаем основную клавишу.
/// - Отпускаем модификаторы в обратном порядке (чтобы стек модификаторов корректно размотался).
///
/// # Параметры
/// - `modifiers`: Срез виртуальных кодов модификаторов (Ctrl/Shift/Alt/Win и т.п.).
/// - `key`: Виртуальный код основной клавиши.
///
/// # Ошибки
/// Возвращает `Err(String)`, если SendInput не смог отправить события.
pub(super) fn send_key_combo(modifiers: &[VIRTUAL_KEY], key: VIRTUAL_KEY) -> Result<(), String> {
    let mut inputs: Vec<INPUT> = Vec::with_capacity(modifiers.len() * 2 + 2);

    for &m in modifiers {
        inputs.push(_make_key_input(m, false));
    }   // for

    inputs.push(_make_key_input(key, false));
    inputs.push(_make_key_input(key, true));

    for &m in modifiers.iter().rev() {
        inputs.push(_make_key_input(m, true));
    }   // for

    _send_inputs(&inputs)
}   // send_key_combo()

//--------------------------------------------------------------------------------------------------
//                  Внутренний интерфейс
//--------------------------------------------------------------------------------------------------

/// Описание: Универсальная отправка массива INPUT через SendInput.
///
/// # Параметры
/// - `inputs`: Массив событий клавиатуры/мыши.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - SendInput вернул 0 (ошибка, читаем GetLastError).
/// - SendInput отправил не все события.
fn _send_inputs(inputs: &[INPUT]) -> Result<(), String> {
    let sent = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };

    if sent == 0 {
        let win32 = unsafe { GetLastError() };
        let e = WinError::from(win32);
        return Err(format!("SendInput вернул 0: {}", e));
    }   // if

    if sent as usize != inputs.len() {
        return Err(format!("SendInput отправил не все события: {}/{}", sent, inputs.len()));
    }   // if

    Ok(())
}   // send_inputs()

/// Описание: Строит INPUT для клавиатурного события по виртуальному коду.
///
/// Передаёт аппаратный скан-код с флагом KEYEVENTF_SCANCODE.
/// Для extended-клавиш (Insert, Delete, Home, End, стрелки, и т.п.)
/// добавляет KEYEVENTF_EXTENDEDKEY.
///
/// # Параметры
/// - `vk`: Виртуальный код клавиши (используется для получения скан-кода).
/// - `is_key_up`: true => отпускание клавиши; false => нажатие клавиши.
///
/// # Возвращаемое значение
/// Type: INPUT: Структура события для SendInput.
fn _make_key_input(vk: VIRTUAL_KEY, is_key_up: bool) -> INPUT {

    let scan = _vk_to_scan(vk);

    let mut flags = KEYEVENTF_SCANCODE;

    if is_key_up {
        flags |= KEYEVENTF_KEYUP;
    }   // if

    if _is_extended_key(vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }   // if

    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}   // _make_key_input()

/// Описание: Определяет, является ли клавиша extended-клавишей.
///
/// Extended-клавиши — это клавиши, которые имеют одинаковый скан-код
/// с клавишами Numpad, но физически расположены отдельно. Без флага
/// KEYEVENTF_EXTENDEDKEY система не может их различить.
///
/// # Параметры
/// - `vk`: Виртуальный код клавиши.
///
/// # Возвращаемое значение
/// Тип: bool: true если клавиша extended.
fn _is_extended_key(vk: VIRTUAL_KEY) -> bool {
    matches!(
        vk.0,
        0x21 |  // VK_PRIOR     (Page Up)
        0x22 |  // VK_NEXT      (Page Down)
        0x23 |  // VK_END
        0x24 |  // VK_HOME
        0x25 |  // VK_LEFT
        0x26 |  // VK_UP
        0x27 |  // VK_RIGHT
        0x28 |  // VK_DOWN
        0x2D |  // VK_INSERT
        0x2E |  // VK_DELETE
        0x5B |  // VK_LWIN
        0x5C |  // VK_RWIN
        0x5D |  // VK_APPS      (Menu key)
        0xA1 |  // VK_RSHIFT
        0xA3 |  // VK_RCONTROL
        0xA5    // VK_RMENU     (Right Alt)
    )
}   // _is_extended_key()

/// Описание: Строит INPUT для клавиатурного события по виртуальному коду.
///
/// # Параметры
/// - `vk`: Виртуальный код клавиши.
/// - `is_key_up`: true => отпускание клавиши; false => нажатие клавиши.
///
/// # Возвращаемое значение
/// Type: INPUT: Структура события для SendInput.
// fn _make_key_input(vk: VIRTUAL_KEY, is_key_up: bool) -> INPUT {
//     let flags = if is_key_up { KEYEVENTF_KEYUP } else { Default::default() };
//
//     INPUT {
//         r#type: INPUT_KEYBOARD,
//         Anonymous: INPUT_0 {
//             ki: KEYBDINPUT {
//                 wVk: vk,
//                 wScan: _vk_to_scan(vk),
//                 dwFlags: flags,
//                 time: 0,
//                 dwExtraInfo: 0,
//             },
//         },
//     }
// }   // make_key_input()

/// Описание: Преобразует виртуальный код клавиши в аппаратный скан-код.
///
/// # Параметры
/// - `vk`: Виртуальный код клавиши.
///
/// # Возвращаемое значение
/// Тип: u16: Скан-код клавиши (0, если преобразование невозможно).
fn _vk_to_scan(vk: VIRTUAL_KEY) -> u16 {
    unsafe { MapVirtualKeyW(vk.0 as u32, MAPVK_VK_TO_VSC) as u16 }
}   // _vk_to_scan()