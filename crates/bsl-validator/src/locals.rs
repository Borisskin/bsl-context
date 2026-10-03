//! Локальные имена модуля: параметры процедур, объявления `Перем`, присваивания
//! и тип, выведенный прямо из конструктора.
//!
//! Нужны проверке `Идентификатор.Член`: она считает голову обращения именем
//! ПЛАТФОРМЕННОГО ТИПА, если такое имя есть в справке. Но имя переменной вполне
//! может совпасть с именем типа — и тогда проверка ругается на члены чужого
//! типа. Живые примеры (issue #11; модули типовой конфигурации и код,
//! сгенерированный моделью):
//!
//! ```bsl
//! Блокировка = Новый БлокировкаДанных;
//! ЭлементБлокировки = Блокировка.Добавить("РегистрСведений.Тест"); // ложная находка
//!
//! ЭлементОтбора = Список.Отбор.Элементы.Добавить(Тип("ЭлементОтбораКомпоновкиДанных"));
//! ЭлементОтбора.ЛевоеЗначение = Новый ПолеКомпоновкиДанных("Статус"); // ложная находка
//! ```
//!
//! `ЭлементОтбора` и `Блокировка` — имена платформенных типов, но здесь это
//! ПЕРЕМЕННЫЕ других типов, и находки «у типа ЭлементОтбора нет члена
//! ЛевоеЗначение» / «у типа Блокировка нет члена Добавить» ложные. Хуже пропуска:
//! `suggestion` предлагает член чужого типа, и правка по подсказке ломает
//! рабочий код.
//!
//! Правило простое: если имя в этой процедуре связано локально (параметр,
//! `Перем`, присваивание) — это переменная, а не тип. Тип для неё берётся из
//! конструктора (`Запрос = Новый Запрос`) либо из вывода типов (`ScopeMap`,
//! уровень ≥ 2), а если типа нет — проверка молчит.

use bsl_parse::AstFacts;

pub(crate) struct LocalNames<'a> {
    facts: &'a AstFacts,
}

impl<'a> LocalNames<'a> {
    pub(crate) fn new(facts: &'a AstFacts) -> Self {
        Self { facts }
    }

    /// Тип локальной переменной, если он задан конструктором: `Запрос = Новый Запрос;`.
    ///
    /// Это не вывод типов, а факт из текста: справа стоит `Новый ТипX`. Нужно,
    /// чтобы не потерять проверку членов у переменных, названных по своему типу
    /// (`Запрос`, `Массив`, `Структура` — обычное дело в 1С): их члены проверяются
    /// по типу из конструктора, а не по совпадению имени с типом.
    ///
    /// Берётся БЛИЖАЙШЕЕ присваивание ВЫШЕ точки, а не первое в процедуре
    /// (issue #15, класс 1: после переприсваивания тип не менялся). Если ближайшее
    /// присваивание — не конструктор, тип неизвестен (`None`), и решение принимает
    /// вызывающий код. Если присваивание сделано внутри ветви условного оператора,
    /// а спрашиваем мы после него, возвращаются типы ВСЕХ ветвей: переменная может
    /// иметь любой из них. Если в соседней ветви присваивание без конструктора, тип
    /// неизвестен — `None`, потому что молчание лучше ложной находки.
    ///
    /// Учитывается только присваивание в ТОЙ ЖЕ процедуре (для кода вне процедур —
    /// на уровне модуля): переменные BSL локальны для процедуры, и тип из чужой
    /// процедуры к этой точке отношения не имеет.
    pub(crate) fn constructed_type(&self, byte: usize, name: &str) -> Option<Vec<String>> {
        let name_lower = name.to_lowercase();
        let scope = self.facts.procs.iter().find(|p| p.contains(byte));
        let in_scope = |site: usize| match scope {
            Some(s) => s.contains(site),
            None => !self.facts.procs.iter().any(|p| p.contains(site)),
        };
        let nearest = self
            .facts
            .assigns
            .iter()
            .filter(|a| a.name.to_lowercase() == name_lower && a.byte <= byte && in_scope(a.byte))
            .max_by_key(|a| a.byte)?;
        let mut types: Vec<String> = vec![nearest.new_type.as_deref()?.to_string()];

        // Ветви того же условного оператора, уже закрытого к этой точке.
        let block = self
            .facts
            .if_branches
            .iter()
            .filter(|b| b.span.1 <= byte)
            .filter(|b| {
                b.branches
                    .iter()
                    .any(|(s, e)| *s <= nearest.byte && nearest.byte < *e)
            })
            .min_by_key(|b| b.span.1.saturating_sub(b.span.0));
        if let Some(block) = block {
            for a in self.facts.assigns.iter().filter(|a| {
                a.name.to_lowercase() == name_lower && a.byte <= byte && in_scope(a.byte)
            }) {
                if !block
                    .branches
                    .iter()
                    .any(|(s, e)| *s <= a.byte && a.byte < *e)
                {
                    continue;
                }
                // В соседней ветви тип не из конструктора — не угадываем.
                let t = a.new_type.as_deref()?;
                if !types.iter().any(|x| x.eq_ignore_ascii_case(t)) {
                    types.push(t.to_string());
                }
            }
        }
        Some(types)
    }

