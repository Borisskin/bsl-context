//! Регресс на issue #11 (github.com/Regsorm/bsl-context/issues/11): тип переменной
//! определялся по её ИМЕНИ, если имя совпадало с именем платформенного типа, и
//! присвоенное значение при этом игнорировалось.
//!
//! ```bsl
//! Блокировка = Новый БлокировкаДанных;
//! ЭлементБлокировки = Блокировка.Добавить("РегистрСведений.Тест");
//! ```
//!
//! Здесь `Блокировка` — ПЕРЕМЕННАЯ типа `БлокировкаДанных`, но имя совпало с
//! именем COM-типа `Блокировка` (`IObjectLock`), и проверка членов шла по нему:
//! «У типа 'Блокировка' нет члена 'Добавить'» с `confidence: low`. Подсказка
//! ведёт к правке рабочего кода, поэтому находка опаснее пропуска.
//!
//! Тесты держат обе стороны размена:
//!
//! - законный код с именем-тенью не даёт находок (`unknown_type_member`);
//! - опечатка в члене переменной, названной по своему типу, ПО-ПРЕЖНЕМУ ловится
//!   (`Запрос = Новый Запрос;` → `Запрос.Текстъ`) — иначе лечение шума съело бы
//!   правило целиком;
//! - обращение к самому платформенному типу (без локальной переменной с таким
//!   именем) проверяется как раньше.
//!
//! Предпосылки проверяются на реальном `shcntx_ru.hbk`: `БлокировкаДанных`
//! действительно имеет `Добавить` и `Заблокировать`, а подставленный по имени
//! тип `Блокировка` — нет; поэтому `valid: false` из issue ложен по существу.

use std::path::PathBuf;

use bsl_validator::{validate_module_with_profile, Confidence, ExprErrorKind, Profile};
use platform_index::{load_from_hbk, PlatformIndex, Type};

const FORM_MODULE: &str = "base/Catalogs/Х/Forms/Ф/Ext/Form/Module.bsl";

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

fn has_method(ty: &Type, name: &str) -> bool {
    let n = name.to_lowercase();
    ty.methods
        .iter()
        .any(|m| m.name_ru.to_lowercase() == n || m.name_en.to_lowercase() == n)
}

fn has_property(ty: &Type, name: &str) -> bool {
    let n = name.to_lowercase();
    ty.properties
        .iter()
        .any(|p| p.name_ru.to_lowercase() == n || p.name_en.to_lowercase() == n)
}

/// Сообщения `unknown_type_member` при проверке модуля на заданном уровне.
fn type_member_findings(
    index: &PlatformIndex,
    src: &str,
    module_path: Option<&str>,
    level: u8,
) -> Vec<String> {
    validate_module_with_profile(index, src, module_path, None, level, Profile::Full)
        .errors
        .iter()
        .filter(|e| e.kind == ExprErrorKind::UnknownTypeMember)
        .map(|e| format!("{}:{} {}", e.line, e.col, e.message))
        .collect()
}

/// Все находки — для контроля, что законный код не ловит ничего вовсе.
fn all_findings(
    index: &PlatformIndex,
    src: &str,
    module_path: Option<&str>,
    level: u8,
) -> Vec<String> {
    validate_module_with_profile(index, src, module_path, None, level, Profile::Full)
        .errors
        .iter()
        .map(|e| format!("{:?} {}:{} {}", e.kind, e.line, e.col, e.message))
        .collect()
}

// ── Воспроизведение 1 из issue: управляемая блокировка ─────────────────────

#[test]
fn issue11_locking_pattern_yields_no_findings() {
    let Some(path) = hbk_path() else {
        eprintln!("skip: hbk не найден");
        return;
    };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
Блокировка = Новый БлокировкаДанных;
ЭлементБлокировки = Блокировка.Добавить(\"РегистрСведений.Тест\");
Блокировка.Заблокировать();
";

    for level in [1u8, 2, 3] {
        assert!(
            type_member_findings(&index, src, None, level).is_empty(),
            "level={level}: ложный unknown_type_member на имени-тени"
        );
    }
}

#[test]
fn locking_pattern_types_have_those_members() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    // Настоящий тип переменной — `БлокировкаДанных`: методы есть.
    let real = index
        .find_type("БлокировкаДанных")
        .expect("тип БлокировкаДанных в справке");
    assert!(has_method(real, "Добавить"), "БлокировкаДанных.Добавить");
    assert!(
        has_method(real, "Заблокировать"),
        "БлокировкаДанных.Заблокировать"
    );

    // Тип, который подставлялся по совпадению имени, этих методов не имеет —
    // значит находка «нет члена Добавить» была следствием чужого типа.
    let by_name = index
        .find_type("Блокировка")
        .expect("тип Блокировка (IObjectLock) в справке");
    assert!(!has_method(by_name, "Добавить"));
    assert!(!has_method(by_name, "Заблокировать"));
}

