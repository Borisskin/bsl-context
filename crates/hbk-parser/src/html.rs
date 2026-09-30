//! Общие хелперы парсинга html-страниц синтакс-помощника:
//! - `split_chapters` — режет страницу на главы по маркерам `<p class="V8SH_chapter">`/`<hr>`
//! - `extract_text` — текст узла без html-тегов
//! - `to_markdown` — html-фрагмент → Markdown (порт `MarkdownHtmlHandler.kt`)

use ego_tree::NodeRef;
use scraper::{Html, Node, Selector};
use std::sync::OnceLock;

/// Селектор `<body>` — статический: `Selector::parse` компилирует CSS-строку
/// заново, а `split_chapters` зовётся на каждую страницу справки (десятки тысяч
/// раз за сборку). Компиляция здесь не зависит от страницы, поэтому считается
/// один раз на процесс.
fn body_selector() -> &'static Selector {
    static SEL: OnceLock<Selector> = OnceLock::new();
    SEL.get_or_init(|| Selector::parse("body").expect("body selector"))
}

/// Глава html-страницы. Между двумя маркерами идёт «тело» главы — series of
/// верхнеуровневых элементов в DOM. Сохраняем их как html-фрагмент, чтобы
/// можно было пере-распарсить отдельным block-парсером.
#[derive(Debug, Clone)]
pub struct Chapter {
    /// Заголовок: текст внутри `<p class="V8SH_chapter">`. У первой главы
    /// (до любого маркера) заголовок — пустая строка; там обычно лежит
    /// `<p class="V8SH_title">` или `<p class="V8SH_heading">` с именем
    /// сущности — это «голова» страницы, обрабатывается NameBlockHandler.
    pub title: String,
    /// Сериализованный html всех узлов внутри главы (без самого маркера).
    pub body_html: String,
}

/// Разбить html-страницу синтакс-помощника на главы.
///
/// Маркеры между главами:
/// - `<p class="V8SH_chapter">Синтаксис:</p>` — заголовок становится `Chapter.title`,
///   следующее содержимое идёт в `body_html`.
/// - `<hr>` — то же, но без заголовка (используется редко, часто пустая глава).
pub fn split_chapters(html: &str) -> Vec<Chapter> {
    let doc = Html::parse_document(html);
    // Парсер scraper всегда оборачивает в <html><head>/<body>; берём body.
    let body = match doc.select(body_selector()).next() {
        Some(b) => b,
        None => return Vec::new(),
    };

    let mut chapters: Vec<Chapter> = vec![Chapter {
        title: String::new(),
        body_html: String::new(),
    }];

    for child in body.children() {
        if is_chapter_marker(child) {
            chapters.push(Chapter {
                title: chapter_title_of(child),
                body_html: String::new(),
            });
            continue;
        }
        // node serialize: scraper не имеет прямого .html() для узла-не-Element,
        // используем ego_tree NodeRef из node().
        if let Some(text) = serialize_node(child) {
            chapters
                .last_mut()
                .expect("at least one chapter")
                .body_html
                .push_str(&text);
        }
    }
    chapters
}

/// Текст узла без html-тегов и нормализованный (collapse whitespace, trim).
pub fn extract_text(html: &str) -> String {
    let doc = Html::parse_fragment(html);
    let mut buf = String::new();
    for node in doc.tree.nodes() {
        if let Node::Text(text) = node.value() {
            buf.push_str(text);
        }
    }
    collapse_whitespace(buf.trim())
}

/// Заменить последовательности whitespace на один пробел, сохранить начальные/
/// конечные пробелы как пустые строки убрать через trim снаружи.
pub fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_ws = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_ws {
                out.push(' ');
                prev_ws = true;
            }
        } else {
            out.push(ch);
            prev_ws = false;
        }
    }
    out
}

/// Конвертировать html-фрагмент в Markdown.
///
/// Порт `MarkdownHtmlHandler.kt` (alkoleft): обрабатывает h1-h6, p, br, strong/b,
/// em/i, code, pre, blockquote, ul/ol/li, a (с поддержкой `v8help://` ссылок —
/// они оборачиваются в `code`-кавычки, обычные ссылки — в `[text](href)`).
pub fn to_markdown(html: &str) -> String {
    let doc = Html::parse_fragment(html);
    let root = doc.tree.root();
    let mut state = MdState::default();
    walk(root, &mut state);
    state.output.trim().to_string()
}

