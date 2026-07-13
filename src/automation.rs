//! automation.rs
//!
//! Высокоуровневые функции UI Automation для работы с окнами приложений.
//!
//! # Ответственность
//! - Публичный API модуля автоматизации.
//! - Возврат геометрии UI-элементов в экранных координатах.
//! - Делегирование низкоуровневой работы в `automation_backend`.

pub mod automation_backend;

use std::thread::sleep;
use std::time::Duration;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, IsWindow, SM_CXVSCROLL, SM_CYHSCROLL};
use crate::automation::automation_backend::ComGuard;

/// Число повторных попыток поиска контейнера и файла.
const FIND_RETRY_COUNT: usize = 5;

/// Пауза между повторными попытками поиска.
const FIND_RETRY_DELAY_MS: u64 = 150;

/// Геометрия элемента Explorer в координатах виртуального экрана.
#[derive(Debug, Clone, Copy)]
pub struct ExplorerItemGeometry {
    pub rect: RECT,
    pub center: POINT,
}   // ExplorerItemGeometry

/// Найти файл в окне Explorer по имени и вернуть его геометрию.
///
/// # Описание
/// Функция ищет элемент файла в стандартном окне Windows Explorer через UI Automation.
/// При необходимости пытается прокрутить список к элементу.
///
/// # Алгоритм работы
/// - Проверяет валидность входных параметров.
/// - Инициализирует COM и `IUIAutomation`.
/// - При необходимости восстанавливает окно Explorer и переводит его в foreground.
/// - Несколько раз пытается:
///   - получить корневой `AutomationElement` окна;
///   - найти файловую область Explorer;
///   - найти элемент файла по имени;
///   - прокрутить список к файлу;
///   - получить `BoundingRectangle`.
///
/// # Параметры
/// - `explorer_hwnd`: Дескриптор окна Explorer.
/// - `file_name`: Точное имя файла.
///
/// # Возвращаемое значение
/// Тип: `Result<ExplorerItemGeometry, String>`. `ExplorerItemGeometry` возвращает найденный
/// прямоугольник и его центр в КООРДИНАТАХ ВИРТУАЛЬНОГО ЭКРАНА.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - `file_name` пустой;
/// - `HWND` невалиден;
/// - UI Automation не смогла инициализироваться;
/// - файловая область Explorer не найдена;
/// - файл не найден;
/// - у найденного элемента нет валидного прямоугольника.
/// - центр прямоугольника найденного элемента вне поля файлов.
pub fn find_file_rect_in_explorer(explorer_hwnd: HWND, file_name: &str)
                                          -> Result<ExplorerItemGeometry, String>
{
    // Пустое имя файла не имеет смысла: поиск по нему либо даст мусор, либо приведёт к неочевидным ошибкам.
    if file_name.trim().is_empty() {
        return Err("file_name is empty".to_string());
    }   // if

    // Проверяем, что дескриптор вообще указывает на существующее окно.
    // Это ещё не доказывает, что окно — именно Explorer, но отсекает совсем неверный HWND.
    unsafe {
        if !IsWindow(Some(explorer_hwnd)).as_bool() {
            return Err(format!("invalid explorer hwnd: {:?}", explorer_hwnd));
        }   // if
    }   // unsafe

    // Инициализируем COM для текущего потока.
    let _com = ComGuard::init_sta()?;

    // Создаём корневой объект UI Automation.
    let automation = automation_backend::_create_automation()?;

    // Приводим окно в пригодное для работы состояние:
    // восстанавливаем из minimized и, по возможности, выносим на передний план.
    automation_backend::_ensure_window_focused(explorer_hwnd)?;

    // Несколько раз повторяем попытку поиска.
    // Это нужно потому, что Explorer и UI Automation могут "догонять" друг друга с небольшой задержкой:
    // дерево элементов может появляться не мгновенно после restore/focus/scroll.
    for _attempt in 0..FIND_RETRY_COUNT {

        // Получаем AutomationElement, соответствующий окну Explorer.
        let root = automation_backend::_element_from_hwnd(&automation, explorer_hwnd)?;

        // Ищем контейнер, который, по нашей эвристике, является файловой областью окна.
        let files_container =
            automation_backend::_find_explorer_files_container(&automation, &root)?;

        // Ищем внутри контейнера элемент файла по имени.
        if let Some(item) =
            automation_backend::_find_file_item(&automation, &files_container, file_name)? {

            // Если у элемента поддерживается ScrollItemPattern, просим Explorer прокрутить его в видимую область.
            // Это особенно важно, если файл найден в дереве UIA, но сейчас вне viewport.
            automation_backend::_try_scroll_into_view(&item);

            // После прокрутки даём Explorer/UIA немного времени на обновление layout и bounding rectangle.
            sleep(Duration::from_millis(100));

            // Читаем экранный прямоугольник элемента.
            let rect = automation_backend::_get_current_bounding_rect(&item)?;

            // Вычисляем центр этого прямоугольника.
            let center = automation_backend::_rect_center(rect);

            // 2. Получаем прямоугольник контейнера (файловой области), чтобы убедиться, что файл видим.
            let mut container_rect = automation_backend::_get_current_bounding_rect(&files_container)?;

            // 3. Корректируем область контейнера, исключая полосы прокрутки.
            // Мы уменьшаем рабочую область справа (вертикальный скролл) и снизу (горизонтальный скролл).
            unsafe {
                container_rect.right -= GetSystemMetrics(SM_CXVSCROLL);
                container_rect.bottom -= GetSystemMetrics(SM_CYHSCROLL);
            }   // unsafe

            // 4. Проверяем, не выходит ли центр файла за границы контейнера.
            // Это защищает от ситуации, когда элемент в дереве есть, но он перекрыт другими панелями
            // или не полностью прокручен, и клик по его центру попадет в "чужой" UI.
            if automation_backend::_point_out_of_rect(container_rect, center) {
                return Err(format!(
                    "{}, {}: центр файла '{}' находится вне поля файлов. x:y={}:{},\
                     container_rect=[{}, {}, {}, {}]",
                    file!(), line!(), file_name, center.x, center.y,
                    container_rect.left, container_rect.top, container_rect.right, container_rect.bottom
                ));
            }   // if

            // Возвращаем готовую структуру.
            return Ok(ExplorerItemGeometry { rect, center });
        }   // if

        // На этой попытке файл не найден. Делаем короткую паузу и пробуем ещё раз.
        sleep(Duration::from_millis(FIND_RETRY_DELAY_MS));
    }   // for

    // После всех попыток файл так и не был найден.
    Err(format!("file '{}' not found in explorer window", file_name))
}   // find_file_rect_in_explorer()

// mod test {
//     use crate::library::automation::find_file_rect_in_explorer;
//     use crate::library::mouse::move_cursor_to_position;
//     use crate::library::window::find_window_by_needle_and_focus;
//     use crate::wrln;
// 
//     #[test]
//     fn smoke() {
//         let win = find_window_by_needle_and_focus("tech_spec");
//         let win = win.unwrap();
//         let hwnd = win.hwnd;
//         let geom = find_file_rect_in_explorer(hwnd, "user_guide.pdf");
//         wrln!(win, geom);
//         let center = geom.unwrap().center;
//         let _ = move_cursor_to_position(center.x, center.y);
//     }
// }