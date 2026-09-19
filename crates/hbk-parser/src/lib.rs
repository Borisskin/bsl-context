//! Парсеры html-страниц `shcntx_ru.hbk`.
//!
//! Pipeline (Phase 2): hbk-страница (UTF-8 html) → структурированный `*Info` (`MethodInfo`,
//! `PropertyInfo`, `ObjectInfo`, `EnumInfo`, `EnumValueInfo`, `ConstructorInfo`).
//!
//! Архитектура отличается от апстрима: апстрим использует SAX-обработчики
//! поверх Ksoup (см. `BlockHandler.kt`), мы используем DOM-подход через
//! `scraper` — режем страницу на главы по маркерам `<p class="V8SH_chapter">`/
//! `<hr>`, потом каждую главу обрабатываем функцией под её тип.
//!
//! Главное исправление vs апстрима — на уровне business-mapper'а (Phase 3):
//! апстрим теряет `signature` методов и `constructors`, не сохраняет `EnumInfo`.
//! Сами hbk-парсеры апстрима в основном корректны.

pub mod blocks;
pub mod constructor_page;
pub mod enum_page;
pub mod enum_value;
pub mod html;
pub mod method_page;
pub mod models;
pub mod object_page;
pub mod property_page;

pub use constructor_page::parse_constructor_page;
pub use enum_page::parse_enum_page;
pub use enum_value::parse_enum_value_page;
pub use method_page::parse_method_page;
pub use object_page::parse_object_page;
pub use property_page::parse_property_page;

pub use models::{
    ConstructorInfo, EnumInfo, EnumValueInfo, MethodInfo, MethodParameterInfo, MethodSignatureInfo,
    ObjectInfo, PropertyInfo, RelatedObject, ValueInfo,
};