#[derive(Default)]
struct MdState {
    output: String,
    list_level: usize,
    in_pre: bool,
    in_anchor: Option<String>, // href текущей ссылки
    anchor_text: String,
}

fn walk(node: NodeRef<Node>, st: &mut MdState) {
    match node.value() {
        Node::Document | Node::Fragment => {
            for child in node.children() {
                walk(child, st);
            }
        }
        Node::Text(text) => {
            if st.in_anchor.is_some() {
                st.anchor_text.push_str(text);
            } else {
                st.output.push_str(text);
            }
        }
        Node::Element(el) => {
            let name = el.name().to_ascii_lowercase();
            on_open_tag(&name, el, st);
            for child in node.children() {
                walk(child, st);
            }
            on_close_tag(&name, st);
        }
        _ => {}
    }
}

fn on_open_tag(name: &str, el: &scraper::node::Element, st: &mut MdState) {
    match name {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let lvl: usize = name[1..].parse().unwrap_or(1);
            st.output.push('\n');
            for _ in 0..lvl {
                st.output.push('#');
            }
            st.output.push(' ');
        }
        "p" => {
            if !st.output.is_empty() {
                st.output.push('\n');
            }
        }
        "br" => st.output.push('\n'),
        "strong" | "b" => st.output.push_str("**"),
        "em" | "i" => st.output.push('*'),
        "code" if !st.in_pre => st.output.push('`'),
        "pre" => {
            st.output.push_str("\n```\n");
            st.in_pre = true;
        }
        "blockquote" => st.output.push_str("\n> "),
        "ul" | "ol" => st.list_level += 1,
        "li" if st.list_level > 0 => {
            st.output.push('\n');
            for _ in 0..st.list_level.saturating_sub(1) {
                st.output.push_str("  ");
            }
            st.output.push_str("* ");
        }
        "a" => {
            let href = el.attr("href").map(|s| s.to_string()).unwrap_or_default();
            st.in_anchor = Some(href);
            st.anchor_text.clear();
        }
        _ => {}
    }
}

fn on_close_tag(name: &str, st: &mut MdState) {
    match name {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" => st.output.push('\n'),
        "strong" | "b" => st.output.push_str("**"),
        "em" | "i" => st.output.push('*'),
        "code" if !st.in_pre => st.output.push('`'),
        "pre" => {
            st.output.push_str("\n```\n");
            st.in_pre = false;
        }
        "blockquote" => st.output.push('\n'),
        "ul" | "ol" => {
            if st.list_level > 0 {
                st.list_level -= 1;
            }
        }
        "a" => {
            let href = st.in_anchor.take().unwrap_or_default();
            let text = collapse_whitespace(st.anchor_text.trim());
            if !text.is_empty() {
                if href.starts_with("v8help://") {
                    st.output.push('`');
                    st.output.push_str(&text);
                    st.output.push('`');
                } else {
                    st.output.push('[');
                    st.output.push_str(&text);
                    st.output.push_str("](");
                    st.output.push_str(&href);
                    st.output.push(')');
                }
            }
            st.anchor_text.clear();
        }
        _ => {}
    }
}

// ---- helpers --------------------------------------------------------------

fn is_chapter_marker(child: NodeRef<Node>) -> bool {
    if let Node::Element(el) = child.value() {
        let name = el.name().to_ascii_lowercase();
        if name == "hr" {
            return true;
        }
        if name == "p" && el.attr("class") == Some("V8SH_chapter") {
            return true;
        }
    }
    false
}

fn chapter_title_of(child: NodeRef<Node>) -> String {
    let mut buf = String::new();
    collect_text(child, &mut buf);
    collapse_whitespace(buf.trim())
}

fn collect_text(node: NodeRef<Node>, buf: &mut String) {
    if let Node::Text(text) = node.value() {
        buf.push_str(text);
    }
    for child in node.children() {
        collect_text(child, buf);
    }
}

fn serialize_node(node: NodeRef<Node>) -> Option<String> {
    match node.value() {
        Node::Element(_) => {
            // scraper::ElementRef::html() — используем через попытку вернуться
            // к ElementRef. Для не-Element узлов (Text, Comment) — собираем сами.
            let element_ref = scraper::ElementRef::wrap(node)?;
            Some(element_ref.html())
        }
        Node::Text(text) => Some(text.to_string()),
        _ => None,
    }
}