// ── Воспроизведение 2 из issue: элемент отбора СКД ─────────────────────────

#[test]
fn issue11_dcs_filter_item_yields_no_findings() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
Процедура Тест()
	Настройки = Новый НастройкиКомпоновкиДанных;
	ЭлементОтбора = Настройки.Отбор.Элементы.Добавить(Тип(\"ЭлементОтбораКомпоновкиДанных\"));
	ЭлементОтбора.ЛевоеЗначение = Новый ПолеКомпоновкиДанных(\"Регион\");
	ЭлементОтбора.ВидСравнения = ВидСравненияКомпоновкиДанных.ВСписке;
	ЭлементОтбора.РежимОтображения = РежимОтображенияЭлементаНастройкиКомпоновкиДанных.Недоступный;
КонецПроцедуры
";

    for level in [1u8, 2, 3] {
        assert!(
            type_member_findings(&index, src, None, level).is_empty(),
            "level={level}: ложный unknown_type_member на `ЭлементОтбора`"
        );
    }
}

#[test]
fn dcs_filter_item_types_have_those_members() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let item = index
        .find_type("ЭлементОтбораКомпоновкиДанных")
        .expect("тип ЭлементОтбораКомпоновкиДанных");
    assert!(has_property(item, "ЛевоеЗначение"));
    assert!(has_property(item, "ВидСравнения"));
    assert!(has_property(item, "РежимОтображения"));

    // Обычный `ЭлементОтбора` — другой тип, и подсказка «Возможно: 'Значение'»
    // приходила именно из его состава.
    let other = index
        .find_type("ЭлементОтбора")
        .expect("тип ЭлементОтбора в справке");
    assert!(!has_property(other, "ЛевоеЗначение"));
}

// ── Имя-тень у параметра процедуры ─────────────────────────────────────────

#[test]
fn parameter_named_as_platform_type_yields_no_findings() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    // Тип параметра в BSL не объявляется: вывести его нечем. Подставлять
    // одноимённый платформенный тип нельзя — это ложная находка (issue #11,
    // «тот же эффект наблюдался … на параметрах функций»).
    let src = "\
Процедура Тест(Соединение)
	Соединение.Получить(\"/\", Файл);
КонецПроцедуры
";
    assert!(
        type_member_findings(&index, src, None, 3).is_empty(),
        "параметр с именем платформенного типа не должен давать находок"
    );
}

// ── Свойство контекста формы перекрывает одноимённый тип ───────────────────

#[test]
fn form_context_property_head_yields_no_findings() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    // `УсловноеОформление` — свойство `ФормаКлиентскогоПриложения` (и, отдельно,
    // имя платформенного типа с другим составом членов). В модуле формы
    // обращение идёт к свойству формы.
    let src = "\
&НаСервере
Процедура Тест()
	Элемент = УсловноеОформление.Элементы.Добавить();
КонецПроцедуры
";
    assert!(
        type_member_findings(&index, src, Some(FORM_MODULE), 3).is_empty(),
        "свойство контекста формы не должно проверяться по одноимённому типу"
    );
}

// ── Обратная сторона: настоящие находки остаются ───────────────────────────

#[test]
fn typo_in_variable_named_as_its_type_is_still_caught() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    // Переменная названа по своему типу — обычное дело в 1С. Тип берётся из
    // конструктора, поэтому опечатка в члене обязана ловиться на всех уровнях.
    let src = "\
Процедура Тест()
	Запрос = Новый Запрос;
	Запрос.Текстъ = \"ВЫБРАТЬ 1\";
КонецПроцедуры
";
    for level in [1u8, 2, 3] {
        let result = validate_module_with_profile(&index, src, None, None, level, Profile::Full);
        let err = result
            .errors
            .iter()
            .find(|e| e.kind == ExprErrorKind::UnknownTypeMember)
            .unwrap_or_else(|| {
                panic!(
                    "level={level}: опечатка 'Текстъ' должна ловиться: {:#?}",
                    result.errors
                )
            });
        assert_eq!(err.suggestion.as_deref(), Some("Текст"), "level={level}");
        assert_eq!(err.confidence, Confidence::Low);
    }
}

