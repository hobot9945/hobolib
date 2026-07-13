//! markdown_fence

/// Описание: Добавляет fenced code block в Markdown-строку с автоматически подобранным забором.
///
/// # Параметры
/// - `dst`: Строка-аккумулятор, куда дописывается Markdown.
/// - `text`: Полезная нагрузка для блока (если пусто — подставляется `(empty)`).
///
/// # Побочные эффекты
/// - Модифицирует `dst` (дописывает блок кода).
pub fn push_fenced_block(dst: &mut String, text: &str) {
    let payload = if text.is_empty() { "(empty)" } else { text };
    let fence = _calc_fence(payload);

    dst.push_str(&format!("{}\n", fence));
    dst.push_str(payload);
    if !payload.ends_with('\n') {
        dst.push('\n');
    }   // if
    dst.push_str(&format!("{}\n", fence));
}   // _push_fenced_block()

/// Описание: Оборачивает текст в fenced code block и возвращает результат в виде новой строки.
///
/// Функция автоматически подбирает длину "забора" (бэктиков), чтобы она была
/// на один символ длиннее самой длинной последовательности бэктиков внутри текста.
///
/// # Параметры
/// - `text`: Исходный текст для оборачивания.
///
/// # Возвращаемое значение
/// Тип: String: Текст, оформленный как Markdown-блок кода.
/// Если входной текст пустой, возвращается блок с текстом `(empty)`.
pub fn wrap_in_fence(text: &str) -> String {
    let payload = if text.is_empty() { "(empty)" } else { text };
    let fence = _calc_fence(payload);

    let mut result = String::with_capacity(payload.len() + 2*fence.len() + 3);

    result.push_str(&fence);
    result.push('\n');
    result.push_str(payload);

    // Гарантируем, что текст внутри блока заканчивается переводом строки перед закрывающим забором.
    if !payload.ends_with('\n') {
        result.push('\n');
    }   // if

    result.push_str(&fence);
    result.push('\n');

    result
}   // wrap_in_fence()

//--------------------------------------------------------------------------------------------------
//                  Внутренние утилиты
//--------------------------------------------------------------------------------------------------

/// Описание: Вычисляет длину “забора” для fenced code block (Adaptive Fences).
///
/// # Алгоритм работы
/// - Сканирует `text` и находит максимальную длину подряд идущих символов '`'.
/// - Возвращает строку из `max(3, N + 1)` обратных кавычек.
///
/// # Параметры
/// - `text`: Текст, который будет помещен внутрь fenced блока.
///
/// # Возвращаемое значение
/// Тип: String: Строка-забор из обратных кавычек.
fn _calc_fence(text: &str) -> String {
    let mut max_run: usize = 0;
    let mut cur_run: usize = 0;

    for ch in text.chars() {
        if ch == '`' {
            cur_run += 1;
            if cur_run > max_run {
                max_run = cur_run;
            }   // if
        } else {
            cur_run = 0;
        }   // if
    }   // for

    let fence_len = std::cmp::max(3, max_run + 1);
    "`".repeat(fence_len)
}   // _calc_fence()
