//! Парсер страницы свойства типа платформы.
//!
//! Главы: «Описание:» (текст с типом), «Использование:» (флаг readonly),
//! «См. также:», «Примечание:».
//!
//! Порт `PropertyPageParser.kt`. У апстрима блок «Описание:» парсится через
//! `ValueInfoBlockHandler` — вытаскивает type_name + description.

use crate::blocks::{
    parse_head_name, parse_note, parse_readonly, parse_related_objects, parse_value_info,
};
use crate::html::split_chapters;
use crate::models::PropertyInfo;

pub fn parse_property_page(html: &str) -> PropertyInfo {
    let chapters = split_chapters(html);

    let head_html = chapters.first().map(|c| c.body_html.as_str()).unwrap_or("");
    let (name_ru, name_en) = parse_head_name(head_html);

    let mut description = String::new();
    let mut type_name = String::new();
    let mut readonly = false;
    let mut note: Option<String> = None;
    let mut related = Vec::new();

    for ch in chapters.iter().skip(1) {
        match ch.title.as_str() {
            "Описание:" => {
                if let Some(info) = parse_value_info(&ch.body_html) {
                    type_name = info.type_name;
                    description = info.description;
                }
            }
            "Использование:" => readonly = parse_readonly(&ch.body_html),
            "См. также:" => related = parse_related_objects(&ch.body_html),
            "Примечание:" => note = Some(parse_note(&ch.body_html)),
            "Доступность:" | "Использование в версии:" | "Использование в интерфейсе:" =>
                {}
            _ => {}
        }
    }

    PropertyInfo {
        name_ru,
        name_en,
        description,
        type_name,
        readonly,
        note,
        related_objects: related,
    }
}
