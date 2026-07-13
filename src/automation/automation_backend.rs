//! automation_backend.rs
//!
//! Низкоуровневая реализация UI Automation для поиска элементов в окне Explorer.
//!
//! # ОТВЕТСТВЕННОСТЬ
//! - Инициализация COM и `IUIAutomation`.
//! - Поиск файловой области окна Explorer.
//! - Поиск элемента файла по имени.
//! - Прокрутка к элементу через `ScrollItemPattern`.
//! - Возврат геометрии элемента в координатах виртуального рабочего стола.

use std::thread::sleep;
use std::time::Duration;
use windows::core::{BSTR, Interface, BOOL};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationCondition, IUIAutomationElement,
    IUIAutomationScrollItemPattern, TreeScope_Descendants, UIA_AutomationIdPropertyId,
    UIA_ControlTypePropertyId, UIA_DataItemControlTypeId, UIA_ListControlTypeId,
    UIA_ListItemControlTypeId, UIA_PaneControlTypeId, UIA_ScrollItemPatternId, UIA_PROPERTY_ID,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
};

/// RAII-обёртка для инициализации COM в текущем потоке.
///
/// Если `CoInitializeEx()` вернул успех, при уничтожении объекта будет вызван `CoUninitialize()`.
/// Если поток уже был инициализирован в другом COM-режиме (`RPC_E_CHANGED_MODE`), объект создаётся,
/// но `CoUninitialize()` затем не вызывается.
pub(super) struct ComGuard {
    should_uninit: bool,
}   // ComGuard

impl ComGuard {

    /// Инициализировать COM в STA-режиме.
    ///
    /// # Ошибки
    /// Возвращает `Err(String)`, если `CoInitializeEx()` завершился ошибкой,
    /// отличной от `RPC_E_CHANGED_MODE`.
    pub(super) fn init_sta() -> Result<Self, String> {
        unsafe {
            // Специальный HRESULT:
            // поток уже инициализирован для COM, но в другом режиме apartment.
            // Для нашей задачи это не фатально: просто не будем потом вызывать CoUninitialize().
            const RPC_E_CHANGED_MODE: i32 = -2147417850;

            // Пытаемся инициализировать COM в STA.
            // UI Automation традиционно лучше работает именно в таком контексте.
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

            // hr.is_ok() покрывает и S_OK, и S_FALSE.
            // В обоих случаях текущий вызов успешно вошёл в COM apartment,
            // значит при завершении нужен парный CoUninitialize().
            if hr.is_ok() {
                return Ok(Self { should_uninit: true });
            }   // if

            // Поток уже был инициализирован кем-то раньше в другом режиме.
            // Это не ideal-case, но на практике часто допустимо.
            if hr.0 == RPC_E_CHANGED_MODE {
                return Ok(Self { should_uninit: false });
            }   // if

            // Любая другая ошибка считаем фатальной для работы automation.
            Err(format!("CoInitializeEx() failed: {:?}", hr))
        }   // unsafe
    }   // init_sta()
}   // impl ComGuard

impl Drop for ComGuard {
    fn drop(&mut self) {
        // Вызываем CoUninitialize() только если именно этот объект ранее успешно инициализировал COM.
        if self.should_uninit {
            unsafe {
                CoUninitialize();
            }   // unsafe
        }   // if
    }   // drop()
}   // impl Drop for ComGuard

/// Создать экземпляр `IUIAutomation`.
///
/// # Ошибки
/// Возвращает `Err(String)`, если `CoCreateInstance(CUIAutomation)` завершился ошибкой.
pub(super) fn _create_automation() -> Result<IUIAutomation, String> {
    unsafe {
        // Создаём COM-объект UI Automation.
        CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| format!("CoCreateInstance(CUIAutomation) failed: {}", e))
    }   // unsafe
}   // create_automation()