#[test]
fn typo_on_platform_type_head_is_still_caught() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    // Локально это имя не связано — значит обращение к самому типу,
    // и проверка членов работает как раньше.
    let src = "Процедура Тест()\n\tТаблицаЗначений.Колонкы.Добавить(\"Х\");\nКонецПроцедуры\n";
    let found = type_member_findings(&index, src, None, 1);
    assert_eq!(found.len(), 1, "ожидалась одна находка: {found:#?}");
    assert!(found[0].contains("Колонки"), "{found:#?}");
}

#[test]
fn global_manager_collection_head_is_not_flagged() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    // `Справочники` — свойство глобального контекста, а не переменная и не тип
    // вида объекта. Обращение к менеджеру коллекции законно и находок не даёт.
    let src = "\
Процедура Тест()
	Ссылка = Справочники.Контрагенты.ПустаяСсылка();
КонецПроцедуры
";
    assert!(
        all_findings(&index, src, None, 3).is_empty(),
        "законное обращение к менеджеру не должно давать находок"
    );
}

#[test]
fn renamed_variable_keeps_behaviour_of_issue_controls() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    // Контроль из issue: переименование переменной ничего не ломало и раньше.
    // Тест держит, что после правки результат тот же — находок нет.
    let src = "\
Процедура Тест()
	Блок = Новый БлокировкаДанных;
	ЭлементБлокировки = Блок.Добавить(\"РегистрСведений.Тест\");
	Блок.Заблокировать();
КонецПроцедуры
";
    for level in [1u8, 3] {
        assert!(
            all_findings(&index, src, None, level).is_empty(),
            "level={level}: контрольный пример обязан остаться чистым"
        );
    }
}

// ── Issue #15: остатки ложных unknown_type_member после 0.20.1 ─────────────

/// Класс 2: члены `COMОбъект` связываются поздно, состав из справки неизвестен.
#[test]
fn issue15_com_object_members_are_not_checked() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
Процедура Тест()
	Подключение = Новый COMОбъект(\"ADODB.Connection\");
	Подключение.Open(\"Provider=SQLOLEDB\");
КонецПроцедуры
";
    for level in [1u8, 2, 3] {
        let found = type_member_findings(&index, src, None, level);
        assert!(
            found.is_empty(),
            "level={level}: члены COM-объекта проверять нельзя: {found:#?}"
        );
    }
}

/// Класс 4: тип свойства составной — член проверяется по ОБЪЕДИНЕНИЮ альтернатив.
///
/// `ПараметрыВыполненияКоманды.Источник` — `ФормаКлиентскогоПриложения` ИЛИ
/// `ОкноКлиентскогоПриложения`; `ИмяФормы` есть только у первой, и проверка по
/// одной альтернативе давала ложную находку.
#[test]
fn issue15_composite_property_member_is_checked_across_alternatives() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
&НаКлиенте
Процедура ОбработкаКоманды(ПараметрКоманды, ПараметрыВыполненияКоманды)
	Форма = ПараметрыВыполненияКоманды.Источник;
	Имя = Форма.ИмяФормы;
КонецПроцедуры
";
    let found = type_member_findings(
        &index,
        src,
        Some("Catalogs/Тест/Commands/Открыть/Ext/CommandModule.bsl"),
        3,
    );
    assert!(
        found.is_empty(),
        "член есть у одной из альтернатив — находки быть не должно: {found:#?}"
    );
}

/// Класс 4, обратная сторона: если члена нет ни у одной альтернативы, находка
/// остаётся — иначе «объединение» превратилось бы в глушение проверки.
#[test]
fn issue15_composite_type_still_reports_unknown_member_for_all_alternatives() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
&НаКлиенте
Процедура ОбработкаКоманды(ПараметрКоманды, ПараметрыВыполненияКоманды)
	Форма = ПараметрыВыполненияКоманды.Источник;
	Значение = Форма.НетТакогоЧленаФормы;
КонецПроцедуры
";
    let found = type_member_findings(
        &index,
        src,
        Some("Catalogs/Тест/Commands/Открыть/Ext/CommandModule.bsl"),
        3,
    );
    assert_eq!(found.len(), 1, "ожидалась одна находка: {found:#?}");
}

/// Issue #18: значение перечисления, записанное в справке с латинской буквой,
/// не даёт находки на корректном коде.
#[test]
fn issue18_enum_value_with_help_homoglyph_is_not_a_finding() {
    let Some(path) = hbk_path() else { return };
    let index = load_from_hbk(&path).expect("PlatformIndex");

    let src = "\
&НаКлиенте
Процедура Тест()
	Режим = РежимОткрытияОкнаФормы.БлокироватьВесьИнтерфейс;
КонецПроцедуры
";
    for level in [1u8, 3] {
        assert!(
            all_findings(&index, src, None, level).is_empty(),
            "level={level}: корректное значение перечисления не должно давать находок"
        );
    }
}
