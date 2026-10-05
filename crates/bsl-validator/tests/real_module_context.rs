//! Issue #19: неквалифицированный вызов в модуле менеджера, объекта и обычной
//! формы разрешается методом КОНТЕКСТА модуля, а не глобальной функцией.
//!
//! Автор issue воспроизвёл два случая на живой платформе:
//!
//! - модуль менеджера (`Catalogs/ЕдиницыИзмерения/Ext/ManagerModule.bsl`):
//!   `ПолучитьДанныеВыбора(Параметры)` — метод менеджера, а сверялось с глобальной
//!   функцией и давало `wrong_argument_count` с `confidence: high`;
//! - модуль ОБЫЧНОЙ формы: `ПолучитьФорму(…, …, …, …, …, …)` — это
//!   `ДокументОбъект.ПолучитьФорму(<Форма>, <Владелец>, <КлючУникальности>)`
//!   с тремя параметрами, а не глобальная функция с диапазоном 1..6.
//!
//! Признак обычной формы — отсутствие директив компиляции (`&НаКлиенте`
//! и подобных): у управляемой формы они есть всегда, и её контекст — сама форма,
//! а не объект-владелец.

use std::path::PathBuf;

use bsl_validator::{validate_module_with_profile, ExprErrorKind, Profile};
use platform_index::load_from_hbk;

const MANAGER_MODULE: &str = "Catalogs/ЕдиницыИзмерения/Ext/ManagerModule.bsl";
const ORDINARY_FORM_MODULE: &str =
    "Documents/ВозвратТоваровОтПокупателя/Forms/ФормаДокумента/Ext/Form/Module.bsl";

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
    module_path: &str,
    level: u8,
) -> Vec<(ExprErrorKind, String)> {
    validate_module_with_profile(index, src, Some(module_path), None, level, Profile::Full)
        .errors
        .into_iter()
        .map(|e| (e.kind, e.message))
        .collect()
}

fn argument_findings(
    index: &platform_index::PlatformIndex,
    src: &str,
    module_path: &str,
    level: u8,
) -> Vec<String> {
    findings(index, src, module_path, level)
        .into_iter()
        .filter(|(kind, _)| *kind == ExprErrorKind::WrongArgumentCount)
        .map(|(_, message)| message)
        .collect()
}

/// Модуль менеджера: вызов метода менеджера не сверяется с глобальной функцией.
#[test]
fn issue19_manager_module_call_uses_manager_signature() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
Процедура ОбработкаПолученияДанныхВыбора(ДанныеВыбора, Параметры, СтандартнаяОбработка)
	СтандартнаяОбработка = Ложь;
	ДанныеВыбора = ПолучитьДанныеВыбора(Параметры);
КонецПроцедуры
";
    for level in [2u8, 3] {
        let found = argument_findings(&index, src, MANAGER_MODULE, level);
        assert!(
            found.is_empty(),
            "level={level}: метод менеджера не сверяется с глобальной функцией: {found:#?}"
        );
    }
}

/// Issue #32, часть 2: переадресация `ПолучитьДанныеВыбора` глобальному
/// контексту документирована в справке менеджера — при двух параметрах в модуле
/// менеджера вызывается глобальная функция (у неё два параметра). Три аргумента
/// не принимает ни одна из сигнатур, и находка обязана остаться.
#[test]
fn issue32_manager_redirect_keeps_both_signatures() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let two = "\
Процедура Т(Параметры)
	Д = ПолучитьДанныеВыбора(Параметры, Неопределено);
КонецПроцедуры
";
    let found = argument_findings(&index, two, MANAGER_MODULE, 3);
    assert!(
        found.is_empty(),
        "переадресованный вызов с двумя параметрами законен: {found:#?}"
    );

    let three = "\
Процедура Т(Параметры)
	Д = ПолучитьДанныеВыбора(Параметры, Неопределено, Ложь);
КонецПроцедуры
";
    let found = argument_findings(&index, three, MANAGER_MODULE, 3);
    assert_eq!(
        found.len(),
        1,
        "три аргумента не принимает ни одна сигнатура: {found:#?}"
    );
}

/// Модуль обычной формы: `ПолучитьФорму` — метод объекта-владельца (3 параметра).
#[test]
fn issue19_ordinary_form_call_uses_object_signature() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    // Шесть аргументов: глобальная функция их принимает, метод объекта — нет.
    let six = "\
Процедура Тест()
	Форма = ПолучитьФорму(\"Обработка.Х.Форма\", Неопределено, Неопределено, Ложь, Неопределено, Неопределено);
КонецПроцедуры
";
    let found = argument_findings(&index, six, ORDINARY_FORM_MODULE, 3);
    assert_eq!(
        found.len(),
        1,
        "в обычной форме вызов сверяется с методом объекта: {found:#?}"
    );
    assert!(
        found[0].contains("ПолучитьФорму"),
        "в сообщении должен быть метод: {}",
        found[0]
    );

    // Три аргумента — сигнатура метода объекта, находки быть не должно.
    let three = "\
Процедура Тест()
	Форма = ПолучитьФорму(\"Обработка.Х.Форма\", Неопределено, Неопределено);
КонецПроцедуры
";
    let clean = argument_findings(&index, three, ORDINARY_FORM_MODULE, 3);
    assert!(
        clean.is_empty(),
        "три аргумента — это сигнатура метода объекта: {clean:#?}"
    );
}

/// Управляемая форма: директивы компиляции есть, методов объекта в контексте нет,
/// поэтому шесть аргументов `ПолучитьФорму` — законный вызов глобальной функции.
#[test]
fn issue19_managed_form_keeps_global_signature() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
&НаКлиенте
Процедура Тест()
	Форма = ПолучитьФорму(\"Обработка.Х.Форма\", Неопределено, Неопределено, Ложь, Неопределено, Неопределено);
КонецПроцедуры
";
    let found = argument_findings(&index, src, ORDINARY_FORM_MODULE, 3);
    assert!(
        found.is_empty(),
        "у управляемой формы контекст — форма, а не объект: {found:#?}"
    );
}

/// Неизвестный путь (общий модуль) — поведение прежнее: глобальная сигнатура.
#[test]
fn issue19_common_module_keeps_global_signature() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
Процедура Тест()
	Длина = СтрДлина(\"a\", \"b\");
КонецПроцедуры
";
    let found = argument_findings(
        &index,
        src,
        "CommonModules/ОбщегоНазначения/Ext/Module.bsl",
        3,
    );
    assert_eq!(
        found.len(),
        1,
        "настоящая ошибка обязана остаться: {found:#?}"
    );
}
