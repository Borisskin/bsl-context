//! Структурные проверки уровня языка, которые не видит разбор дерева:
//! баланс блоков и обращение к члену у ВЫРАЖЕНИЯ (issue #17).
//!
//! Повод — ложные пропуски. `tree-sitter-bsl` восстанавливается после ошибки и
//! отдаёт дерево с узлами `ERROR`, поэтому `validate_module` на коде
//!
//! ```bsl
//! Если Ложь Тогда
//!     Попытка
//!         А = 1;
//!     Исключение
//!         А = 2;
//!     КонецЕсли;
//! КонецЕсли;
//! ```
//!
//! отвечал `valid: true, tree_parsed: true`, хотя платформа такой модуль не
//! компилирует («Ожидается КонецПопытки»). Второй случай — обращение к члену у
//! результата `Новый X(...)` или у группирующих скобок:
//! `Новый Файл("x").Расширение`, `(Новый Файл("x")).Размер()` — платформа тоже
//! отвергает, а мы молчали. При этом `Запрос.Выполнить().Выбрать()` ЗАКОННО:
//! после вызова обращение к члену разрешено, после группирующих скобок — нет.
//!
//! Обе проверки текстовые: дерево на таком коде повреждено, опираться на него
//! нельзя. Текст подаётся уже замаскированным (`mask_strings_and_comments`),
//! поэтому строки и комментарии проверкам не мешают.
//!
//! Всё чтение текста — ПО СИМВОЛАМ, а не по байтам: в кириллице байт продолжения
//! (`0x80..=0xBF`) при `as char` даёт `U+0085` (NEL), который `char::is_whitespace`
//! считает пробелом, и побайтовый пропуск пробелов разрезал букву пополам —
//! корпусный замер падал с «not a char boundary» на модулях с кириллицей.

use crate::expression::{pos_at, Confidence, ExprError, ExprErrorKind};

