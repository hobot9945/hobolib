//! opencv_backend.rs

#![allow(unused)]
use ::xcap::image::RgbaImage;
use opencv::core::{CV_8UC4, Mat, Rect, Point};
use opencv::core::AlgorithmHint::ALGO_HINT_DEFAULT;
use opencv::prelude::*;

/// Лучший результат сопоставления шаблона.
#[derive(Debug, Clone)]
pub struct TemplateMatchHit {
    pub score: f64,      // Оценка совпадения.
    pub top_left: Point, // Координаты левого верхнего угла найденной области.
    pub rect: Rect,      // Прямоугольник найденной области в координатах исходного изображения.
    pub center: Point,   // Центр найденной области.
}   // TemplateMatchHit

/// Преобразует `RgbaImage` в `opencv::core::Mat` формата `CV_8UC4`.
///
/// Важно:
/// - каналы сохраняются в порядке **RGBA** как в исходном `RgbaImage`;
/// - функция **не** переставляет каналы в BGRA/BGR;
/// - полученный `Mat` владеет собственной копией данных.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - размеры изображения не помещаются в `i32`;
/// - OpenCV не смог создать или заполнить `Mat`.
pub(super) fn _rgba_image_to_mat(image: &RgbaImage) -> Result<Mat, String> {

    // Безопасное приведение типов
    let width = i32::try_from(image.width())
        .map_err(|_| format!("Ширина изображения слишком велика: {}", image.width()))?;
    let height = i32::try_from(image.height())
        .map_err(|_| format!("Высота изображения слишком велика: {}", image.height()))?;

    // Получить ссылку на сырой буфер исходного изображения.
    let src = image.as_raw();

    // Создать пустой Mat, содержащий буфер нужного размера.
    let mut mat = Mat::new_rows_cols_with_default(
        height,
        width,
        CV_8UC4,
        opencv::core::Scalar::all(0.0),
    ).map_err(|e| format!("Не удалось создать Mat CV_8UC4: {}", e))?;

    // Взять мутабельную ссылку на буфер в Mat.
    let dst = mat
        .data_bytes_mut()
        .map_err(|e| format!("Не удалось получить доступ к буферу Mat: {}", e))?;

    // Проверить совпадение размеров источника и цели.
    if dst.len() != src.len() {
        return Err(format!(
            "Несовпадение размеров буферов при конвертации RgbaImage -> Mat: src={}, dst={}",
            src.len(),
            dst.len()
        ));
    }

    // Копировать.
    dst.copy_from_slice(src);

    Ok(mat)
}   // _rgba_image_to_mat()

/// Преобразует `Mat` формата RGBA (`CV_8UC4`) в оттенки серого (`CV_8UC1`).
///
/// # Алгоритм работы
/// 1. Проверяет, что входная матрица имеет тип `CV_8UC4`.
/// 2. Вызывает `cvt_color` с кодом `COLOR_RGBA2GRAY`.
/// 3. Возвращает новый `Mat`, содержащий одноканальное изображение.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - входная матрица имеет неожиданный тип (не `CV_8UC4`);
/// - OpenCV не смог выполнить преобразование цвета.
///
/// # Побочные эффекты
/// - Исходный `Mat` не модифицируется.
pub(super) fn _rgba_mat_to_grayscale(src: &Mat) -> Result<Mat, String> {

    // Проверить, что входная матрица имеет ожидаемый тип.
    let src_type = src.typ();
    if src_type != CV_8UC4 {
        return Err(format!(
            "Ожидался Mat типа CV_8UC4 ({}), получен тип: {}",
            CV_8UC4, src_type
        ));
    }   // if

    // Создать целевую матрицу.
    let mut gray = Mat::default();

    // Преобразовать RGBA -> Grayscale.
    opencv::imgproc::cvt_color(src, &mut gray, opencv::imgproc::COLOR_RGBA2GRAY, 0, ALGO_HINT_DEFAULT)
        .map_err(|e| format!("Ошибка преобразования RGBA -> GRAY: {}", e))?;

    Ok(gray)
}   // rgba_mat_to_grayscale()

