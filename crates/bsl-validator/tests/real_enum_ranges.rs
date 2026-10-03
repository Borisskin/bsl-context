//! Регрессии по issue #22: значения-диапазоны в справке, устаревшие значения и
//! переменная цикла, названная как системное перечисление.
//!
//! Все три класса дают ложную находку `unknown_enum_value` с `confidence: high`
//! (её пропускает даже профиль `strict`), причём на коде, который платформа
//! исполняет: `Клавиша.A` — горячие клавиши, `ОтображениеОбычнойГруппы.Линия` —
//! совместимость, `ГруппировкаКолонок` — переменная цикла в типовом коде.

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

#[test]
fn issue22_key_enum_ranges_are_accepted() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
&НаКлиенте
Процедура Тест()
	К1 = Клавиша.A;
	К2 = Клавиша.F1;
	К3 = Клавиша.Num0;
	К4 = Клавиша._1;
	Сочетание = Новый СочетаниеКлавиш(Клавиша.S, Ложь, Истина, Истина);
КонецПроцедуры
";
    for level in [1u8, 2, 3] {
        let found = findings(&index, src, level);
        assert!(
            found.is_empty(),
            "level={level}: значения из диапазонов справки обязаны приниматься: {found:#?}"
        );
    }
}

/// Обратная сторона: значение вне диапазона по-прежнему находка, и подсказка
/// приходит из развёрнутого списка (а не из литерала `F1...F12`).
#[test]
fn issue22_value_outside_range_is_still_reported() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
&НаКлиенте
Процедура Тест()
	К = Клавиша.F13;
КонецПроцедуры
";
    let found = findings(&index, src, 3);
    assert_eq!(found.len(), 1, "ожидалась одна находка: {found:#?}");
    assert_eq!(found[0].0, ExprErrorKind::UnknownEnumValue);
}

#[test]
fn issue22_deprecated_values_are_accepted() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    // Автор issue проверил эти значения на живой платформе (8.3.17.1549 и
    // 8.3.27.2214): платформа их принимает, в справке их уже нет.
    let src = "\
Процедура Тест()
	А = ОтображениеОбычнойГруппы.Линия;
	Б = ОтображениеОбычнойГруппы.РамкаГруппы;
КонецПроцедуры
";
    let found = findings(&index, src, 3);
    assert!(
        found.is_empty(),
        "устаревшие значения из словаря совместимости не должны давать находок: {found:#?}"
    );
}

/// Словарь точечный: чужое имя у того же типа по-прежнему находка.
#[test]
fn issue22_unknown_value_of_same_enum_is_still_reported() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
Процедура Тест()
	А = ОтображениеОбычнойГруппы.ЛинияХ;
КонецПроцедуры
";
    let found = findings(&index, src, 3);
    assert_eq!(
        found.len(),
        1,
        "опечатка обязана остаться находкой: {found:#?}"
    );
}

/// Issue #22.3: переменная цикла, названная как системное перечисление.
#[test]
fn issue22_loop_variable_named_as_enum_is_not_checked() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
&НаСервере
Процедура Тест(Список)
	Для Каждого ГруппировкаКолонок Из Список Цикл
		Х = ГруппировкаКолонок.Значение;
	КонецЦикла;
КонецПроцедуры
";
    for level in [2u8, 3] {
        let found = findings(&index, src, level);
        assert!(
            found.is_empty(),
            "level={level}: переменная цикла — локальное имя, не перечисление: {found:#?}"
        );
    }
}
