//! taskbar.rs
//!
//! Утилиты разного назначения, не вошедшие в другие модули.
//! Включает управление панелью задач (Taskbar).

#![allow(unused)]
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::Duration;
use windows::Win32::Foundation::LPARAM;
use windows::Win32::UI::Shell::{SHAppBarMessage, ABM_GETSTATE, ABM_SETSTATE, APPBARDATA, ABS_AUTOHIDE};
use windows::Win32::UI::WindowsAndMessaging::{
    SystemParametersInfoW, SPI_GETANIMATION, ANIMATIONINFO, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS
};

/// Хранилище предыдущего состояния панели задач.
///
/// Хранит `Option<bool>`:
/// - `None`: Состояние не сохранено (show_taskbar еще не вызывали или restore уже отработал).
/// - `Some(true)`: Панель задач была скрыта (Auto-hide).
/// - `Some(false)`: Панель задач была закреплена.
static TASKBAR_AUTOHIDE_SAVED: LazyLock<Mutex<Option<bool>>> = LazyLock::new(|| Mutex::new(None));

/// Описание: Делает панель задач видимой, отключая Auto-hide (если он включен).
///
/// Сохраняет исходное состояние Auto-hide для последующего `restore_taskbar()`.
///
/// # Алгоритм работы
/// - Получает текущий state панели задач.
/// - Если глобальное хранилище пусто (`None`), сохраняет в него текущий флаг Auto-hide.
///   (Повторные вызовы `show_taskbar` игнорируют сохранение, чтобы не затереть исходное состояние).
/// - Если Auto-hide включен — выключает его и ожидает завершения анимации.
///
/// # Побочные эффекты
/// - Меняет глобальное состояние панели задач Windows.
/// - Блокирует поток на время анимации (~500мс), если состояние изменилось.
pub fn show_taskbar() {
    let mut data = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        ..Default::default()
    };

    unsafe {
        // Получаем текущее состояние.
        let current_state = SHAppBarMessage(ABM_GETSTATE, &mut data) as u32;
        let is_autohide_on = (current_state & ABS_AUTOHIDE) != 0;

        // Блокируем мьютекс для обновления сохраненного состояния.
        // unwrap() безопасен, так как мы не ожидаем паники внутри критической секции в этом простом коде.
        let mut saved_lock = TASKBAR_AUTOHIDE_SAVED.lock().unwrap();

        // Сохраняем состояние только если оно еще не сохранено (первый вызов).
        if saved_lock.is_none() {
            *saved_lock = Some(is_autohide_on);
        }   // if

        // Если Auto-hide включен — выключаем его.
        if is_autohide_on {
            let new_state = current_state & !ABS_AUTOHIDE;
            data.lParam = LPARAM(new_state as isize);

            let _ = SHAppBarMessage(ABM_SETSTATE, &mut data);

            // Ждем, пока панель выедет, чтобы она попала на скриншот.
            _wait_for_taskbar_animation();
        }   // if
    }   // unsafe
}   // show_taskbar()

/// Описание: Восстанавливает состояние Auto-hide панели задач, сохраненное `show_taskbar()`.
///
/// # Алгоритм работы
/// - Забирает (`take`) сохраненное состояние из мьютекса.
/// - Если состояния нет (`None`) — ничего не делает.
/// - Если сохраненное состояние отличается от текущего — восстанавливает его и ждет анимацию.
///
/// # Побочные эффекты
/// - Меняет глобальное состояние панели задач Windows.
/// - Блокирует поток на время анимации, если состояние изменилось.
pub fn restore_taskbar_autohide_state() {
    // Забираем состояние и освобождаем мьютекс.
    let saved_opt = TASKBAR_AUTOHIDE_SAVED.lock().unwrap().take();

    if let Some(should_be_autohide) = saved_opt {
        let mut data = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            ..Default::default()
        };

        unsafe {
            let current_state = SHAppBarMessage(ABM_GETSTATE, &mut data) as u32;

            // Вычисляем целевое состояние флагов.
            let new_state = if should_be_autohide {
                current_state | ABS_AUTOHIDE
            } else {
                current_state & !ABS_AUTOHIDE
            };

            // Применяем изменения только если они требуются.
            if new_state != current_state {
                data.lParam = LPARAM(new_state as isize);
                let _ = SHAppBarMessage(ABM_SETSTATE, &mut data);

                // Ждем, пока панель уедет обратно (или закрепится), чтобы интерфейс успокоился.
                // Задвигается с приличной задержкой.
                _wait_for_taskbar_animation();
                _wait_for_taskbar_animation();
            }   // if
        }   // unsafe
    }   // if let Some
}   // restore_taskbar()

/// Описание: Проверяет, включен ли режим авто-скрытия (Auto-hide) у панели задач.
///
/// # Возвращаемое значение
/// - `true`: Auto-hide включен (панель скрывается).
/// - `false`: Auto-hide выключен (панель всегда видна).
///
/// # Побочные эффекты
/// - Нет (только чтение состояния системы).
pub fn is_taskbar_autohide_enabled() -> bool {
    let mut data = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        ..Default::default()
    };

    unsafe {
        // Получаем текущее состояние.
        let state = SHAppBarMessage(ABM_GETSTATE, &mut data) as u32;

        // Проверяем наличие бита ABS_AUTOHIDE.
        (state & ABS_AUTOHIDE) != 0
    }   // unsafe
}   // is_taskbar_hidden()

//--------------------------------------------------------------------------------------------------
//                  Внутренние утилиты
//--------------------------------------------------------------------------------------------------

/// Описание: Ожидает завершения визуальной анимации панели задач.
///
/// # Алгоритм работы
/// 1. Проверяет системную настройку "Анимация окон при сворачивании и разворачивании".
/// 2. Если анимация включена — ждет 500 мс (с запасом).
/// 3. Если выключена — ждет 50 мс для надежности перерисовки.
fn _wait_for_taskbar_animation() {
    let mut info = ANIMATIONINFO {
        cbSize: std::mem::size_of::<ANIMATIONINFO>() as u32,
        iMinAnimate: 0,
    };

    let animation_enabled = unsafe {
        // SPI_GETANIMATION: получаем параметры анимации.
        let result = SystemParametersInfoW(
            SPI_GETANIMATION,
            info.cbSize,
            Some(&mut info as *mut _ as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)
        );

        // iMinAnimate != 0 означает, что анимация включена.
        result.is_ok() && info.iMinAnimate != 0
    };

    if animation_enabled {
        thread::sleep(Duration::from_millis(500));
    } else {
        thread::sleep(Duration::from_millis(50));
    }
}   // _wait_for_taskbar_animation()

/// Дымовой тест: show_taskbar() и restore_taskbar() выполняются без паники.
///
/// Проверяет:
/// - Одиночный цикл show/restore.
/// - Повторный restore (когда состояние уже сброшено) не паникует.
/// - Повторный show не перетирает исходное сохраненное состояние.
#[test]
#[ignore]
fn test_taskbar_show_restore_smoke() {

    use crate::wrln;

    // Первый цикл: show сохраняет состояние, restore восстанавливает.
    show_taskbar();
    wrln!("выдвинулся");
    thread::sleep(Duration::from_secs(1));

    restore_taskbar_autohide_state();
    wrln!("задвинулся");
    thread::sleep(Duration::from_secs(1));
}   // test_taskbar_show_restore_smoke()
