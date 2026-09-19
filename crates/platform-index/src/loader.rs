//! Pipeline `HbkContent → PlatformIndex`.
//!
//! Один проход по TOC платформы:
//! 1. Найти `Global context` → собрать `global_methods` и `global_properties`.
//! 2. Найти каталоги перечислений → распарсить как типы с `enum_values`.
//! 3. Найти каталоги типов → распарсить с `methods/properties/constructors`.
//!
//! Все типы (обычные и перечисления) складываются в одну `HashMap` по `name_ru`.

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use hbk_reader::HbkContent;
use tracing::{info, warn};

use crate::mapper::{method_from, property_from, type_from_enum, type_from_object};
use crate::storage::PlatformIndex;
use crate::visitor::{
    collect_global_methods, collect_global_properties, collect_root_pages, drill_down,
    visit_enum_page, visit_type_page,
};

/// Загрузить `PlatformIndex` из hbk-файла платформы.
///
/// Принимает путь к `shcntx_ru.hbk`. Не разделяет файлы — всё в один проход.
pub fn load_from_hbk(path: &Path) -> Result<PlatformIndex> {
    info!(?path, "загрузка платформенного контекста из hbk");
    let mut content = HbkContent::read(path)
        .map_err(|e| anyhow!("не удалось открыть hbk {}: {}", path.display(), e))?;
    build_index(&mut content).with_context(|| format!("сборка PlatformIndex из {}", path.display()))
}

/// Та же логика, но для уже открытого `HbkContent` (удобно в тестах).
pub fn build_index(content: &mut HbkContent) -> Result<PlatformIndex> {
    let mut index = PlatformIndex::new();

    // Снимаем заимствование TOC clone'ом дерева — нам нужно одновременно
    // итерировать по страницам и читать их html через `&mut HbkContent`.
    let pages = content.toc.pages.clone();
    let roots = collect_root_pages(&pages);

    if let Some(global) = roots.global_context {
        index.global_methods = collect_global_methods(content, global)
            .iter()
            .map(method_from)
            .collect();
        index.global_properties = collect_global_properties(content, global)
            .iter()
            .map(property_from)
            .collect();
    } else {
        warn!("раздел 'Global context' не найден в TOC — global_methods/global_properties пусты");
    }

    // Перечисления (типы с enum_values).
    let mut enum_pages = Vec::new();
    for root in &roots.enums {
        drill_down(root, &mut enum_pages);
    }
    for page in enum_pages {
        if let Some(info) = visit_enum_page(content, page) {
            index.insert_type(type_from_enum(&info));
        }
    }

    // Обычные типы.
    let mut type_pages = Vec::new();
    for root in &roots.types {
        drill_down(root, &mut type_pages);
    }
    for page in type_pages {
        if let Some(info) = visit_type_page(content, page) {
            index.insert_type(type_from_object(&info));
        }
    }

    info!(
        global_methods = index.global_methods.len(),
        global_properties = index.global_properties.len(),
        types = index.types.len(),
        enum_types = index.enum_types_count(),
        "PlatformIndex собран"
    );
    Ok(index)
}
