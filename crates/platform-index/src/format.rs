//! Markdown-форматтер для ответов MCP-tools.
//!
//! Порт `MarkdownFormatterService.kt`. Формат соблюдён 1:1 с апстримом, чтобы
//! модель видела привычный layout.

use std::fmt::Write;

use crate::entities::{Constructor, Definition, EnumValue, Method, Property, Signature, Type};

pub fn format_query_header(query: &str) -> String {
    format!("# Результаты поиска: '{query}'\n\n")
}

pub fn format_search_results(results: &[Definition]) -> String {
    if results.is_empty() {
        return "❌ **Не найдено:** Ничего не найдено для запроса\n".to_string();
    }
    if results.len() == 1 {
        return format_member(&results[0]);
    }

    let mut out = String::new();
    let _ = writeln!(out, "## Найдено {} элементов\n", results.len());
    for d in results {
        let desc = if d.description().is_empty() {
            "Нет описания"
        } else {
            d.description()
        };
        let _ = writeln!(out, "### {}", d.name_ru());
        let _ = writeln!(out, "**Тип элемента:** {}", d.kind_label());
        let _ = writeln!(out, "**Описание:** {desc}\n");
    }
    out
}

pub fn format_member(def: &Definition) -> String {
    match def {
        Definition::Type(t) => format_type(t),
        Definition::Method(m) => format_method(m),
        Definition::Property(p) => format_property(p),
    }
}

pub fn format_type(t: &Type) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# {}\n", t.name_ru);

    if !t.description.is_empty() {
        let _ = writeln!(out, "{}\n", t.description);
    }

    if t.has_methods() {
        out.push_str("## Методы\n\n");
        for m in &t.methods {
            out.push_str(&format_method_summary(m));
        }
    }

    if t.has_properties() {
        out.push_str("\n## Свойства\n\n");
        for p in &t.properties {
            out.push_str(&format_property_summary(p));
        }
    }

    if t.has_constructors() {
        out.push_str("\n## Конструкторы\n\n");
        for c in &t.constructors {
            out.push_str(&format_constructor_summary(c));
        }
    }

    if t.is_enum() {
        out.push_str("\n## Значения\n\n");
        for v in &t.enum_values {
            out.push_str(&format_enum_value_summary(v));
        }
    }

    out
}

pub fn format_method(m: &Method) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "### {}\n", m.name_ru);
    if !m.description.is_empty() {
        let _ = writeln!(out, "{}\n", m.description);
    }
    out.push_str(&format_signatures(&m.signatures, &m.name_ru));
    if !m.return_type.is_empty() {
        let _ = writeln!(out, "**Возвращаемый тип:** `{}`\n", m.return_type);
    }
    out
}

pub fn format_property(p: &Property) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "### {}\n", p.name_ru);
    if !p.description.is_empty() {
        let _ = writeln!(out, "{}\n", p.description);
    }
    let _ = writeln!(out, "**Тип:** `{}`", p.type_name);
    let _ = writeln!(
        out,
        "**Только для чтения:** {}\n",
        if p.readonly { "Да" } else { "Нет" }
    );
    out
}

pub fn format_constructors(constructors: &[Constructor], type_name: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Конструкторы объекта {type_name}");
    for c in constructors {
        let _ = writeln!(out, "## Конструктор: {} ({})", c.name, c.description);
        out.push_str(&format_signature_block(
            &c.parameters,
            &format!("Новый {type_name}"),
        ));
    }
    out
}

pub fn format_enum_values(values: &[EnumValue], type_name: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Значения системного перечисления {type_name}\n");
    if values.is_empty() {
        out.push_str(
            "❌ **Не найдено:** у типа нет значений (он не является системным перечислением)\n",
        );
        return out;
    }
    for v in values {
        out.push_str(&format_enum_value_summary(v));
    }
    out
}

fn format_signatures(signatures: &[Signature], method_name: &str) -> String {
    let mut out = String::new();
    for s in signatures {
        let _ = writeln!(out, "## Сигнатура: {} ({})", s.name, s.description);
        out.push_str(&format_signature_block(&s.parameters, method_name));
    }
    out
}

fn format_signature_block(parameters: &[crate::entities::Parameter], call_name: &str) -> String {
    let mut out = String::new();
    let inline = parameters
        .iter()
        .map(|p| {
            format!(
                "{}{}: {}",
                p.name,
                if p.required { "" } else { "?" },
                p.type_name
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    out.push_str("```bsl\n");
    let _ = writeln!(out, "{call_name}({inline})");
    out.push_str("```\n\n");

    if !parameters.is_empty() {
        out.push_str("### Параметры\n");
        for p in parameters {
            let required_mark = if p.required {
                "(обязательный)"
            } else {
                ""
            };
            let desc = if p.description.is_empty() {
                ""
            } else {
                p.description.as_str()
            };
            let _ = writeln!(
                out,
                "- **{}** *({})* {} - {}",
                p.name, p.type_name, required_mark, desc
            );
        }
        out.push('\n');
    }
    out
}

fn format_method_summary(m: &Method) -> String {
    let return_type = if m.return_type.is_empty() {
        String::new()
    } else {
        format!(": {}", m.return_type)
    };
    if m.signatures.is_empty() {
        format!("- {}(){} - {}\n", m.name_ru, return_type, m.description)
    } else {
        let mut out = String::new();
        for s in &m.signatures {
            let inline = s
                .parameters
                .iter()
                .map(|p| {
                    format!(
                        "{}{}: {}",
                        p.name,
                        if p.required { "" } else { "?" },
                        p.type_name
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(
                out,
                "- {}({}){} - {}",
                m.name_ru, inline, return_type, s.description
            );
        }
        out
    }
}

fn format_property_summary(p: &Property) -> String {
    format!("- {}: {} - {}\n", p.name_ru, p.type_name, p.description)
}

fn format_constructor_summary(c: &Constructor) -> String {
    format!("- {} - {}\n", c.name, c.description)
}

fn format_enum_value_summary(v: &EnumValue) -> String {
    if v.description.is_empty() {
        format!("- {} (`{}`)\n", v.name_ru, v.name_en)
    } else {
        format!("- {} (`{}`) - {}\n", v.name_ru, v.name_en, v.description)
    }
}
