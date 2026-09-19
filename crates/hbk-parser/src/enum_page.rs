//! Парсер страницы системного перечисления (тип-перечисление, например
//! `ТипРазмещенияТекстаТабличногоДокумента`).
//!
//! Сама страница содержит описание, пример, ссылки «См. также» и **не
//! содержит значения** — значения лежат как дочерние страницы TOC в
//! `/properties/...`. Их собирает PagesVisitor из crate-уровня (см.
//! `PagesVisitor::visitEnumPage` в апстриме). Здесь парсим только саму
//! страницу-родитель.
//!
//! Порт `EnumPageParser.kt` (alkoleft).

use crate::blocks::{parse_description, parse_example, parse_head_name, parse_related_objects};
use crate::html::split_chapters;
use crate::models::EnumInfo;

pub fn parse_enum_page(html: &str) -> EnumInfo {
    let chapters = split_chapters(html);

    let head_html = chapters.first().map(|c| c.body_html.as_str()).unwrap_or("");
    let (name_ru, name_en) = parse_head_name(head_html);

    let mut description = String::new();
    let mut example: Option<String> = None;
    let mut related = Vec::new();

    for ch in chapters.iter().skip(1) {
        match ch.title.as_str() {
            "Описание:" => description = parse_description(&ch.body_html),
            "Пример:" => example = Some(parse_example(&ch.body_html)),
            "См. также:" => related = parse_related_objects(&ch.body_html),
            // Игнорируемые главы (как в апстриме):
            "Значения"
            | "Свойства:"
            | "Доступность:"
            | "Использование в версии:"
            | "Использование в интерфейсе:" => {}
            _ => {}
        }
    }

    EnumInfo {
        name_ru,
        name_en,
        description,
        example,
        related_objects: related,
        // values заполняется через PagesVisitor отдельным проходом по детям TOC.
        values: Vec::new(),
    }
}
