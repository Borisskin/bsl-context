//! Issue #20: буквы кириллицы вне русского алфавита.
//!
//! Платформа принимает в именах любые буквы Unicode, а грамматика
//! `tree-sitter-bsl` описывает идентификатор как `[\wа-я_]` — только русский
//! алфавит. Украинские, белорусские и казахские буквы (`і`, `ї`, `є`, `ґ`, `ў`,
//! `қ`) рвали имя на куски, и корректный код получал `wrong_argument_count` с
//! `confidence: high` — то есть находку, которую пропускает даже профиль
//! `strict`. Для украиноязычных конфигураций это массовый класс: в замере автора
//! 1137 находок на `НСтр` (двуязычные ru/uk сообщения) и ещё около трёх десятков
//! на `Прав`, `СтрДлина`, `ПустаяСтрока`, `Формат` с украинскими именами.

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
fn issue20_ukrainian_identifiers_do_not_break_argument_count() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
Процедура Тест()
	Закінчення = \"ок\";
	Остаток = Прав(Закінчення, 1);
	Длина = СтрДлина(Закінчення);
	Пусто = ПустаяСтрока(Закінчення);
	єСумма = 1;
	Текст = Формат(єСумма, \"ЧДЦ=2\");
КонецПроцедуры
";
    for level in [1u8, 2, 3] {
        let found = findings(&index, src, level);
        assert!(
            found.is_empty(),
            "level={level}: украинские имена — законные идентификаторы: {found:#?}"
        );
    }
}

/// Многострочный `НСтр` из соседних литералов (issue #20 + #21): в выгрузке он
/// записан несколькими строками, и буква вне русского алфавита внутри литерала
/// раньше сбивала и склейку, и подсчёт аргументов.
#[test]
fn issue20_multiline_nstr_with_ukrainian_letters() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
Процедура Тест(ОписаниеОшибки)
	ОбщегоНазначения.СообщитьОбОшибке(НСтр(\"ru='Не удалось записать элемент:'
\"';uk='Не вдалося записати елемент:'
\"'\") + ОписаниеОшибки);
КонецПроцедуры
";
    let found = findings(&index, src, 3);
    let argument_noise: Vec<_> = found
        .iter()
        .filter(|(kind, _)| *kind == ExprErrorKind::WrongArgumentCount)
        .collect();
    assert!(
        argument_noise.is_empty(),
        "НСтр из соседних литералов — один аргумент: {argument_noise:#?}"
    );
}

/// Обратная сторона: имя с буквой вне русского алфавита по-прежнему участвует в
/// проверках — если имя типа написано неверно, находка остаётся.
#[test]
fn issue20_unknown_type_still_reported() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
Процедура Тест()
	Х = Новый ЗакінченняНетТакого;
КонецПроцедуры
";
    let found = findings(&index, src, 3);
    assert!(
        found
            .iter()
            .any(|(kind, _)| *kind == ExprErrorKind::UnknownNewType),
        "несуществующий тип обязан остаться находкой: {found:#?}"
    );
}