/// Подготовить окно Explorer к поиску элементов.
///
/// # Алгоритм работы
/// - Если окно свернуто, восстанавливает его.
/// - Если окно не находится в foreground, пытается перевести его туда.
///
/// # Параметры
/// - `hwnd`: Дескриптор окна Explorer.
///
/// # Ошибки
/// Возвращает `Ok(())` всегда. Ошибки системных вызовов здесь не считаются фатальными.
///
/// # Побочные эффекты
/// - Может изменить состояние окна.
/// - Может изменить foreground-окно.
pub(super) fn _ensure_window_focused(hwnd: HWND) -> Result<(), String> {
    unsafe {
        // Если окно свернуто, сначала восстанавливаем его.
        // Иначе файловая область может быть недоступна или UIA-дерево будет неполным.
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);

            // Даём системе время реально развернуть окно.
            sleep(Duration::from_millis(200));
        }   // if

        // Если окно не находится в foreground, пытаемся его туда вывести.
        // Это не обязательно для чтения UIA, но повышает стабильность работы Explorer в интерактивном сценарии.
        if GetForegroundWindow() != hwnd {
            let ok: BOOL = SetForegroundWindow(hwnd);
            let _ = ok;

            // Даём системе время на фактическое переключение.
            sleep(Duration::from_millis(120));
        }   // if
    }   // unsafe

    Ok(())
}   // ensure_window_ready()

/// Получить `AutomationElement` по `HWND`.
///
/// # Параметры
/// - `automation`: Экземпляр `IUIAutomation`.
/// - `hwnd`: Дескриптор окна.
///
/// # Возвращаемое значение
/// Тип: `Result<IUIAutomationElement, String>`
///
/// # Ошибки
/// Возвращает `Err(String)`, если `ElementFromHandle()` завершился ошибкой.
pub(super) fn _element_from_hwnd(automation: &IUIAutomation, hwnd: HWND)
    -> Result<IUIAutomationElement, String>
{
    unsafe {
        // Преобразуем HWND окна Explorer в корневой UIA-элемент.
        automation
            .ElementFromHandle(hwnd)
            .map_err(|e| format!("ElementFromHandle() failed: {}", e))
    }   // unsafe
}   // element_from_hwnd()

/// Найти файловую область окна Explorer.
///
/// # Алгоритм работы
/// Использует следующую стратегию:
/// - сначала ищет элемент с `AutomationId = "ItemsView"`;
/// - затем ищет первый `List`;
/// - затем ищет первый `Pane`.
///
/// # Параметры
/// - `automation`: Экземпляр `IUIAutomation`.
/// - `root`: Корневой `AutomationElement` окна Explorer.
///
/// # Возвращаемое значение
/// Тип: `Result<IUIAutomationElement, String>`
///
/// # Ошибки
/// Возвращает `Err(String)`, если подходящий контейнер не найден.
pub(super) fn _find_explorer_files_container(automation: &IUIAutomation, root: &IUIAutomationElement)
    -> Result<IUIAutomationElement, String>
{
    // Самый предпочтительный вариант: стандартный контейнер списка файлов Explorer.
    if let Some(items_view) =
        _find_first_descendant_by_automation_id(automation, root, "ItemsView")?
    {
        return Ok(items_view);
    }   // if

    // Fallback №1: любой List внутри окна.
    // Это уже менее надёжно, потому что List может оказаться не тем контейнером, который нам нужен.
    if let Some(list_el) =
        _find_first_descendant_by_control_type(automation, root, UIA_ListControlTypeId.0)?
    {
        return Ok(list_el);
    }   // if

    // Fallback №2: любой Pane внутри окна.
    // Это ещё менее точно, но иногда Explorer рисует файловую область именно так.
    if let Some(pane_el) =
        _find_first_descendant_by_control_type(automation, root, UIA_PaneControlTypeId.0)?
    {
        return Ok(pane_el);
    }   // if

    // Ничего похожего на файловую область не найдено.
    Err("explorer files container not found".to_string())
}   // find_explorer_files_container()

