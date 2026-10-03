//! Integration-тесты Phase 8 MVP (Уровень 2 — локальный type inference).
//!
//! Acceptance:
//! 1. Опечатка в свойстве через `Запрос = Новый Запрос` ловится уже на level=1:
//!    тип берётся из КОНСТРУКТОРА — это явное имя типа в исходнике, а не вывод.
//! 2. Переменная без конструктора (`Х = Список.Отбор.Элементы.Добавить()`) на
//!    level=1 молчит: тип в тексте не виден, а имя переменной не подставляется
//!    вместо типа, даже если совпало с именем платформенного типа (issue #11).
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
fn constructor_type_catches_typo_at_all_levels() {
    let Some(path) = hbk_path() else {
        eprintln!("skip: hbk не найден");
        return;
    };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    // Тип переменной задан КОНСТРУКТОРОМ — это явное имя типа в исходнике, а не
    // вывод типов. Поэтому опечатка ловится уже на Уровне 1, и независимо от
    // того, совпадает ли имя переменной с именем платформенного типа.
    let src = "\
Процедура Тест()
    МойЗапрос = Новый Запрос;
    МойЗапрос.Текстъ = \"ВЫБРАТЬ 1\";
КонецПроцедуры";

    for level in [1, 2] {
        let r = validate_expression_at_level(&index, src, level);
        println!("--- level={level} ---\n{r:#?}");
        let err = r
            .errors
            .iter()
            .find(|e| e.kind == ExprErrorKind::UnknownTypeMember)
            .unwrap_or_else(|| panic!("level {level}: должна быть ошибка UnknownTypeMember"));
        assert_eq!(err.suggestion.as_deref(), Some("Текст"));
    }
}

#[test]
fn variable_without_constructor_is_silent_at_level1() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    // Тип переменной в тексте не виден (результат вызова метода) — на Уровне 1
    // проверять члены не по чему, и валидатор молчит, а не выдумывает тип по
    // совпадению имени переменной с именем платформенного типа (issue #11:
    // `ЭлементОтбора` — имя типа, но здесь это переменная).
    let src = "\
Процедура Тест()
    ЭлементОтбора = Список.Отбор.Элементы.Добавить();
    ЭлементОтбора.ЛевоеЗначение = 1;
КонецПроцедуры";

    let r = validate_expression_at_level(&index, src, 1);
    println!("--- level=1 без конструктора ---\n{r:#?}");
    assert!(
        !r.errors
            .iter()
            .any(|e| e.kind == ExprErrorKind::UnknownTypeMember),
        "не должно быть UnknownTypeMember: {:#?}",
        r.errors
    );
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
