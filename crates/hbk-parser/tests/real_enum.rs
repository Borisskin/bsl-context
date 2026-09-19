//! Integration-тест на реальном `shcntx_ru.hbk`: парсим страницу системного
//! перечисления и собираем её значения из дочерних страниц TOC.
//!
//! Запуск:
//! ```pwsh
//! $env:BSL_CONTEXT_PLATFORM_PATH = 'C:\Program Files\1cv8\8.3.27.1786'
//! cargo test -p hbk-parser --test real_enum -- --nocapture
//! ```

use std::path::PathBuf;

use hbk_parser::{parse_enum_page, parse_enum_value_page};
use hbk_reader::{HbkContent, Page};

fn hbk_path() -> Option<PathBuf> {
    let root = std::env::var("BSL_CONTEXT_PLATFORM_PATH")
        .ok()
        .map(PathBuf::from)?;
    let candidates = [
        root.join("shcntx_ru.hbk"),
        root.join("bin").join("shcntx_ru.hbk"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

/// Собрать html_path страницы (по точному русскому названию) и html_path всех
/// её дочерних страниц-значений (`/properties/`). Делается за один обход
/// дерева, чтобы потом использовать &mut HbkContent без conflicts с borrow.
fn collect_enum_paths(pages: &[Page], target_ru: &str) -> Option<EnumPaths> {
    fn walk<'a>(pages: &'a [Page], target: &str) -> Option<&'a Page> {
        for p in pages {
            if p.title.ru == target {
                return Some(p);
            }
            if let Some(found) = walk(&p.children, target) {
                return Some(found);
            }
        }
        None
    }

    let page = walk(pages, target_ru)?;
    let value_paths: Vec<String> = page
        .children
        .iter()
        .filter(|c| c.html_path.contains("/properties/"))
        .map(|c| c.html_path.clone())
        .collect();
    Some(EnumPaths {
        html_path: page.html_path.clone(),
        title_ru: page.title.ru.clone(),
        title_en: page.title.en.clone(),
        value_paths,
    })
}

struct EnumPaths {
    html_path: String,
    title_ru: String,
    #[allow(dead_code)]
    title_en: String,
    value_paths: Vec<String>,
}

/// Найти любое перечисление с ≥1 дочерней `/properties/` страницей и
/// **непустым** html_path у самой страницы-родителя.
fn find_first_enum_paths(pages: &[Page]) -> Option<EnumPaths> {
    fn walk_collect(page: &Page) -> Option<EnumPaths> {
        if page.html_path.is_empty() {
            // У страниц-каталогов html_path может быть пуст — это контейнер,
            // а не страница перечисления. Идём вглубь.
            for c in &page.children {
                if let Some(found) = walk_collect(c) {
                    return Some(found);
                }
            }
            return None;
        }
        let value_paths: Vec<String> = page
            .children
            .iter()
            .filter(|c| c.html_path.contains("/properties/"))
            .map(|c| c.html_path.clone())
            .collect();
        if !value_paths.is_empty() {
            return Some(EnumPaths {
                html_path: page.html_path.clone(),
                title_ru: page.title.ru.clone(),
                title_en: page.title.en.clone(),
                value_paths,
            });
        }
        for c in &page.children {
            if let Some(found) = walk_collect(c) {
                return Some(found);
            }
        }
        None
    }
    for root in pages {
        let is_enum_catalog = root.title.ru == "Системные наборы значений"
            || root.title.ru == "Системные перечисления";
        if is_enum_catalog {
            for ty in &root.children {
                if let Some(found) = walk_collect(ty) {
                    return Some(found);
                }
            }
        }
        if let Some(found) = walk_collect(root) {
            return Some(found);
        }
    }
    None
}

#[test]
fn parse_concrete_text_layout_enum() {
    let Some(path) = hbk_path() else {
        eprintln!("skip: BSL_CONTEXT_PLATFORM_PATH не задан или shcntx_ru.hbk не найден");
        return;
    };

    let mut content = HbkContent::read(&path).expect("hbk open");

    // Канонический пример из карточки #638.
    let paths = collect_enum_paths(&content.toc.pages, "ТипРазмещенияТекстаТабличногоДокумента")
        .expect("страница ТипРазмещенияТекстаТабличногоДокумента должна быть в TOC");

    let html = content.get_entry_text(&paths.html_path).expect("read html");
    let mut info = parse_enum_page(&html);

    println!(
        "EnumInfo: name_ru={:?} name_en={:?} desc_len={} value_paths={}",
        info.name_ru,
        info.name_en,
        info.description.len(),
        paths.value_paths.len()
    );

    for value_path in &paths.value_paths {
        let value_html = match content.get_entry_text(value_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("skip {value_path}: {e}");
                continue;
            }
        };
        let v = parse_enum_value_page(&value_html);
        println!("  value: ru={:?} en={:?}", v.name_ru, v.name_en);
        info.values.push(v);
    }

    assert!(
        !info.values.is_empty(),
        "у этого перечисления должны быть значения"
    );
    let names_ru: Vec<&str> = info.values.iter().map(|v| v.name_ru.as_str()).collect();
    for expected in ["Авто", "Забивать", "Обрезать", "Переносить"] {
        assert!(
            names_ru.contains(&expected),
            "ожидалось значение '{expected}', получено: {:?}",
            names_ru
        );
    }
}

#[test]
fn parse_first_enum_with_values_smoke() {
    let Some(path) = hbk_path() else {
        eprintln!("skip: BSL_CONTEXT_PLATFORM_PATH не задан");
        return;
    };

    let mut content = HbkContent::read(&path).expect("hbk open");

    let paths = find_first_enum_paths(&content.toc.pages)
        .expect("должна быть хотя бы одна страница-перечисление с детьми /properties/");

    let html = content.get_entry_text(&paths.html_path).expect("read html");
    let mut info = parse_enum_page(&html);

    for value_path in &paths.value_paths {
        let value_html = content.get_entry_text(value_path).expect("read child");
        info.values.push(parse_enum_value_page(&value_html));
    }
    println!(
        "SMOKE: ru={:?} en={:?} values={}",
        paths.title_ru,
        info.name_en,
        info.values.len()
    );
    assert!(!info.name_ru.is_empty());
    assert!(!info.values.is_empty());
}