/// Проверить баланс блоков `Попытка`/`Если`/`Цикл`.
///
/// Директивы препроцессора пропускаются целиком: `#Если` — не блок языка, и
/// `#КонецЕсли` не закрывает `Если`. Строки этих директив затираются пробелами
/// заранее ([`blank_directive_lines`]), поэтому в скан они не попадают.
///
/// Находка — одна на процедуру: после первой ошибки баланс в этом блоке уже
/// неизвестен, и продолжать значит сыпать каскадом.
pub fn check_block_balance(source: &str, cleaned: &str, errors: &mut Vec<ExprError>) {
    let text = blank_directive_lines(cleaned);
    let mut stack: Vec<(&'static str, usize)> = Vec::new();
    let mut reported = false;
    let mut i = 0usize;

    while i < text.len() {
        // Заголовок процедуры/функции и её конец — границы блока: незакрытые
        // блоки внутри процедуры дальше не переносятся.
        if let Some((word, end)) = word_at(&text, i) {
            let lower = word.to_lowercase();
            // Слово после точки — ИМЯ свойства или метода, а не ключевое слово:
            // в выгрузке есть `Параметры.КонецЦикла = ВершинаВыхода` и
            // `СтрокаЗадачи.Цикл = ЗадачаXDTO.iterationNumber`. Без этой проверки
            // баланс ломается на первом же таком модуле (на замере — три находки,
            // все ложные).
            if preceded_by_dot(&text, i) {
                i = end;
                continue;
            }
            // Границы процедуры отличаем от СВОЙСТВ с тем же именем: в выгрузке
            // есть `Обработчик.Процедура = "Модуль.Метод"` (поле структуры
            // обработчика обновления), и если считать это заголовком, стек блоков
            // сбрасывается посреди процедуры — на замере это давало 472 ложные
            // находки `UnbalancedCodeBlock` в 75 модулях.
            if is_procedure_boundary(&text, i, &lower) {
                if !stack.is_empty() && !reported {
                    report_unclosed(source, &stack, errors);
                }
                stack.clear();
                reported = false;
                i = end;
                continue;
            }
            match lower.as_str() {
                "попытка" => stack.push(("КонецПопытки", i)),
                "если" => stack.push(("КонецЕсли", i)),
                "цикл" => stack.push(("КонецЦикла", i)),
                "конецпопытки" | "конецесли" | "конеццикла" => {
                    let expected = match lower.as_str() {
                        "конецпопытки" => "КонецПопытки",
                        "конецесли" => "КонецЕсли",
                        _ => "КонецЦикла",
                    };
                    let ok = stack.last().is_some_and(|(kind, _)| *kind == expected);
                    if !ok && !reported {
                        let (line, col) = pos_at(source, i);
                        // Внутри стека назовём то, что реально открыто: это и есть
                        // подсказка «где искать пропущенное закрытие».
                        let open = stack
                            .last()
                            .map(|(kind, _)| format!("Сейчас открыт блок, ожидается «{kind}»."))
                            .unwrap_or_else(|| "Открытых блоков нет.".to_string());
                        errors.push(ExprError::new_with_confidence(
                            line,
                            col,
                            ExprErrorKind::UnbalancedCodeBlock,
                            format!(
                                "Ожидается «{expected}»: блок закрыт не тем ключевым словом. \
                                 Модуль не компилируется. {open}"
                            ),
                            Confidence::High,
                            None,
                            Vec::new(),
                        ));
                        reported = true;
                    }
                    if ok {
                        stack.pop();
                    }
                }
                _ => {}
            }
            i = end;
            continue;
        }
        i += next_char_len(&text, i);
    }

    if !stack.is_empty() && !reported {
        report_unclosed(source, &stack, errors);
    }
}

/// Сообщить о незакрытом блоке (по одному сообщению — о первом).
fn report_unclosed(source: &str, stack: &[(&'static str, usize)], errors: &mut Vec<ExprError>) {
    let Some((kind, byte)) = stack.first() else {
        return;
    };
    let (line, col) = pos_at(source, *byte);
    errors.push(ExprError::new_with_confidence(
        line,
        col,
        ExprErrorKind::UnbalancedCodeBlock,
        format!("Блок не закрыт: ожидается «{kind}». Модуль не компилируется."),
        Confidence::High,
        None,
        Vec::new(),
    ));
}

/// Проверить обращение к члену у ВЫРАЖЕНИЯ: `Новый X(...).Член` и `(…).Член`.
///
/// После ВЫЗОВА (`Запрос.Выполнить().Выбрать()`) обращение к члену законно, после
/// конструктора и группирующих скобок — нет: платформа отвечает ошибкой
/// («Ожидается конец выражения»). Именно это различие и проверяется.
pub fn check_member_access_on_expression(source: &str, cleaned: &str, errors: &mut Vec<ExprError>) {
    let text = blank_directive_lines(cleaned);
    // Стек открытых скобок: (позиция, вид).
    let mut stack: Vec<(usize, ParenKind)> = Vec::new();
    let mut i = 0usize;
    let mut in_string = false;

    while i < text.len() {
        let c = text[i..].chars().next().expect("граница символа");
        if c == '"' {
            in_string = !in_string;
            i += c.len_utf8();
            continue;
        }
        if in_string {
            i += c.len_utf8();
            continue;
        }
        match c {
            '(' => stack.push((i, paren_kind(&text, i))),
            ')' => {
                if let Some((open, kind)) = stack.pop() {
                    // Что стоит сразу после закрывающей скобки?
                    let mut j = i + 1;
                    while j < text.len() {
                        let next = text[j..].chars().next().expect("граница символа");
                        if !next.is_whitespace() {
                            break;
                        }
                        j += next.len_utf8();
                    }
                    if text[j..].starts_with('.') {
                        match kind {
                            // Вызов — законно: `Запрос.Выполнить().Выбрать()`.
                            ParenKind::Call => {}
                            ParenKind::Constructor | ParenKind::Grouping => {
                                let (line, col) = pos_at(source, open);
                                errors.push(ExprError::new_with_confidence(
                                    line,
                                    col,
                                    ExprErrorKind::MemberAccessOnExpression,
                                    "Обращение к члену у выражения: платформа разрешает его \
                                     только после ВЫЗОВА метода. После конструктора \
                                     («Новый X(...)») и группирующих скобок нужно сначала \
                                     присвоить значение переменной."
                                        .to_string(),
                                    Confidence::High,
                                    None,
                                    Vec::new(),
                                ));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        i += c.len_utf8();
    }
}

/// Слово стоит сразу после точки (`Объект.Слово`) — значит это имя свойства или
/// метода, а не ключевое слово языка.
fn preceded_by_dot(text: &str, i: usize) -> bool {
    let mut j = i;
    while j > 0 {
        let prev = text[..j].chars().next_back().expect("граница символа");
        if !prev.is_whitespace() {
            return prev == '.';
        }
        j -= prev.len_utf8();
    }
    false
}

/// Слово открывает или закрывает блок процедуры?
///
/// Одного совпадения слова мало: `Обработчик.Процедура` — поле структуры, а не
/// заголовок процедуры, и на выгрузке УТ такая подмена давала 472 ложные находки
/// `UnbalancedCodeBlock` в 75 модулях (слово встречалось посреди процедуры и
/// сбрасывало стек блоков). Поэтому смотрим на соседа слева: точка означает
/// обращение к свойству.
fn is_procedure_boundary(text: &str, i: usize, lower: &str) -> bool {
    let is_open = matches!(lower, "процедура" | "функция");
    let is_close = matches!(lower, "конецпроцедуры" | "конецфункции");
    if !is_open && !is_close {
        return false;
    }
    // Слева: точка (свойство) или буква (часть другого слова) — не граница.
    let mut j = i;
    while j > 0 {
        let prev = text[..j].chars().next_back().expect("граница символа");
        if !prev.is_whitespace() {
            if prev == '.' {
                return false;
            }
            break;
        }
        j -= prev.len_utf8();
    }
    if is_close {
        return true;
    }
    // Открывающий: дальше идёт ИМЯ процедуры, а не `=` (присваивание переменной
    // с именем `Процедура`) и не `.` (обращение к свойству).
    let mut k = i + lower.len();
    while k < text.len() {
        let next = text[k..].chars().next().expect("граница символа");
        if !next.is_whitespace() {
            return next != '=' && next != '.' && (next.is_alphanumeric() || next == '_');
        }
        k += next.len_utf8();
    }
    false
}

/// Вид открывающей скобки — по тому, что стоит СЛЕВА от неё.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParenKind {
    /// Вызов метода/функции: `Метод(`, `Объект.Метод(`, `Ф(1)(`.
    Call,
    /// Конструктор: `Новый ТипX(`.
    Constructor,
    /// Группирующие скобки: `(выражение)`.
    Grouping,
}

fn paren_kind(text: &str, open: usize) -> ParenKind {
    // Пропускаем пробелы влево — по символам, иначе байт продолжения кириллицы
    // (`0x85`) читается как NEL и считается пробелом.
    let mut j = open;
    while j > 0 {
        let prev = text[..j].chars().next_back().expect("граница символа");
        if !prev.is_whitespace() {
            break;
        }
        j -= prev.len_utf8();
    }
    if j == 0 {
        return ParenKind::Grouping;
    }
    let prev = text[..j].chars().next_back().expect("граница символа");
    // Тернарный оператор записан как вызов `?(Усл, А, Б)`, и обращение к члену у
    // его результата платформа ПРИНИМАЕТ: в выгрузке УТ есть
    // `?(ОписаниеПодсистемы.Родитель = Неопределено, ДеревоРолей.Строки,
    // ОписаниеПодсистемы.Родитель.Строки).Индекс(ОписаниеПодсистемы)`. Поэтому
    // `?(` — вызов, а не группирующие скобки (иначе 8 ложных находок на выгрузке).
    if prev == '?' {
        return ParenKind::Call;
    }
    // Перед вызовом стоит буква, цифра, `_`, закрывающая скобка или кавычка.
    let call_like = prev.is_alphanumeric() || prev == '_' || prev == ')' || prev == ']';
    if !call_like {
        return ParenKind::Grouping;
    }
    // Перед именем стоит `Новый`? Тогда это конструктор.
    let word_end = word_start(text, j);
    let mut k = word_end;
    while k > 0 {
        let prev = text[..k].chars().next_back().expect("граница символа");
        if !prev.is_whitespace() {
            break;
        }
        k -= prev.len_utf8();
    }
    if k > 0 {
        let prev_word_end = k;
        let prev_word_start = word_start(text, prev_word_end);
        if prev_word_start < prev_word_end {
            let word = text[prev_word_start..prev_word_end].to_lowercase();
            if word == "новый" || word == "new" {
                return ParenKind::Constructor;
            }
        }
    }
    ParenKind::Call
}

/// Начало слова, заканчивающегося на `end` (байт за последним символом слова).
fn word_start(text: &str, end: usize) -> usize {
    let mut start = end;
    while start > 0 {
        let c = text[..start].chars().next_back().expect("граница символа");
        if !(c.is_alphanumeric() || c == '_') {
            break;
        }
        start -= c.len_utf8();
    }
    start
}

/// Слово, начинающееся в позиции `i`, и байт за его концом.
fn word_at(text: &str, i: usize) -> Option<(&str, usize)> {
    let first = text[i..].chars().next()?;
    if !(first.is_alphanumeric() || first == '_') {
        return None;
    }
    let mut j = i;
    while j < text.len() {
        let c = text[j..].chars().next().expect("граница символа");
        if !(c.is_alphanumeric() || c == '_') {
            break;
        }
        j += c.len_utf8();
    }
    Some((&text[i..j], j))
}

/// Длина символа, начинающегося в позиции `i` (для посимвольного шага).
fn next_char_len(text: &str, i: usize) -> usize {
    text[i..].chars().next().map_or(1, |c| c.len_utf8())
}

/// Затереть пробелами строки директив препроцессора (`#Если`, `#КонецЕсли`,
/// `#Область`, `#Удаление`, …), сохранив длину текста.
///
/// Директива — не блок языка: `#КонецЕсли` не закрывает `Если`, а `#Если` не
/// открывает его. Без этого правила проверка баланса ругалась бы на каждом
/// `#Если … #КонецЕсли` в модуле.
fn blank_directive_lines(cleaned: &str) -> String {
    let mut out = cleaned.as_bytes().to_vec();
    let mut line_start = 0usize;
    for (idx, byte) in cleaned.bytes().enumerate() {
        if byte != b'\n' {
            continue;
        }
        blank_if_directive(&mut out, cleaned, line_start, idx);
        line_start = idx + 1;
    }
    blank_if_directive(&mut out, cleaned, line_start, cleaned.len());
    String::from_utf8(out).expect("замена байтов пробелами сохраняет UTF-8")
}

fn blank_if_directive(out: &mut [u8], text: &str, from: usize, to: usize) {
    let line = &text[from..to];
    let trimmed = line.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{FEFF}');
    if !trimmed.starts_with('#') {
        return;
    }
    for b in out.iter_mut().take(to).skip(from) {
        *b = b' ';
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn findings(src: &str) -> Vec<(ExprErrorKind, String)> {
        let cleaned = bsl_parse::mask_strings_and_comments(src);
        let mut errors = Vec::new();
        check_block_balance(src, &cleaned, &mut errors);
        check_member_access_on_expression(src, &cleaned, &mut errors);
        errors.into_iter().map(|e| (e.kind, e.message)).collect()
    }

    #[test]
    fn balanced_blocks_are_clean() {
        let src = "\
Процедура Тест()
	Попытка
		Если Истина Тогда
			Для Каждого Стр Из Массив Цикл
				Пока Истина Цикл
					Прервать;
				КонецЦикла;
			КонецЦикла;
		Иначе
			ВызватьИсключение;
		КонецЕсли;
	Исключение
		Попытка
		Исключение
		КонецПопытки;
	КонецПопытки;
КонецПроцедуры
";
        assert!(findings(src).is_empty(), "{:#?}", findings(src));
    }

    /// Issue #17: `Попытка`, закрытая `КонецЕсли`.
    #[test]
    fn attempt_closed_by_endif_is_reported() {
        let src = "\
Процедура Тест()
	Если Ложь Тогда
		Попытка
			А = 1;
		Исключение
			А = 2;
		КонецЕсли;
	КонецЕсли;
КонецПроцедуры
";
        let found = findings(src);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].0, ExprErrorKind::UnbalancedCodeBlock);
    }

    #[test]
    fn unclosed_attempt_is_reported() {
        let src = "\
Процедура Тест()
	Попытка
		А = 1;
КонецПроцедуры
";
        let found = findings(src);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].0, ExprErrorKind::UnbalancedCodeBlock);
    }

    /// Директивы препроцессора блоками языка не считаются.
    #[test]
    fn preprocessor_directives_are_not_blocks() {
        let src = "\
#Если Сервер Тогда
Процедура Тест()
	Если Истина Тогда
		А = 1;
	КонецЕсли;
КонецПроцедуры
#КонецЕсли
";
        assert!(findings(src).is_empty(), "{:#?}", findings(src));
    }

    /// Кириллица не должна ломать посимвольный скан: байт продолжения буквы при
    /// чтении как `char` даёт NEL, который считается пробелом, и побайтовый
    /// пропуск разрезал букву пополам (паника «not a char boundary»).
    #[test]
    fn cyrillic_text_does_not_break_scanning() {
        let src = "\
Процедура Тест()
	// Проверка: комментарий с кириллицей и «кавычками»
	Значение = Новый Массив;
	Значение.Добавить(1);
	Результат = СтрШаблон(\"%1\", Значение).ВРег();
КонецПроцедуры
";
        assert!(findings(src).is_empty(), "{:#?}", findings(src));
    }

    #[test]
    fn member_access_after_constructor_is_reported() {
        let src = "Процедура Тест()\n\tР = Новый Файл(\"x\").Расширение;\nКонецПроцедуры\n";
        let found = findings(src);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].0, ExprErrorKind::MemberAccessOnExpression);
    }

    #[test]
    fn member_access_after_grouping_parens_is_reported() {
        let src = "Процедура Тест()\n\tР = (Новый Файл(\"x\")).Размер();\nКонецПроцедуры\n";
        let found = findings(src);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].0, ExprErrorKind::MemberAccessOnExpression);
    }

    /// Обращение после ВЫЗОВА законно — это самая частая конструкция языка.
    #[test]
    fn member_access_after_call_is_clean() {
        let src = "\
Процедура Тест()
	Выборка = Запрос.Выполнить().Выбрать();
	Пока Выборка.Следующий() Цикл
	КонецЦикла;
	Стр = СтрШаблон(\"%1\", Значение).ВРег();
КонецПроцедуры
";
        assert!(findings(src).is_empty(), "{:#?}", findings(src));
    }

    /// Тернарный оператор записан как вызов — обращение к члену у его результата
    /// платформа принимает (встречается в выгрузке УТ).
    #[test]
    fn member_access_after_ternary_is_clean() {
        let src = "\
Процедура Тест(Элемент)
	Индекс = ?(Элемент.Родитель = Неопределено, Дерево.Строки, Элемент.Родитель.Строки).Индекс(Элемент);
КонецПроцедуры
";
        assert!(findings(src).is_empty(), "{:#?}", findings(src));
    }

    /// Свойство с именем `Процедура` — не заголовок процедуры: слово посередине
    /// процедуры не должно сбрасывать стек блоков (на выгрузке УТ это давало 472
    /// ложные находки `UnbalancedCodeBlock` в 75 модулях).
    #[test]
    fn property_named_procedure_is_not_a_boundary() {
        let src = "\
Процедура Тест()
	Обработчик = Обработчики.Добавить();
	Обработчик.Процедура = \"Модуль.Метод\";
	Если Истина Тогда
		А = 1;
	КонецЕсли;
КонецПроцедуры
";
        assert!(findings(src).is_empty(), "{:#?}", findings(src));
    }

    /// Свойство с именем ключевого слова тоже не блок: в выгрузке УТ есть
    /// `Параметры.КонецЦикла = ВершинаВыхода` и `СтрокаЗадачи.Цикл = …`.
    #[test]
    fn property_named_keyword_is_not_a_block() {
        let src = "\
Процедура Тест(Параметры, СтрокаЗадачи)
	Параметры.НачалоЦикла = 1;
	Параметры.КонецЦикла = 2;
	СтрокаЗадачи.Цикл = 3;
	Если Истина Тогда
		А = 1;
	КонецЕсли;
КонецПроцедуры
";
        assert!(findings(src).is_empty(), "{:#?}", findings(src));
    }
}
