//! Integration-тесты на реальном `shcntx_ru.hbk` платформы 1С.
//!
//! Запуск:
//! ```pwsh
//! $env:BSL_CONTEXT_PLATFORM_PATH = 'C:\Program Files\1cv8\8.3.27.1786'
//! cargo test -p hbk-reader --test real_hbk -- --nocapture
//! ```
//!
//! Без env-переменной тесты ничего не делают (skip).

use std::path::PathBuf;

use hbk_reader::HbkContent;

fn platform_path() -> Option<PathBuf> {
    std::env::var("BSL_CONTEXT_PLATFORM_PATH")
        .ok()
        .map(PathBuf::from)
}

fn hbk_path() -> Option<PathBuf> {
    let root = platform_path()?;
    let candidates = [
        root.join("shcntx_ru.hbk"),
        root.join("bin").join("shcntx_ru.hbk"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

#[test]
fn opens_real_hbk_and_returns_nonempty_toc() {
    let Some(path) = hbk_path() else {
        eprintln!("skip: BSL_CONTEXT_PLATFORM_PATH не задан или shcntx_ru.hbk не найден");
        return;
    };

    let content = HbkContent::read(&path).expect("HbkContent::read должен открыть hbk");
    let pages = &content.toc.pages;

    assert!(!pages.is_empty(), "TOC пустой — что-то не так с парсером");

    // Первые 5 страниц для глаз — печать в --nocapture
    for (i, p) in pages.iter().take(5).enumerate() {
        println!(
            "root[{i}]: ru={:?} en={:?} html={:?} children={}",
            p.title.ru,
            p.title.en,
            p.html_path,
            p.children.len()
        );
    }

    // Простая sanity-check: хотя бы у одной корневой страницы должно быть
    // непустое название (TOC не должен состоять из одних пустышек).
    let any_named = pages
        .iter()
        .any(|p| !p.title.ru.is_empty() || !p.title.en.is_empty());
    assert!(any_named, "ни одна корневая страница не имеет названия");
}

#[test]
fn reads_first_html_page() {
    let Some(path) = hbk_path() else {
        eprintln!("skip: BSL_CONTEXT_PLATFORM_PATH не задан или shcntx_ru.hbk не найден");
        return;
    };

    let mut content = HbkContent::read(&path).expect("hbk open");

    // Найти первую страницу с непустым htmlPath (рекурсивно).
    fn find_html(pages: &[hbk_reader::Page]) -> Option<&str> {
        for p in pages {
            if !p.html_path.is_empty() {
                return Some(&p.html_path);
            }
            if let Some(c) = find_html(&p.children) {
                return Some(c);
            }
        }
        None
    }

    let html_path = find_html(&content.toc.pages)
        .expect("в TOC должен быть хотя бы один htmlPath")
        .to_string();
    let bytes = content
        .get_entry(&html_path)
        .expect("html-страница должна читаться из FileStorage");

    println!(
        "html_path={html_path} size={} first128={:?}",
        bytes.len(),
        String::from_utf8_lossy(&bytes[..bytes.len().min(128)])
    );

    assert!(!bytes.is_empty(), "html-страница пустая");
}
