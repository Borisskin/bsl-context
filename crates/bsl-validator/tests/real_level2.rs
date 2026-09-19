//! Integration-тесты Phase 8 MVP (Уровень 2 — локальный type inference).
//!
//! Acceptance:
//! 1. Опечатка в свойстве через `Запрос = Новый Запрос` ловится только на level=2.
//! 2. На level=1 та же опечатка пропускается (head — переменная, не тип).
//! 3. Аннотация `// @type ТаблицаЗначений` помогает вывести тип.
//! 4. `Х = ТипРазмещенияТекстаТабличногоДокумента.Переносить; Х.Лажа` — ловится на level=2.

use std::path::PathBuf;

use bsl_validator::{validate_expression_at_level, ExprErrorKind};
use platform_index::load_from_hbk;

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

#[test]
fn level1_misses_local_var_typo_level2_catches() {
    let Some(path) = hbk_path() else {
        eprintln!("skip: hbk не найден");
        return;
    };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    // Переменная с именем, отличным от имени типа — на Уровне 1 валидатор не
    // знает её тип и пропускает опечатку. На Уровне 2 — выводит тип через
    // 'Новый Запрос' и ловит ошибку.
    let src = "\
Процедура Тест()
    МойЗапрос = Новый Запрос;
    МойЗапрос.Текстъ = \"ВЫБРАТЬ 1\";
КонецПроцедуры";

    let r1 = validate_expression_at_level(&index, src, 1);
    println!("--- level=1 ---\n{r1:#?}");
    // На Уровне 1 'МойЗапрос' слева — переменная, не тип, проверка скипается.
    assert!(
        r1.valid,
        "level 1 не должен ловить опечатки в локальных переменных"
    );

    let r2 = validate_expression_at_level(&index, src, 2);
    println!("--- level=2 ---\n{r2:#?}");
    assert!(!r2.valid, "level 2 должен поймать опечатку 'Текстъ'");
    let err = r2
        .errors
        .iter()
        .find(|e| e.kind == ExprErrorKind::UnknownTypeMember)
        .expect("должна быть ошибка UnknownTypeMember");
    assert_eq!(err.suggestion.as_deref(), Some("Текст"));
}

#[test]
fn level2_uses_type_annotation_directive() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    // Аннотация подсказывает тип, валидатор ловит опечатку метода 'Колонкы'.
    let src = "\
Процедура Тест()
    // @type ТаблицаЗначений
    ТЗ = СоздатьТЗ();
    ТЗ.Колонкы.Добавить(\"Поле\");
КонецПроцедуры";

    let r1 = validate_expression_at_level(&index, src, 1);
    assert!(r1.valid, "level 1 не должен ловить через аннотацию");

    let r2 = validate_expression_at_level(&index, src, 2);
    println!("--- level=2 annotation ---\n{r2:#?}");
    assert!(!r2.valid, "level 2 должен поймать 'Колонкы'");
    assert!(r2
        .errors
        .iter()
        .any(|e| e.kind == ExprErrorKind::UnknownTypeMember));
}

#[test]
fn level2_does_not_break_level1_passing_code() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    // Корректный код — должен оставаться valid и на level=2.
    let src = "\
Процедура Тест()
    ТЗ = Новый ТаблицаЗначений;
    ТЗ.Колонки.Добавить(\"Поле\");
КонецПроцедуры";

    let r2 = validate_expression_at_level(&index, src, 2);
    println!("--- level=2 OK ---\n{r2:#?}");
    assert!(
        r2.valid,
        "корректный код не должен порождать ошибок на level=2: {:#?}",
        r2.errors
    );
}

#[test]
fn level2_inference_from_enum_value_assignment() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    // Х = ТипРазмещенияТекстаТабличногоДокумента.Переносить → Х: ТипРазмещения...
    // Затем Х.Лажа — опечатка в значении.
    let src = "\
Процедура Тест()
    Х = ТипРазмещенияТекстаТабличногоДокумента.Переносить;
    Y = Х.Перенос;
КонецПроцедуры";

    let r2 = validate_expression_at_level(&index, src, 2);
    println!("--- level=2 enum inference ---\n{r2:#?}");
    assert!(!r2.valid);
    let err = r2
        .errors
        .iter()
        .find(|e| e.kind == ExprErrorKind::UnknownEnumValue)
        .expect("должна быть UnknownEnumValue для Х.Перенос");
    assert_eq!(err.suggestion.as_deref(), Some("Переносить"));
}