/// Выполняет поиск шаблона `needle` в изображении `haystack` через OpenCV `match_template`.
///
/// Ожидается, что оба изображения уже подготовлены для сопоставления:
/// - имеют одинаковый тип;
/// - искомый шаблон по размерам не превосходит исходное изображение;
///
/// # Алгоритм работы
/// - Проверяет, что входные матрицы не пусты.
/// - Проверяет совпадение типов `haystack` и `needle`.
/// - Проверяет, что тип изображений равен `CV_8UC1`.
/// - Проверяет, что шаблон не больше исходного изображения.
/// - Вызывает `match_template` с переданным методом сравнения.
/// - Возвращает `Mat`, содержащий карту score-ов совпадения.
///
/// # Параметры
/// - `haystack`: Исходное изображение, в котором выполняется поиск.
/// - `needle`: Шаблон для поиска.
/// - `method`: Алгоритм сравнения OpenCV (`TM_CCOEFF_NORMED`, `TM_SQDIFF`, ...).
///
/// # Возвращаемое значение
/// Тип: `Mat`: Матрица результатов сопоставления шаблона.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - одна из матриц пуста;
/// - типы входных матриц не совпадают;
/// - тип матриц отличается от ожидаемого `CV_8UC1`;
/// - шаблон больше исходного изображения;
/// - OpenCV не смог выполнить сопоставление.
///
/// # Побочные эффекты
/// - Исходные `Mat` не модифицируются.
pub(super) fn _run_match_template(haystack: &Mat, needle: &Mat, method: i32) -> Result<Mat, String> {

    // Проверить, что исходное изображение не пусто.
    if haystack.empty() {
        return Err("Исходное изображение haystack пусто.".to_string());
    }   // if

    // Проверить, что шаблон не пуст.
    if needle.empty() {
        return Err("Шаблон needle пуст.".to_string());
    }   // if

    // Проверить совпадение типов изображений.
    let haystack_type = haystack.typ();
    let needle_type = needle.typ();

    if haystack_type != needle_type {
        return Err(format!(
            "Типы изображений не совпадают: haystack={}, needle={}",
            haystack_type, needle_type
        ));
    }   // if

    // Проверить, что шаблон помещается в исходное изображение.
    let haystack_size = haystack
        .size()
        .map_err(|e| format!("Не удалось получить размер haystack: {}", e))?;
    let needle_size = needle
        .size()
        .map_err(|e| format!("Не удалось получить размер needle: {}", e))?;

    if needle_size.width > haystack_size.width || needle_size.height > haystack_size.height {
        return Err(format!(
            "Шаблон больше исходного изображения: haystack={}x{}, needle={}x{}",
            haystack_size.width,
            haystack_size.height,
            needle_size.width,
            needle_size.height
        ));
    }   // if

    // Создать матрицу результата.
    let mut result = Mat::default();

    // Выполнить поиск шаблона.
    opencv::imgproc::match_template(
        haystack,
        needle,
        &mut result,
        method,
        &Mat::default(),
    )
        .map_err(|e| format!("Ошибка OpenCV при выполнении match_template: {}", e))?;

    Ok(result)
}   // _run_match_template()

/// Извлекает лучший результат сопоставления шаблона из матрицы score-ов.
///
/// # Алгоритм работы
/// - Проверяет, что `result_map` не пуста.
/// - Получает размеры шаблона.
/// - Вызывает `min_max_loc` для поиска экстремумов.
/// - В зависимости от `method` выбирает:
///   - `max_loc` и `max_val` для корреляционных методов;
///   - `min_loc` и `min_val` для методов `TM_SQDIFF*`.
/// - Строит прямоугольник найденной области и вычисляет его центр.
///
/// # Параметры
/// - `result_map`: Матрица результатов, возвращенная `match_template`.
/// - `needle`: Шаблон, использованный при сопоставлении. Нужен для вычисления размеров найденной области.
/// - `method`: Алгоритм сравнения OpenCV (`TM_CCOEFF_NORMED`, `TM_SQDIFF`, ...).
///
/// # Возвращаемое значение
/// Тип: `TemplateMatchHit`: Лучший результат сопоставления.
///
/// # Ошибки
/// Возвращает `Err(String)`, если:
/// - `result_map` пуста;
/// - `needle` пуст;
/// - не удалось получить размеры шаблона;
/// - OpenCV не смог выполнить `min_max_loc`.
///
/// # Побочные эффекты
/// - Отсутствуют.
pub(super) fn _extract_best_match(result_map: &Mat, needle: &Mat, method: i32)
    -> Result<TemplateMatchHit, String>
{
    // Проверить, что карта результатов не пуста.
    if result_map.empty() {
        return Err("Матрица result_map пуста.".to_string());
    }   // if

    // Проверить, что шаблон не пуст.
    if needle.empty() {
        return Err("Матрица needle пуста.".to_string());
    }   // if

    // Получить размеры шаблона.
    let needle_size = needle
        .size()
        .map_err(|e| format!("Не удалось получить размер needle: {}", e))?;

    // Получить минимум, максимум и их координаты.
    let mut min_val = 0.0_f64;
    let mut min_loc = Point::default();
    let mut max_val = 0.0_f64;
    let mut max_loc = Point::default();
    opencv::core::min_max_loc(result_map, Some(&mut min_val), Some(&mut max_val), Some(&mut min_loc),
        Some(&mut max_loc), &Mat::default())
        .map_err(|e| format!("Ошибка OpenCV при выполнении min_max_loc: {}", e))?;

    // Выбрать лучшую точку и score в зависимости от метода.
    let (top_left, score) = match method {
        opencv::imgproc::TM_SQDIFF | opencv::imgproc::TM_SQDIFF_NORMED => {
            (min_loc, min_val)
        },
        _ => {
            (max_loc, max_val)
        }
    };  // match method

    // Построить прямоугольник найденной области.
    let rect = Rect::new(
        top_left.x,
        top_left.y,
        needle_size.width,
        needle_size.height,
    );

    // Вычислить центр найденной области.
    let center = Point::new(
        top_left.x + needle_size.width / 2,
        top_left.y + needle_size.height / 2,
    );

    Ok(TemplateMatchHit {
        rect,
        center,
        score,
        top_left,
    })
}   // _extract_best_match()