/// Найти элемент файла по имени внутри файловой области Explorer.
///
/// # Алгоритм работы
/// - Сначала ищет среди `ListItem`.
/// - Затем ищет среди `DataItem`.
/// - Если не найдено, выполняет fallback-поиск по любому `descendant` внутри файловой области.
///
/// # Параметры
/// - `automation`: Экземпляр `IUIAutomation`.
/// - `files_container`: Контейнер файловой области Explorer.
/// - `file_name`: Точное имя файла.
///
/// # Возвращаемое значение
/// Тип: `Result<Option<IUIAutomationElement>, String>`
///
/// # Ошибки
/// Возвращает `Err(String)`, если вызовы UI Automation завершились ошибкой.
pub(super) fn _find_file_item(automation: &IUIAutomation, files_container: &IUIAutomationElement,
    file_name: &str)
    -> Result<Option<IUIAutomationElement>, String>
{
    // В большинстве режимов Explorer файл представлен как ListItem.
    if let Some(found) = _find_named_element_by_control_type(automation,
        files_container, file_name, UIA_ListItemControlTypeId.0)?
    {
        return Ok(Some(found));
    }   // if

    // В режиме таблицы/деталей файл может быть представлен как DataItem.
    if let Some(found) = _find_named_element_by_control_type(automation,
        files_container, file_name, UIA_DataItemControlTypeId.0)?
    {
        return Ok(Some(found));
    }   // if

    // Последний fallback:
    // ищем любой элемент с подходящим Name, но только внутри уже найденной файловой области.
    _find_named_descendant_any_type(automation, files_container, file_name)
}   // find_file_item()

/// Найти первый `descendant` по `AutomationId`.
///
/// # Параметры
/// - `automation`: Экземпляр `IUIAutomation`.
/// - `root`: Корневой элемент поиска.
/// - `automation_id`: Значение `AutomationId`.
///
/// # Возвращаемое значение
/// Тип: `Result<Option<IUIAutomationElement>, String>`
///
/// # Ошибки
/// Возвращает `Err(String)`, если не удалось создать условие поиска.
pub(super) fn _find_first_descendant_by_automation_id(automation: &IUIAutomation,
                                                      root: &IUIAutomationElement, automation_id: &str)
    -> Result<Option<IUIAutomationElement>, String>
{
    // Строим условие вида: AutomationId == "ItemsView".
    let cond =
        _create_string_property_condition(automation, UIA_AutomationIdPropertyId.0, automation_id)?;

    unsafe {
        // Ищем первый подходящий descendant.
        // Ошибки самого поиска здесь трактуем мягко: если не нашли/получили сбой FindFirst, возвращаем None.
        root.FindFirst(TreeScope_Descendants, &cond)
            .map(Some)
            .or_else(|_| Ok(None))
    }   // unsafe
}   // find_first_descendant_by_automation_id()

/// Найти первый `descendant` по типу элемента UI Automation.
///
/// # Параметры
/// - `automation`: Экземпляр `IUIAutomation`.
/// - `root`: Корневой элемент поиска.
/// - `control_type`: Значение `UIA_*ControlTypeId`.
///
/// # Возвращаемое значение
/// Тип: `Result<Option<IUIAutomationElement>, String>`
///
/// # Ошибки
/// Возвращает `Err(String)`, если не удалось создать условие поиска.
pub(super) fn _find_first_descendant_by_control_type(automation: &IUIAutomation, root: &IUIAutomationElement,
    control_type: i32)
    -> Result<Option<IUIAutomationElement>, String>
{
    // Строим условие вида: ControlType == UIA_ListControlTypeId / UIA_PaneControlTypeId / ...
    let cond =
        _create_i32_property_condition(automation, UIA_ControlTypePropertyId.0, control_type)?;

    unsafe {
        // Ищем первый подходящий descendant.
        root.FindFirst(TreeScope_Descendants, &cond)
            .map(Some)
            .or_else(|_| Ok(None))
    }   // unsafe
}   // find_first_descendant_by_control_type()

