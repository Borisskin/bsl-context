//! Issue #15, класс 1: тип берётся из ближайшего присваивания ВЫШЕ точки, а не
//! из первого конструктора процедуры и не из присваивания ниже.
//!
//! До этой правки тип переменной был один на процедуру: слой локальных имён брал
//! ПЕРВОЕ присваивание с `Новый`, а вывод типов применял ПОСЛЕДНЕЕ присваивание
//! ко всем строкам процедуры, включая расположенные выше. Отсюда ложные находки
//! на членах: имя проверялось по типу, которого в этой точке ещё (или уже) нет.

use std::path::PathBuf;

use bsl_validator::{validate_module_with_profile, ExprErrorKind, Profile};
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

fn findings(
    index: &platform_index::PlatformIndex,
    src: &str,
    level: u8,
) -> Vec<(ExprErrorKind, String)> {
    validate_module_with_profile(index, src, None, None, level, Profile::Full)
        .errors
        .into_iter()
        .map(|e| (e.kind, e.message))
        .collect()
}

/// Переприсваивание другим типом: до него имя имеет прежний тип, после — новый.
///
/// Пара подобрана так, чтобы тест ловил именно позиционность: `НайтиСтроки` есть
/// только у `ТаблицаЗначений`, `ВГраница` — только у `Массива`. Пока тип брался из
/// первого конструктора процедуры, второе обращение проверялось по
/// `ТаблицаЗначений` и давало ложную находку.
#[test]
fn issue15_class1_type_changes_after_reassignment() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
Процедура Тест(Отбор)
	Данные = Новый ТаблицаЗначений;
	Найденные = Данные.НайтиСтроки(Отбор);
	Данные = Новый Массив;
	Граница = Данные.ВГраница();
КонецПроцедуры
";
    for level in [2u8, 3] {
        let found = findings(&index, src, level);
        assert!(
            found.is_empty(),
            "level={level}: у каждой строки свой тип: {found:#?}"
        );
    }
}

/// Присваивание НИЖЕ точки не должно давать тип этой строке.
#[test]
fn issue15_class1_type_does_not_leak_from_below() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
Процедура Тест()
	Данные.НетТакогоЧлена();
	Данные = Новый Массив;
КонецПроцедуры
";
    let found = findings(&index, src, 3);
    assert!(
        found.is_empty(),
        "тип из присваивания ниже не должен применяться выше: {found:#?}"
    );
}

/// Разные типы в ветвях `Если/Иначе`: после `КонецЕсли` тип — объединение, поэтому
/// член, существующий хотя бы у одной альтернативы, находкой не считается.
///
/// Пара выбрана так, чтобы тест ловил именно объединение: `НайтиСтроки` есть у
/// `ТаблицаЗначений` и отсутствует у `Массива`, то есть при типе «только из
/// последней ветви» находка была бы ложной.
#[test]
fn issue15_class1_branch_types_are_unioned() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
Процедура Тест(Флаг, Отбор)
	Если Флаг Тогда
		Данные = Новый Массив;
	Иначе
		Данные = Новый ТаблицаЗначений;
	КонецЕсли;
	Найденные = Данные.НайтиСтроки(Отбор);
КонецПроцедуры
";
    for level in [2u8, 3] {
        let found = findings(&index, src, level);
        assert!(
            found.is_empty(),
            "level={level}: член есть у одной из ветвей: {found:#?}"
        );
    }
}

/// Обратная сторона объединения: члена нет НИ У ОДНОЙ ветви — находка остаётся.
#[test]
fn issue15_class1_member_missing_in_all_branches_is_reported() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
Процедура Тест(Флаг)
	Если Флаг Тогда
		Данные = Новый ТаблицаЗначений;
	Иначе
		Данные = Новый Массив;
	КонецЕсли;
	Данные.НетТакогоЧлена();
КонецПроцедуры
";
    let found = findings(&index, src, 3);
    assert_eq!(found.len(), 1, "ожидалась одна находка: {found:#?}");
    assert_eq!(found[0].0, ExprErrorKind::UnknownTypeMember);
}
