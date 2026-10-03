//! Issue #21: соседние строковые литералы — один аргумент.
//!
//! Платформа склеивает соседние литералы через перевод строки
//! (`СтрДлина("a" "b")` → 3, `Формат(Дата, "ДФ=" "дддд")` → «суббота»), а
//! грамматика второй литерал отдаёт узлом `ERROR`. Пока он считался отдельным
//! аргументом, корректный код получал `wrong_argument_count` с `confidence: high`
//! — а это профиль `strict`, то есть находка проходила даже самую строгую
//! настройку. В реальном коде встречается как `Формат(ТекущаяДата(), "ДФ=" "дддд")`.

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
fn issue21_adjacent_literals_do_not_break_argument_count() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
Процедура Тест()
	ИмяДня = Формат(ТекущаяДата(), \"ДФ=\" \"дддд\");
	Длина = СтрДлина(\"a\" \"b\");
	Длина2 = СтрДлина(\"a\"
\"b\");
КонецПроцедуры
";
    for level in [1u8, 2, 3] {
        let found = findings(&index, src, level);
        assert!(
            found.is_empty(),
            "level={level}: соседние литералы — один аргумент: {found:#?}"
        );
    }
}

/// Обратная сторона: настоящее превышение числа аргументов остаётся находкой.
#[test]
fn issue21_real_argument_mismatch_is_still_reported() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
Процедура Тест()
	Длина = СтрДлина(\"a\", \"b\");
КонецПроцедуры
";
    let found = findings(&index, src, 3);
    assert_eq!(found.len(), 1, "ожидалась одна находка: {found:#?}");
    assert_eq!(found[0].0, ExprErrorKind::WrongArgumentCount);
}

/// Issue #26: запятые ВНУТРИ литерала-продолжения не делают его отдельным
/// аргументом. Грамматика режет второй литерал по запятым, и корректный код
/// получал `wrong_argument_count` с `confidence: high`.
#[test]
fn issue26_commas_inside_adjacent_literals_are_not_arguments() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
Процедура Тест()
	Т = НСтр(\"a\"
\"b, c, d\");
	Д = СтрДлина(\"a\"
\"b, c\");
	Т2 = НСтр(\"a, b, c\");
	Т3 = НСтр(\"a
|b, c, d\");
КонецПроцедуры
";
    for level in [1u8, 3] {
        let found = findings(&index, src, level);
        assert!(
            found.is_empty(),
            "level={level}: запятые внутри литералов — не аргументы: {found:#?}"
        );
    }
}

/// Реальный вызов из issue #26: многострочный `НСтр` с запятыми и вторым
/// аргументом — два аргумента, а не пять.
#[test]
fn issue26_real_multiline_nstr_is_clean() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
Процедура Тест()
	ОбщегоНазначения.СообщитьОбОшибке(НСтр(\"ru='Не удалось заблокировать %1: %2, для изменения основного банковского счета, по причине:'\"
\"%3';uk='Не вдалося заблокувати %1: %2, для зміни основного банківського рахунку, через:'\"
\"%3'\", ОбщегоНазначения.КодОсновногоЯзыка()));
КонецПроцедуры
";
    let found = findings(&index, src, 3);
    let argument_noise: Vec<_> = found
        .iter()
        .filter(|(kind, _)| *kind == ExprErrorKind::WrongArgumentCount)
        .collect();
    assert!(
        argument_noise.is_empty(),
        "НСтр из трёх литералов с запятыми — два аргумента: {argument_noise:#?}"
    );
}