/// Найти элемент по имени среди `descendant` заданного типа.
///
/// # Алгоритм работы
/// - Строит условие по `ControlType`.
/// - Получает все подходящие `descendant`.
/// - Сравнивает имя каждого элемента с `file_name`.
///
/// # Параметры
/// - `automation`: Экземпляр `IUIAutomation`.
/// - `root`: Корневой элемент поиска.
/// - `file_name`: Точное имя файла.
/// - `control_type`: Значение `UIA_*ControlTypeId`.
///
/// # Возвращаемое значение
/// Тип: `Result<Option<IUIAutomationElement>, String>`
///
/// # Ошибки
/// Возвращает `Err(String)`, если вызовы UI Automation завершились ошибкой.
pub(super) fn _find_named_element_by_control_type(automation: &IUIAutomation, root: &IUIAutomationElement,
    file_name: &str, control_type: i32)
    -> Result<Option<IUIAutomationElement>, String>
{
    // Сначала ограничиваем поиск только нужным типом элементов.
    let type_cond =
        _create_i32_property_condition(automation, UIA_ControlTypePropertyId.0, control_type)?;

    unsafe {
        // Получаем массив всех descendant-элементов данного типа.
        let arr = root
            .FindAll(TreeScope_Descendants, &type_cond)
            .map_err(|e| format!("FindAll() by control type failed: {}", e))?;

        // Узнаём число найденных элементов.
        let length = arr
            .Length()
            .map_err(|e| format!("IUIAutomationElementArray::Length() failed: {}", e))?;

        // Последовательно перебираем всех кандидатов.
        for i in 0..length {
            let el = arr
                .GetElement(i)
                .map_err(|e| format!("GetElement({}) failed: {}", i, e))?;

            // Читаем имя текущего элемента.
            let name = _get_element_name(&el)?;

            // Сравниваем его с искомым именем файла.
            if _names_equal(&name, file_name) {
                return Ok(Some(el));
            }   // if
        }   // for
    }   // unsafe

    // Ни один элемент этого типа не подошёл.
    Ok(None)
}   // find_named_element_by_control_type()

/// Найти любой `descendant` по имени внутри файловой области Explorer.
///
/// # Алгоритм работы
/// - Получает все `descendant` внутри `root`.
/// - Сравнивает имя каждого элемента с `file_name`.
/// - Возвращает первый элемент с валидным `BoundingRectangle`.
///
/// # Параметры
/// - `automation`: Экземпляр `IUIAutomation`.
/// - `root`: Корневой элемент поиска.
/// - `file_name`: Точное имя файла.
///
/// # Возвращаемое значение
/// Тип: `Result<Option<IUIAutomationElement>, String>`
///
/// # Ошибки
/// Возвращает `Err(String)`, если вызовы UI Automation завершились ошибкой.
pub(super) fn _find_named_descendant_any_type(automation: &IUIAutomation, root: &IUIAutomationElement,
    file_name: &str)
    -> Result<Option<IUIAutomationElement>, String>
{
    let true_cond = unsafe {
        // CreateTrueCondition() даёт условие, пропускающее вообще любые элементы.
        // Это используется только как fallback, когда более узкие стратегии не сработали.
        automation
            .CreateTrueCondition()
            .map_err(|e| format!("CreateTrueCondition() failed: {}", e))?
    };

    unsafe {
        // Получаем вообще всех descendant-элементов внутри файлового контейнера.
        let arr = root
            .FindAll(TreeScope_Descendants, &true_cond)
            .map_err(|e| format!("FindAll() any-type failed: {}", e))?;

        let length = arr
            .Length()
            .map_err(|e| format!("IUIAutomationElementArray::Length() failed: {}", e))?;

        for i in 0..length {
            let el = arr
                .GetElement(i)
                .map_err(|e| format!("GetElement({}) failed: {}", i, e))?;

            // Читаем имя элемента.
            let name = _get_element_name(&el)?;

            // Если имя не совпало — это точно не наш файл.
            if !_names_equal(&name, file_name) {
                continue;
            }   // if

            // Для fallback-кандидата дополнительно проверяем, что у него есть валидный прямоугольник.
            // Это фильтрует часть мусорных элементов, которые технически имеют Name, но не имеют полезной геометрии.
            let rect = _get_current_bounding_rect(&el).unwrap_or_default();
            if _is_valid_rect(rect) {
                return Ok(Some(el));
            }   // if
        }   // for
    }   // unsafe

    // Подходящий fallback-элемент не найден.
    Ok(None)
}   // find_named_descendant_any_type()

