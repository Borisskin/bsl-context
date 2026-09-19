//! Парсер страницы значения системного перечисления (`/properties/...html`
//! у родителя-перечисления).
//!
//! Простейший парсер: берёт имя страницы (русское/английское) и блоки
//! «Описание:» и «См. также:». Игнорирует «Доступность:», «Использование в
//! версии:», «Использование в интерфейсе:».
//!
//! Порт `EnumValuePageParser.kt` (alkoleft).

use crate::blocks::{parse_description, parse_head_name, parse_related_objects};
use crate::html::split_chapters;
use crate::models::EnumValueInfo;

pub fn parse_enum_value_page(html: &str) -> EnumValueInfo {
    let chapters = split_chapters(html);

    // Голова страницы — нулевая глава (до любого V8SH_chapter/<hr>).
    let head_html = chapters.first().map(|c| c.body_html.as_str()).unwrap_or("");
    let (name_ru, name_en) = parse_head_name(head_html);

    let mut description = String::new();
    let mut related = Vec::new();

    for ch in chapters.iter().skip(1) {
        match ch.title.as_str() {
            "Описание:" => description = parse_description(&ch.body_html),
            "См. также:" => related = parse_related_objects(&ch.body_html),
            // Эти главы есть в апстриме, но мы их игнорируем как и он.
            "Доступность:" | "Использование в версии:" | "Использование в интерфейсе:" =>
                {}
            _ => {}
        }
    }

    EnumValueInfo {
        name_ru,
        name_en,
        description,
        related_objects: related,
    }
}
