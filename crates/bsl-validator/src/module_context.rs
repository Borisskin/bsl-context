//! Тип контекста модуля: чей состав имён доступен неквалифицированному вызову
//! (issue #19).
//!
//! В модуле менеджера, объекта, набора записей и обычной формы неквалифицированное
//! имя разрешается методом САМОГО объекта модуля, а не глобальной функцией
//! платформы. Живой пример из issue: в модуле обычной формы
//! `ПолучитьФорму("Обработка.Х.Форма", Неопределено, Неопределено, Ложь,
//! Неопределено, Неопределено)` — это `ДокументОбъект.ПолучитьФорму(<Форма>,
//! <Владелец>, <КлючУникальности>)` с тремя параметрами, а не глобальная функция
//! с диапазоном 1..6. Сверка с глобальной сигнатурой давала ложную находку
//! `wrong_argument_count` (в модуле менеджера — то же самое, см. комментарий
//! автора issue про `ПолучитьДанныеВыбора`).
//!
//! Тип контекста выводится из ПУТИ модуля в выгрузке: `Catalogs/Х/Ext/
//! ManagerModule.bsl` → тип-менеджер `СправочникМенеджер.<Имя справочника>`,
//! `…/Ext/ObjectModule.bsl` → `СправочникОбъект.<Имя справочника>`,
//! `…/Ext/RecordSetModule.bsl` → `РегистрСведенийНаборЗаписей.<Имя регистра>`.
//! Для модуля формы контекст зависит от вида формы: у ОБЫЧНОЙ формы доступны
//! методы объекта-владельца, у управляемой — нет. Признак управляемой формы —
//! директивы компиляции (`&НаКлиенте`/`&НаСервере`), их отсутствие означает
//! обычную форму (`AstFacts::has_directives`).
//!
//! Всё, что не опознано, даёт `None` — проверка молчит, а не угадывает.

use platform_index::{PlatformIndex, Type};

/// Вид модуля — по имени файла в выгрузке.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleKind {
    /// `…/Ext/ManagerModule.bsl` — модуль менеджера объекта.
    Manager,
    /// `…/Ext/ObjectModule.bsl` — модуль объекта.
    Object,
    /// `…/Ext/RecordSetModule.bsl` — модуль набора записей регистра.
    RecordSet,
    /// `…/Ext/Form/Module.bsl` — модуль формы.
    Form,
    /// Всё остальное (общий модуль, модуль команды, модуль приложения): контекста
    /// объекта у таких модулей нет.
    Other,
}

/// Вид модуля по пути выгрузки.
pub fn module_kind(module_path: &str) -> ModuleKind {
    let path = module_path.replace('\\', "/").to_lowercase();
    if path.ends_with("/ext/managermodule.bsl") {
        ModuleKind::Manager
    } else if path.ends_with("/ext/objectmodule.bsl") {
        ModuleKind::Object
    } else if path.ends_with("/ext/recordsetmodule.bsl") {
        ModuleKind::RecordSet
    } else if path.ends_with("/ext/form/module.bsl") {
        ModuleKind::Form
    } else {
        ModuleKind::Other
    }
}

/// Префиксы шаблонных типов справки для модулей объекта и набора записей.
///
/// Первый столбец — папка выгрузки (`Catalogs`, `Documents`, …), второй — префикс
/// типа объекта, третий — префикс набора записей (у справочников и документов
/// набора записей нет). Префиксы взяты из самой справки: типы называются
/// `СправочникОбъект.<Имя справочника>`, `РегистрНакопленияНаборЗаписей.<Имя
/// регистра>`. Префиксы менеджеров берутся из `config_objects::MANAGER_COLLECTIONS`
/// — там же, где сверка `Коллекция.Объект.Метод(...)`, чтобы таблицы не разошлись.
const OBJECT_AND_RECORD_SET_PREFIXES: &[(&str, &str, Option<&str>)] = &[
    ("Catalogs", "СправочникОбъект", None),
    ("Documents", "ДокументОбъект", None),
    (
        "InformationRegisters",
        "",
        Some("РегистрСведенийНаборЗаписей"),
    ),
    (
        "AccumulationRegisters",
        "",
        Some("РегистрНакопленияНаборЗаписей"),
    ),
    (
        "AccountingRegisters",
        "",
        Some("РегистрБухгалтерииНаборЗаписей"),
    ),
    (
        "CalculationRegisters",
        "",
        Some("РегистрРасчетаНаборЗаписей"),
    ),
    (
        "ChartsOfCharacteristicTypes",
        "ПланВидовХарактеристикОбъект",
        None,
    ),
    ("ChartsOfAccounts", "ПланСчетовОбъект", None),
    ("ChartsOfCalculationTypes", "ПланВидовРасчетаОбъект", None),
    ("BusinessProcesses", "БизнесПроцессОбъект", None),
    ("Tasks", "ЗадачаОбъект", None),
    ("ExchangePlans", "ПланОбменаОбъект", None),
    ("DataProcessors", "ОбработкаОбъект", None),
    ("Reports", "ОтчетОбъект", None),
    ("Sequences", "", Some("ПоследовательностьНаборЗаписей")),
];