/// Попытаться прокрутить файловую область к найденному элементу.
///
/// # Параметры
/// - `element`: Элемент файла.
///
/// # Побочные эффекты
/// - Может изменить положение прокрутки списка файлов.
pub(super) fn _try_scroll_into_view(element: &IUIAutomationElement) {
    unsafe {
        // Не каждый UIA-элемент поддерживает ScrollItemPattern.
        // Поэтому сначала пытаемся получить паттерн, а затем мягко игнорируем неудачу.
        if let Ok(pattern_obj) = element.GetCurrentPattern(UIA_ScrollItemPatternId) {

            // Приводим общий COM-объект паттерна к IUIAutomationScrollItemPattern.
            if let Ok(scroll_item) = pattern_obj.cast::<IUIAutomationScrollItemPattern>() {

                // Просим Explorer прокрутить элемент в видимую область.
                let _ = scroll_item.ScrollIntoView();
            }   // if
        }   // if
    }   // unsafe
}   // try_scroll_into_view()

/// Получить имя UI Automation-элемента.
///
/// # Параметры
/// - `element`: Элемент UI Automation.
///
/// # Возвращаемое значение
/// Тип: `Result<String, String>`
///
/// # Ошибки
/// Возвращает `Err(String)`, если `CurrentName()` завершился ошибкой.
pub(super) fn _get_element_name(element: &IUIAutomationElement) -> Result<String, String> {
    unsafe {
        // Имя элемента UIA приходит как BSTR.
        let name: BSTR = element
            .CurrentName()
            .map_err(|e| format!("CurrentName() failed: {}", e))?;

        // Преобразуем BSTR в обычную Rust-строку.
        Ok(name.to_string())
    }   // unsafe
}   // get_element_name()

/// Получить текущий `BoundingRectangle` элемента.
///
/// # Параметры
/// - `element`: Элемент UI Automation.
///
/// # Возвращаемое значение
/// Тип: `Result<RECT, String>`
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - `CurrentBoundingRectangle()` завершился ошибкой;
/// - прямоугольник невалиден.
pub(super) fn _get_current_bounding_rect(element: &IUIAutomationElement) -> Result<RECT, String> {
    unsafe {
        // Читаем экранный прямоугольник элемента.
        let rect = element
            .CurrentBoundingRectangle()
            .map_err(|e| format!("CurrentBoundingRectangle() failed: {}", e))?;

        // UIA иногда может вернуть пустой/бессмысленный прямоугольник.
        if !_is_valid_rect(rect) {
            return Err("element has invalid bounding rectangle".to_string());
        }   // if

        Ok(rect)
    }   // unsafe
}   // get_current_bounding_rect()

/// Проверить валидность прямоугольника.
///
/// # Параметры
/// - `rect`: Проверяемый прямоугольник.
///
/// # Возвращаемое значение
/// Тип: `bool`
pub(super) fn _is_valid_rect(rect: RECT) -> bool {
    // Считаем прямоугольник валидным, только если его ширина и высота строго положительны.
    rect.right > rect.left && rect.bottom > rect.top
}   // is_valid_rect()

/// Вычислить центр прямоугольника.
///
/// # Параметры
/// - `rect`: Исходный прямоугольник.
///
/// # Возвращаемое значение
/// Тип: `POINT`
pub(super) fn _rect_center(rect: RECT) -> POINT {
    // Вычисляем центр простым делением ширины и высоты пополам.
    POINT {
        x: rect.left + (rect.right - rect.left) / 2,
        y: rect.top + (rect.bottom - rect.top) / 2,
    }
}   // rect_center()

