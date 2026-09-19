//! Высокоуровневое чтение hbk: TOC из PackBlock + html-страницы из FileStorage.
//!
//! Порт `HbkContentReader.kt` (alkoleft).
//!
//! Pipeline:
//! 1. `HbkContainer::read(path)` — entities-таблица + сырые байты.
//! 2. `PackBlock` (entity name `"PackBlock"`) — это zip-контейнер с одним entry,
//!    внутри которого UTF-8 текст TOC. Распаковываем, парсим, получаем `Toc`.
//! 3. `FileStorage` (entity name `"FileStorage"`) — это zip-архив с html-страницами.
//!    Складываем в `ZipArchive<Cursor<Vec<u8>>>`, страницы достаём по `htmlPath`.

use std::io::{Cursor, Read};
use std::path::Path;

use zip::ZipArchive;

use crate::container::HbkContainer;
use crate::error::{HbkError, Result};
use crate::models::Toc;
use crate::toc::parse_toc;

const PACK_BLOCK_NAME: &str = "PackBlock";
const FILE_STORAGE_NAME: &str = "FileStorage";

/// Содержимое hbk-файла, готовое для парсинга html-страниц.
pub struct HbkContent {
    pub toc: Toc,
    file_storage: ZipArchive<Cursor<Vec<u8>>>,
}

impl HbkContent {
    /// Прочитать hbk-файл целиком: распаковать TOC, открыть FileStorage.
    pub fn read(path: &Path) -> Result<Self> {
        let container = HbkContainer::read(path)?;

        // PackBlock — zip с одним entry, внутри UTF-8 текст TOC.
        let pack_block = container.get_entity(PACK_BLOCK_NAME)?;
        let toc_text = inflate_pack_block(&pack_block)?;
        let toc = parse_toc(&toc_text)?;

        // FileStorage — zip с html-страницами.
        let file_storage_bytes = container.get_entity(FILE_STORAGE_NAME)?;
        let cursor = Cursor::new(file_storage_bytes);
        let file_storage = ZipArchive::new(cursor)?;

        Ok(Self { toc, file_storage })
    }

    /// Прочитать html-страницу по её `htmlPath` из TOC.
    ///
    /// Имена в zip-архиве могут начинаться с `/` — апстрим срезает ведущий слэш,
    /// мы делаем то же самое.
    pub fn get_entry(&mut self, html_path: &str) -> Result<Vec<u8>> {
        if html_path.is_empty() {
            return Err(HbkError::HtmlEntryNotFound("пустое имя файла".to_string()));
        }
        let name = html_path.strip_prefix('/').unwrap_or(html_path);
        let mut entry = self
            .file_storage
            .by_name(name)
            .map_err(|_| HbkError::HtmlEntryNotFound(html_path.to_string()))?;
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf)?;
        Ok(buf)
    }

    /// Прочитать html-страницу как UTF-8 строку.
    ///
    /// Платформа 1С хранит страницы синтакс-помощника в UTF-8 без BOM.
    /// Если когда-нибудь встретится cp1251 — расширим через `encoding_rs`.
    pub fn get_entry_text(&mut self, html_path: &str) -> Result<String> {
        let bytes = self.get_entry(html_path)?;
        String::from_utf8(bytes)
            .map_err(|e| HbkError::BadFormat(format!("страница {html_path}: не UTF-8 ({e})")))
    }
}

/// Распаковать PackBlock: это zip stream без central directory (только
/// Local File Header + deflate-данные + Data Descriptor). Апстрим использует
/// `ZipInputStream`, который как раз streaming. Rust `ZipArchive::new` требует
/// EOCD и тут не работает («Could not find EOCD»). Используем
/// `zip::read::read_zipfile_from_stream` — streaming-API без EOCD.
fn inflate_pack_block(data: &[u8]) -> Result<String> {
    let mut cursor = Cursor::new(data);
    let mut entry = zip::read::read_zipfile_from_stream(&mut cursor)
        .map_err(|e| HbkError::Inflate(format!("PackBlock не zip-stream: {e}")))?
        .ok_or_else(|| HbkError::Inflate("PackBlock не содержит entry".into()))?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;
    String::from_utf8(buf).map_err(|e| HbkError::Inflate(format!("PackBlock не UTF-8: {e}")))
}