/// Папка выгрузки и имя объекта из пути модуля.
///
/// `Catalogs/ЕдиницыИзмерения/Ext/ManagerModule.bsl` → `("Catalogs",
/// "ЕдиницыИзмерения")`; для модуля формы имя берётся у ВЛАДЕЛЬЦА формы:
/// `Documents/Заказ/Forms/ФормаДокумента/Ext/Form/Module.bsl` → `("Documents",
/// "Заказ")`. `None` — путь не похож на модуль объекта конфигурации.
pub fn owner_of(module_path: &str) -> Option<(&str, &str)> {
    // Разделители — оба: путь из выгрузки приходит и с прямыми слэшами, и с
    // обратными (Windows). Промежуточную строку не создаём — возвращаем срезы
    // исходного пути.
    let parts: Vec<&str> = module_path
        .split(['/', '\\'])
        .filter(|p| !p.is_empty())
        .collect();
    let folder = *parts.first()?;
    // Папка верхнего уровня должна быть известна — иначе это не путь выгрузки
    // (модуль внешней обработки, произвольный текст), и угадывать нельзя.
    if !is_known_folder(folder) {
        return None;
    }
    let name = *parts.get(1)?;
    Some((folder, name))
}

/// Тип контекста модуля — из пути выгрузки и признака «в модуле есть директивы
/// компиляции».
///
/// `None` — контекст неизвестен: проверка обязана молчать, а не сверяться с
/// глобальной сигнатурой (иначе снова получим находку на корректном коде).
pub fn context_type<'a>(
    index: &'a PlatformIndex,
    module_path: &str,
    has_directives: bool,
) -> Option<&'a Type> {
    let kind = module_kind(module_path);
    let (folder, _name) = owner_of(module_path)?;
    let prefix = match kind {
        ModuleKind::Manager => manager_prefix_of(folder),
        ModuleKind::Object => object_prefix_of(folder),
        ModuleKind::RecordSet => record_set_prefix_of(folder),
        // У обычной формы доступны методы объекта-владельца, у управляемой — нет:
        // её контекст — сама форма, а она в списке динамических типов.
        ModuleKind::Form => {
            if has_directives {
                return None;
            }
            object_prefix_of(folder)
        }
        ModuleKind::Other => None,
    }?;
    template_type(index, prefix)
}

/// Папка выгрузки → префикс типа-менеджера. Таблица живёт в
/// `config_objects::MANAGER_COLLECTIONS` (там же сверка `Коллекция.Объект.Метод`),
/// чтобы не держать вторую копию и не развести их со временем.
fn manager_prefix_of(folder: &str) -> Option<&'static str> {
    crate::config_objects::MANAGER_COLLECTIONS
        .iter()
        .find(|(_, f, _)| f.eq_ignore_ascii_case(folder))
        .map(|(_, _, prefix)| *prefix)
}

/// Префикс типа объекта (или набора записей) по папке выгрузки.
fn object_prefix_of(folder: &str) -> Option<&'static str> {
    OBJECT_AND_RECORD_SET_PREFIXES
        .iter()
        .find(|(f, _, _)| f.eq_ignore_ascii_case(folder))
        .map(|(_, prefix, _)| *prefix)
        .filter(|p| !p.is_empty())
}

/// Префикс типа набора записей по папке выгрузки.
fn record_set_prefix_of(folder: &str) -> Option<&'static str> {
    OBJECT_AND_RECORD_SET_PREFIXES
        .iter()
        .find(|(f, _, _)| f.eq_ignore_ascii_case(folder))
        .and_then(|(_, _, prefix)| *prefix)
}

/// Папка верхнего уровня известна? Иначе путь не из выгрузки, и угадывать нельзя.
fn is_known_folder(folder: &str) -> bool {
    manager_prefix_of(folder).is_some()
        || OBJECT_AND_RECORD_SET_PREFIXES
            .iter()
            .any(|(f, _, _)| f.eq_ignore_ascii_case(folder))
}

/// Шаблонный тип справки по префиксу: `СправочникМенеджер` →
/// `СправочникМенеджер.<Имя справочника>`. В справке такой шаблон на каждый вид
/// ровно один — берём первый по префиксу.
fn template_type<'a>(index: &'a PlatformIndex, prefix: &str) -> Option<&'a Type> {
    let needle = format!("{}.", prefix.to_lowercase());
    index
        .types
        .iter()
        .find(|(key, _)| key.starts_with(&needle))
        .map(|(_, ty)| ty)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_kind_by_path() {
        assert_eq!(
            module_kind("Catalogs/ЕдиницыИзмерения/Ext/ManagerModule.bsl"),
            ModuleKind::Manager
        );
        assert_eq!(
            module_kind("Documents\\Заказ\\Ext\\ObjectModule.bsl"),
            ModuleKind::Object
        );
        assert_eq!(
            module_kind("InformationRegisters/Цены/Ext/RecordSetModule.bsl"),
            ModuleKind::RecordSet
        );
        assert_eq!(
            module_kind("Documents/Заказ/Forms/ФормаДокумента/Ext/Form/Module.bsl"),
            ModuleKind::Form
        );
        assert_eq!(
            module_kind("CommonModules/ОбщегоНазначения/Ext/Module.bsl"),
            ModuleKind::Other
        );
    }

    #[test]
    fn owner_is_taken_from_dump_path() {
        assert_eq!(
            owner_of("Catalogs/ЕдиницыИзмерения/Ext/ManagerModule.bsl"),
            Some(("Catalogs", "ЕдиницыИзмерения"))
        );
        assert_eq!(
            owner_of("Documents/Заказ/Forms/ФормаДокумента/Ext/Form/Module.bsl"),
            Some(("Documents", "Заказ"))
        );
        // Не путь выгрузки — не угадываем.
        assert_eq!(owner_of("Модуль.bsl"), None);
        assert_eq!(owner_of("SomeFolder/Х/Ext/ObjectModule.bsl"), None);
    }
}