    /// Имя в этой точке — локальная переменная, а не имя типа?
    ///
    /// Да, если это параметр объемлющей процедуры, либо имя ей присваивается
    /// (или объявлено `Перем`) в той же процедуре, либо объявлено `Перем` на
    /// уровне модуля — такие переменные видны во всех процедурах. В теле модуля
    /// (код вне процедур) имя связывает и обычное присваивание: `Перем` там для
    /// этого не обязателен, а местная переменная тела модуля процедурам не видна.
    pub(crate) fn is_local(&self, byte: usize, name: &str) -> bool {
        let name_lower = name.to_lowercase();
        let scope = self.facts.procs.iter().find(|p| p.contains(byte));
        // Видно ли имя из этой точки: связывание в той же процедуре, либо в теле
        // модуля, если и спрашиваем из тела модуля.
        let visible = |site_byte: usize| match scope {
            Some(s) => s.contains(site_byte),
            None => !self.facts.procs.iter().any(|p| p.contains(site_byte)),
        };

        // Переменная уровня модуля видна везде.
        let module_var = self.facts.assigns.iter().any(|a| {
            a.declaration
                && a.name.to_lowercase() == name_lower
                && !self.facts.procs.iter().any(|p| p.contains(a.byte))
        });
        if module_var {
            return true;
        }

        // Переменная цикла: цикл связывает имя, не порождая присваивания в дереве,
        // поэтому в `assigns` таких имён нет. Область видимости учитываем: имя,
        // связанное циклом в ОДНОЙ процедуре, не должно глушить проверки в другой
        // — иначе модуль с `Для Каждого Поле Из СписокПолей Цикл` теряет проверку
        // члена у ТИПА `Поле` во всех остальных процедурах (замер по выгрузке:
        // 2633 находки на 14943 модулях). Сам случай, ради которого правило
        // нужно (issue #22): `Для Каждого ГруппировкаКолонок Из Список Цикл` —
        // имя совпало с типом-перечислением, и `ГруппировкаКолонок.Значение`
        // получило `unknown_enum_value` с `confidence: high`.
        if self
            .facts
            .loop_var_sites
            .iter()
            .any(|site| site.name == name_lower && visible(site.byte))
        {
            return true;
        }

        let Some(scope) = scope else {
            // Код вне процедур: локальны объявления `Перем` (проверены выше) и
            // присваивания в самом теле модуля.
            return self.facts.assigns.iter().any(|a| {
                a.name.to_lowercase() == name_lower
                    && !self.facts.procs.iter().any(|p| p.contains(a.byte))
            });
        };
        if scope.params.contains(&name_lower) {
            return true;
        }
        self.facts
            .assigns
            .iter()
            .any(|a| scope.contains(a.byte) && a.name.to_lowercase() == name_lower)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bsl_parse::collect_facts;

    /// Байт первого вхождения подстроки — точка, в которой спрашиваем про имя.
    fn byte_of(src: &str, needle: &str) -> usize {
        src.find(needle).expect("подстрока есть в тексте")
    }

    #[test]
    fn assigned_name_is_local() {
        let src = "\
&НаСервере
Процедура Т()
ЭлементОтбора = Новый Структура;
ЭлементОтбора.Поле = 1;
КонецПроцедуры
";
        let facts = collect_facts(src);
        let locals = LocalNames::new(&facts);
        assert!(locals.is_local(byte_of(src, "ЭлементОтбора.Поле"), "ЭлементОтбора"));
    }

    #[test]
    fn parameter_is_local() {
        let src = "\
&НаСервере
Процедура Т(Отбор)
Отбор.Вставить(\"А\", 1);
КонецПроцедуры
";
        let facts = collect_facts(src);
        let locals = LocalNames::new(&facts);
        assert!(locals.is_local(byte_of(src, "Отбор.Вставить"), "Отбор"));
    }

    #[test]
    fn module_var_is_local_inside_any_procedure() {
        let src = "\
Перем Отбор;

Процедура Т()
Отбор.Вставить(\"А\", 1);
КонецПроцедуры
";
        let facts = collect_facts(src);
        let locals = LocalNames::new(&facts);
        assert!(locals.is_local(byte_of(src, "Отбор.Вставить"), "Отбор"));
    }

    #[test]
    fn name_from_other_procedure_is_not_local() {
        let src = "\
Процедура Первая()
Отбор = Новый Структура;
КонецПроцедуры

Процедура Вторая()
Отбор.Вставить(\"А\", 1);
КонецПроцедуры
";
        let facts = collect_facts(src);
        let locals = LocalNames::new(&facts);
        assert!(!locals.is_local(byte_of(src, "Отбор.Вставить"), "Отбор"));
    }

    #[test]
    fn assignment_in_module_body_is_local_for_module_body() {
        // Код вне процедур: `Перем` не обязателен, присваивание связывает имя
        // здесь же. Воспроизведение 1 из issue #11 записано именно так.
        let src = "\
Блокировка = Новый БлокировкаДанных;
Блокировка.Заблокировать();
";
        let facts = collect_facts(src);
        let locals = LocalNames::new(&facts);
        let byte = byte_of(src, "Блокировка.Заблокировать");
        assert!(locals.is_local(byte, "Блокировка"));
        assert_eq!(
            locals.constructed_type(byte, "Блокировка"),
            Some(vec!["БлокировкаДанных".to_string()])
        );
    }

    #[test]
    fn module_body_assignment_does_not_leak_into_procedure() {
        // Присваивание в теле модуля процедуре не видно: там своя область.
        let src = "\
Блокировка = Новый БлокировкаДанных;

Процедура Т()
Блокировка.Заблокировать();
КонецПроцедуры
";
        let facts = collect_facts(src);
        let locals = LocalNames::new(&facts);
        let byte = byte_of(src, "Блокировка.Заблокировать");
        assert!(!locals.is_local(byte, "Блокировка"));
    }

    #[test]
    fn constructed_type_is_taken_from_new_expression() {
        // Переменная названа по своему типу — обычное дело в 1С. Тип берём из
        // конструктора, поэтому её члены по-прежнему проверяются.
        let src = "\
&НаСервере
Процедура Т()
Запрос = Новый Запрос;
Запрос.Текст = \"ВЫБРАТЬ 1\";
КонецПроцедуры
";
        let facts = collect_facts(src);
        let locals = LocalNames::new(&facts);
        let byte = byte_of(src, "Запрос.Текст");
        assert!(locals.is_local(byte, "Запрос"));
        assert_eq!(
            locals.constructed_type(byte, "Запрос"),
            Some(vec!["Запрос".to_string()])
        );
    }

    #[test]
    fn constructor_type_of_other_procedure_is_not_used() {
        // Тип из чужой процедуры к этой точке отношения не имеет.
        let src = "\
Процедура Первая()
Запрос = Новый Запрос;
КонецПроцедуры

Процедура Вторая()
Запрос.Текст = \"ВЫБРАТЬ 1\";
КонецПроцедуры
";
        let facts = collect_facts(src);
        let locals = LocalNames::new(&facts);
        let byte = byte_of(src, "Запрос.Текст");
        assert!(!locals.is_local(byte, "Запрос"));
        assert_eq!(locals.constructed_type(byte, "Запрос"), None);
    }

    #[test]
    fn no_constructor_means_unknown_type() {
        // Справа не конструктор — тип неизвестен, проверять члены не по чему.
        let src = "\
&НаСервере
Процедура Т()
ЭлементОтбора = Список.Отбор.Элементы.Добавить();
ЭлементОтбора.ЛевоеЗначение = 1;
КонецПроцедуры
";
        let facts = collect_facts(src);
        let locals = LocalNames::new(&facts);
        let byte = byte_of(src, "ЭлементОтбора.ЛевоеЗначение");
        assert!(locals.is_local(byte, "ЭлементОтбора"));
        assert_eq!(locals.constructed_type(byte, "ЭлементОтбора"), None);
    }

    #[test]
    fn platform_type_name_without_assignment_is_not_local() {
        // `Справочники.Контрагенты` — обращение к глобальному свойству, не к переменной.
        let src = "\
Процедура Т()
Ссылка = Справочники.Контрагенты.ПустаяСсылка();
КонецПроцедуры
";
        let facts = collect_facts(src);
        let locals = LocalNames::new(&facts);
        assert!(!locals.is_local(byte_of(src, "Справочники.Контрагенты"), "Справочники"));
    }

    /// Issue #22: переменная цикла, названная как системное перечисление.
    ///
    /// `ГруппировкаКолонок` есть в справке как тип-перечисление, но здесь это
    /// переменная цикла — проверку значений перечисления вести нельзя, иначе
    /// корректный код получает `unknown_enum_value` с `confidence: high`.
    #[test]
    fn loop_variable_is_local() {
        let src = "\
&НаСервере
Процедура Т(Список)
Для Каждого ГруппировкаКолонок Из Список Цикл
Х = ГруппировкаКолонок.Значение;
КонецЦикла;
КонецПроцедуры
";
        let facts = collect_facts(src);
        let locals = LocalNames::new(&facts);
        assert!(
            locals.is_local(
                byte_of(src, "ГруппировкаКолонок.Значение"),
                "ГруппировкаКолонок"
            ),
            "переменная цикла обязана считаться локальным именем"
        );
    }

    /// Переменная цикла видна и в теле модуля (код вне процедур).
    #[test]
    fn loop_variable_in_module_body_is_local() {
        let src = "\
Для Каждого Значение Из Список Цикл
Значение.Свойство = 1;
КонецЦикла;
";
        let facts = collect_facts(src);
        let locals = LocalNames::new(&facts);
        assert!(locals.is_local(byte_of(src, "Значение.Свойство"), "Значение"));
    }

    /// Область видимости переменной цикла учитывается: имя, связанное циклом в
    /// ОДНОЙ процедуре, не глушит проверки в другой.
    ///
    /// На замере по выгрузке (14943 модуля) модуль-широкое правило стоило 2633
    /// находок: модуль с `Для Каждого Поле Из СписокПолей Цикл` терял проверку
    /// члена у ТИПА `Поле` во всех остальных процедурах, хотя внутри цикла
    /// `Поле.Ключ` — законное обращение к элементу коллекции.
    #[test]
    fn loop_variable_does_not_leak_into_another_procedure() {
        let src = "\
Процедура Первая(Список)
Для Каждого Поле Из Список Цикл
КонецЦикла;
КонецПроцедуры

Процедура Вторая()
Поле.Ключ = 1;
КонецПроцедуры
";
        let facts = collect_facts(src);
        let locals = LocalNames::new(&facts);
        assert!(
            !locals.is_local(byte_of(src, "Поле.Ключ"), "Поле"),
            "имя, связанное циклом в чужой процедуре, локальным не считается"
        );
    }
}
