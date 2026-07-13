//! keyboard.rs

use windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY;
use crate::keyboard::keyboard_backend::{send_key_combo, send_key_press};

mod keyboard_backend;

/// Описание: Нажимает и отпускает клавишу по её VK-коду (press).
///
/// # Параметры
/// - `vk`: виртуальный код клавиши (VK_*), например:
///   - Ctrl = 0x11
///   - Enter = 0x0D
///
/// # Ошибки
/// Возвращает `Err(String)`, если SendInput не смог отправить события.
pub fn send_vk_press(vk: u16) -> Result<(), String> {
    send_key_press(VIRTUAL_KEY(vk))
}   // send_vk_press()

/// Описание: Отправляет комбинацию вида "модификаторы + основная клавиша" по VK-кодам.
///
/// Примеры:
/// - modifiers=[0x11], key=0x56  -> Ctrl+V
/// - modifiers=[0x11,0x10], key=0x53 -> Ctrl+Shift+S
///
/// # Параметры
/// - `modifiers`: VK-коды модификаторов (Ctrl/Shift/Alt/Win и т.п.)
/// - `key`: VK-код основной клавиши
///
/// # Ошибки
/// Возвращает `Err(String)`, если SendInput не смог отправить события.
pub fn send_vk_combo(modifiers: &[u16], key: u16) -> Result<(), String> {

    // Переводим u16 -> VIRTUAL_KEY. Нужен Vec, потому что send_key_combo принимает срез.
    let mods: Vec<VIRTUAL_KEY> = modifiers.iter().map(|&v| VIRTUAL_KEY(v)).collect();

    send_key_combo(&mods, VIRTUAL_KEY(key))

}   // send_vk_combo()

/// Описание: Отправляет нажатие Enter (press) в текущий фокус.
///
/// Использует виртуальный код VK_RETURN (0x0D), что достаточно для обычных окон
/// с текстовым вводом.
///
/// # Ошибки
/// Возвращает `Err(String)`, если SendInput не смог отправить события.
pub fn send_enter() -> Result<(), String> {

    let vk_return = VIRTUAL_KEY(0x0D);
    send_key_press(vk_return)
}   // send_enter()

/// Описание: Отправляет комбинацию Ctrl+Enter в текущий фокус.
///
/// Используется для принудительной отправки сообщений в веб-формах,
/// где обычный Enter может работать как перевод строки.
///
/// # Ошибки
/// Возвращает `Err(String)`, если SendInput вернул 0 или отправил не все события.
pub fn send_ctrl_enter() -> Result<(), String> {

    let vk_ctrl = VIRTUAL_KEY(0x11);
    let vk_return = VIRTUAL_KEY(0x0D);

    send_key_combo(&[vk_ctrl], vk_return)
}   // send_ctrl_enter()

/// Описание: Отправляет комбинацию Ctrl+V в текущий фокус.
///
/// # Ошибки
/// Возвращает `Err(String)`, если SendInput вернул 0 или отправил не все события.
pub fn send_ctrl_v() -> Result<(), String> {

    // let vk_ctrl = VIRTUAL_KEY(0x11);
    // let vk_v = VIRTUAL_KEY(0x56);
    // send_key_combo(&[vk_ctrl], vk_v)

    let vk_shift = VIRTUAL_KEY(0x10);
    let vk_insert = VIRTUAL_KEY(0x2D);
    send_key_combo(&[vk_shift], vk_insert)

}   // send_ctrl_v()

/// Описание: Отправляет комбинацию Ctrl+A в текущий фокус (выделить всё).
///
/// # Ошибки
/// Возвращает `Err(String)`, если SendInput вернул 0 или отправил не все события.
pub fn send_ctrl_a() -> Result<(), String> {

    let vk_ctrl = VIRTUAL_KEY(0x11);
    let vk_a = VIRTUAL_KEY(0x41);

    send_key_combo(&[vk_ctrl], vk_a)
}   // send_ctrl_a()

/// Описание: Отправляет комбинацию Ctrl+C в текущий фокус (копировать).
///
/// # Ошибки
/// Возвращает `Err(String)`, если SendInput вернул 0 или отправил не все события.
pub fn send_ctrl_c() -> Result<(), String> {

    let vk_ctrl = VIRTUAL_KEY(0x11);

    // let vk_c = VIRTUAL_KEY(0x43);
    // send_key_combo(&[vk_ctrl], vk_c)


    // С моей раскладкой сайт https://chat.deepseek.com воспринимает Ctrl+c как Ctrl+j (при этом,
    // Ctrl+v работает нормально). Поэтому, использую запасной вариант Ctrl+insert.
    // Он будет работать на всех раскладках.
    let vk_insert = VIRTUAL_KEY(0x2D);
    send_key_combo(&[vk_ctrl], vk_insert)
}   // send_ctrl_c()

/// Описание: Отправляет нажатие Escape (press) в текущий фокус.
///
/// Использует виртуальный код VK_ESCAPE (0x1B).
///
/// # Ошибки
/// Возвращает `Err(String)`, если SendInput не смог отправить события.
pub fn send_esc() -> Result<(), String> {

    let vk_esc = VIRTUAL_KEY(0x1B);
    send_key_press(vk_esc)
}   // send_esc()

/// Описание: Отправляет комбинацию Alt+F4 в текущий фокус.
///
/// Обычно закрывает текущее окно (best effort, зависит от приложения).
///
/// Виртуальные коды:
/// - VK_MENU (Alt) = 0x12
/// - VK_F4 = 0x73
///
/// # Ошибки
/// Возвращает `Err(String)`, если SendInput вернул 0 или отправил не все события.
pub fn send_alt_f4() -> Result<(), String> {
    let vk_alt = VIRTUAL_KEY(0x12); // VK_MENU
    let vk_f4 = VIRTUAL_KEY(0x73);  // VK_F4

    send_key_combo(&[vk_alt], vk_f4)
}   // send_alt_f4()

/// Описание: Отправляет нажатие Shift+Tab в текущий фокус.
///
/// Использует виртуальный код VK_ESCAPE (0x1B).
///
/// # Ошибки
/// Возвращает `Err(String)`, если SendInput не смог отправить события.
pub fn send_shift_tab() -> Result<(), String> {

    let vk_shift = VIRTUAL_KEY(0x10);
    let vk_tab = VIRTUAL_KEY(0x09);
    send_key_combo(&[vk_shift], vk_tab)
}   // send_esc()