/// Проверить, находится ли точка вне заданного прямоугольника.
///
/// # Алгоритм работы
/// Точка считается находящейся внутри (результат `false`), если одновременно выполняются условия:
/// - x >= rect.left и x < rect.right
/// - y >= rect.top  и y < rect.bottom
/// Во всех остальных случаях функция возвращает `true`.
///
/// # Параметры
/// - `rect`: Прямоугольник (Win32 `RECT`).
/// - `point`: Точка для проверки (Win32 `POINT`).
///
/// # Возвращаемое значение
/// Тип: `bool`: `true`, если точка находится за пределами прямоугольника.
pub(super) fn _point_out_of_rect(rect: RECT, point: POINT) -> bool {
    // Проверяем выход за каждую границу по отдельности.
    // Если точка левее левой границы, правее или на уровне правой,
    // выше верхней или ниже или на уровне нижней — она снаружи.
    point.x < rect.left ||
        point.x >= rect.right ||
        point.y < rect.top ||
        point.y >= rect.bottom
}   // _point_out_of_rect()

/// Сравнить имена файлов.
///
/// # Алгоритм работы
/// - Сначала выполняет точное сравнение.
/// - Затем выполняет сравнение без учёта регистра.
///
/// # Параметры
/// - `left`: Первое имя.
/// - `right`: Второе имя.
///
/// # Возвращаемое значение
/// Тип: `bool`
pub(super) fn _names_equal(left: &str, right: &str) -> bool {
    // Быстрый путь: строки совпали побайтно.
    if left == right {
        return true;
    }   // if

    // Fallback: сравнение без учёта регистра.
    // Используем Unicode-aware lowercase, чтобы не ограничиваться только ASCII.
    left.to_lowercase() == right.to_lowercase()
}   // names_equal()

/// Создать условие UI Automation для строкового свойства.
///
/// # Параметры
/// - `automation`: Экземпляр `IUIAutomation`.
/// - `property_id`: Идентификатор свойства.
/// - `value`: Строковое значение свойства.
///
/// # Возвращаемое значение
/// Тип: `Result<IUIAutomationCondition, String>`
///
/// # Ошибки
/// Возвращает `Err(String)`, если `CreatePropertyCondition()` завершился ошибкой.
pub(super) fn _create_string_property_condition(automation: &IUIAutomation, property_id: i32,
    value: &str)
    -> Result<IUIAutomationCondition, String>
{
    unsafe {
        // Упаковываем строку в VARIANT, как того требует COM-интерфейс UI Automation.
        let variant = VARIANT::from(value);

        automation
            .CreatePropertyCondition(UIA_PROPERTY_ID(property_id), &variant)
            .map_err(|e| format!("CreatePropertyCondition(string) failed: {}", e))
    }   // unsafe
}   // create_string_property_condition()

/// Создать условие UI Automation для целочисленного свойства.
///
/// # Параметры
/// - `automation`: Экземпляр `IUIAutomation`.
/// - `property_id`: Идентификатор свойства.
/// - `value`: Целочисленное значение свойства.
///
/// # Возвращаемое значение
/// Тип: `Result<IUIAutomationCondition, String>`
///
/// # Ошибки
/// Возвращает `Err(String)`, если `CreatePropertyCondition()` завершился ошибкой.
pub(super) fn _create_i32_property_condition(automation: &IUIAutomation, property_id: i32, value: i32)
    -> Result<IUIAutomationCondition, String>
{
    unsafe {
        // Упаковываем целое число в VARIANT.
        let variant = VARIANT::from(value);

        automation
            .CreatePropertyCondition(UIA_PROPERTY_ID(property_id), &variant)
            .map_err(|e| format!("CreatePropertyCondition(i32) failed: {}", e))
    }   // unsafe
}   // create_i32_property_condition()