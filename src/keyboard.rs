//! keyboard.rs

use windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY;
use crate::keyboard::keyboard_backend::{send_key_combo, send_key_press};

mod keyboard_backend;

/// Description: Presses and releases a key by its VK-code (press).
///
/// # Parameters
/// - `vk`: virtual key code (VK_*), e.g.:
///   - Ctrl = 0x11
///   - Enter = 0x0D
///
/// # Errors
/// Returns `Err(String)` if SendInput fails to send the events.
pub fn send_vk_press(vk: u16) -> Result<(), String> {
    send_key_press(VIRTUAL_KEY(vk))
}   // send_vk_press()

/// Description: Sends a key combination of "modifiers + main key" using VK-codes.
///
/// Examples:
/// - modifiers=[0x11], key=0x56  -> Ctrl+V
/// - modifiers=[0x11,0x10], key=0x53 -> Ctrl+Shift+S
///
/// # Parameters
/// - `modifiers`: VK-codes of modifiers (Ctrl/Shift/Alt/Win, etc.)
/// - `key`: VK-code of the main key
///
/// # Errors
/// Returns `Err(String)` if SendInput fails to send the events.
pub fn send_vk_combo(modifiers: &[u16], key: u16) -> Result<(), String> {

    // Convert u16 -> VIRTUAL_KEY. A Vec is needed because send_key_combo takes a slice.
    let mods: Vec<VIRTUAL_KEY> = modifiers.iter().map(|&v| VIRTUAL_KEY(v)).collect();

    send_key_combo(&mods, VIRTUAL_KEY(key))

}   // send_vk_combo()

/// Description: Sends an Enter key press to the current focus.
///
/// Uses the virtual key code VK_RETURN (0x0D), which is sufficient for standard windows
/// with text input.
///
/// # Errors
/// Returns `Err(String)` if SendInput fails to send the events.
pub fn send_enter() -> Result<(), String> {

    let vk_return = VIRTUAL_KEY(0x0D);
    send_key_press(vk_return)
}   // send_enter()

/// Description: Sends a Ctrl+Enter combination to the current focus.
///
/// Used to force send messages in web forms, where a regular Enter might act as a line break.
///
/// # Errors
/// Returns `Err(String)` if SendInput returned 0 or did not send all events.
pub fn send_ctrl_enter() -> Result<(), String> {

    let vk_ctrl = VIRTUAL_KEY(0x11);
    let vk_return = VIRTUAL_KEY(0x0D);

    send_key_combo(&[vk_ctrl], vk_return)
}   // send_ctrl_enter()

/// Description: Sends a Ctrl+V combination to the current focus.
///
/// # Errors
/// Returns `Err(String)` if SendInput returned 0 or did not send all events.
pub fn send_ctrl_v() -> Result<(), String> {

    // let vk_ctrl = VIRTUAL_KEY(0x11);
    // let vk_v = VIRTUAL_KEY(0x56);
    // send_key_combo(&[vk_ctrl], vk_v)

    let vk_shift = VIRTUAL_KEY(0x10);
    let vk_insert = VIRTUAL_KEY(0x2D);
    send_key_combo(&[vk_shift], vk_insert)

}   // send_ctrl_v()

/// Description: Sends a Ctrl+A combination to the current focus (select all).
///
/// # Errors
/// Returns `Err(String)` if SendInput returned 0 or did not send all events.
pub fn send_ctrl_a() -> Result<(), String> {

    let vk_ctrl = VIRTUAL_KEY(0x11);
    let vk_a = VIRTUAL_KEY(0x41);

    send_key_combo(&[vk_ctrl], vk_a)
}   // send_ctrl_a()

/// Description: Sends a Ctrl+C combination to the current focus (copy).
///
/// # Errors
/// Returns `Err(String)` if SendInput returned 0 or did not send all events.
pub fn send_ctrl_c() -> Result<(), String> {

    let vk_ctrl = VIRTUAL_KEY(0x11);

    // let vk_c = VIRTUAL_KEY(0x43);
    // send_key_combo(&[vk_ctrl], vk_c)


    // With my keyboard layout, the site https://chat.deepseek.com perceives Ctrl+C as Ctrl+J (meanwhile,
    // Ctrl+V works fine). Therefore, I use the fallback option Ctrl+Insert.
    // It will work on all layouts.
    let vk_insert = VIRTUAL_KEY(0x2D);
    send_key_combo(&[vk_ctrl], vk_insert)
}   // send_ctrl_c()

/// Description: Sends an Escape key press to the current focus.
///
/// Uses the virtual key code VK_ESCAPE (0x1B).
///
/// # Errors
/// Returns `Err(String)` if SendInput fails to send the events.
pub fn send_esc() -> Result<(), String> {

    let vk_esc = VIRTUAL_KEY(0x1B);
    send_key_press(vk_esc)
}   // send_esc()

/// Description: Sends an Alt+F4 combination to the current focus.
///
/// Usually closes the current window (the best effort, depends on the application).
///
/// Virtual key codes:
/// - VK_MENU (Alt) = 0x12
/// - VK_F4 = 0x73
///
/// # Errors
/// Returns `Err(String)` if SendInput returned 0 or did not send all events.
pub fn send_alt_f4() -> Result<(), String> {
    let vk_alt = VIRTUAL_KEY(0x12); // VK_MENU
    let vk_f4 = VIRTUAL_KEY(0x73);  // VK_F4

    send_key_combo(&[vk_alt], vk_f4)
}   // send_alt_f4()

/// Description: Sends a Shift+Tab combination to the current focus.
///
/// Uses virtual key codes VK_SHIFT (0x10) and VK_TAB (0x09).
///
/// # Errors
/// Returns `Err(String)` if SendInput fails to send the events.
pub fn send_shift_tab() -> Result<(), String> {

    let vk_shift = VIRTUAL_KEY(0x10);
    let vk_tab = VIRTUAL_KEY(0x09);
    send_key_combo(&[vk_shift], vk_tab)
}   // send_shift_tab()

/// Description: Sends a Backspace key press to the currently focused window.
///
/// Uses the virtual key code `VK_BACK` (0x08).
///
/// # Errors
/// Returns `Err(String)` if `SendInput` fails to send the events.
pub fn send_backspace() -> Result<(), String> {

    let vk_back = VIRTUAL_KEY(0x08);
    send_key_press(vk_back)
}   // send_backspace()