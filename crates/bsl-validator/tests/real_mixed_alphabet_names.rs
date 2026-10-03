//! Диагностика справки платформы: имена-двойники, различающиеся только алфавитом
//! буквы (issue #18).
//!
//! В справке 8.3.17 значение перечисления `РежимОткрытияОкнаФормы` записано как
//! `БлокироватьВеcьИнтерфейс` — с ЛАТИНСКОЙ `c`. На экране это неотличимо от
//! корректного `БлокироватьВесьИнтерфейс`, а платформа принимает только второе.
//! Проверка, сравнивающая имена буквально, выдаёт `high`-находку на корректном
//! коде и подсказывает имя, которое платформа отвергает.
//!
//! Такую опечатку опаснее всего найти ДО того, как на неё пожалуется
//! пользователь. Диагностика печатает имена, у которых есть «двойник»: две
//! записи в справке, совпадающие после сведения латинско-кириллических двойников
//! и различающиеся только алфавитом. Это ровно те места, где подсказка может
//! сломать код.
//!
//! Тест помечен `#[ignore]`: он требует реального `shcntx_ru.hbk` и служит
//! инструментом проверки при обновлении версии платформы. Запуск:
//!
//! ```pwsh
//! $env:BSL_CONTEXT_PLATFORM_PATH = 'C:\Program Files\1cv8\8.3.27.1786'
//! cargo test -p bsl-validator --test real_mixed_alphabet_names -- --ignored --nocapture
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use bsl_validator::homoglyphs::{fold_lookalikes, is_mixed_alphabet};
use platform_index::{load_from_hbk, PlatformIndex};

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

/// Собрать все имена справки с указанием, где они лежат.
fn collect_names(index: &PlatformIndex) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for ty in index.types.values() {
        out.push((ty.name_ru.clone(), format!("тип {}", ty.name_ru)));
        for m in &ty.methods {
            out.push((
                m.name_ru.clone(),
                format!("метод {}.{}", ty.name_ru, m.name_ru),
            ));
        }
        for p in &ty.properties {
            out.push((
                p.name_ru.clone(),
                format!("свойство {}.{}", ty.name_ru, p.name_ru),
            ));
        }
        for v in &ty.enum_values {
            out.push((
                v.name_ru.clone(),
                format!("значение {}.{}", ty.name_ru, v.name_ru),
            ));
        }
    }
    for m in &index.global_methods {
        out.push((m.name_ru.clone(), format!("глобальный метод {}", m.name_ru)));
    }
    out
}

#[test]
#[ignore = "требует shcntx_ru.hbk; путь — в BSL_CONTEXT_PLATFORM_PATH"]
fn mixed_alphabet_names_in_help() {
    let Some(path) = hbk_path() else {
        eprintln!("skip: hbk не найден (задайте BSL_CONTEXT_PLATFORM_PATH)");
        return;
    };
    let index = load_from_hbk(&path).expect("PlatformIndex");
    let names = collect_names(&index);

    // Группируем по «сведённому» имени: два разных написания в одной группе —
    // это имя-двойник, различающееся только алфавитом.
    let mut groups: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (name, where_) in &names {
        groups
            .entry(fold_lookalikes(&name.to_lowercase()))
            .or_default()
            .push((name.clone(), where_.clone()));
    }

    let mut doppelgangers = 0usize;
    let mut mixed = 0usize;
    for entries in groups.values() {
        let unique_names: std::collections::BTreeSet<&str> =
            entries.iter().map(|(name, _)| name.as_str()).collect();
        if unique_names.len() > 1 {
            doppelgangers += 1;
            println!("\nДВОЙНИКИ (алфавит различается):");
            for (name, where_) in entries {
                let mark = if is_mixed_alphabet(name) {
                    "  ← смешанные алфавиты"
                } else {
                    ""
                };
                println!("   {name}  [{where_}]{mark}");
            }
        }
        if entries.iter().any(|(name, _)| is_mixed_alphabet(name)) {
            mixed += 1;
        }
    }

    println!("\nвсего имён в справке: {}", names.len());
    println!("групп с двойниками: {doppelgangers}");
    println!("групп со смешанными алфавитами: {mixed}");
    println!(
        "\nПроверка сведения двойников (issue #18) не даёт выдавать находку на \
         корректном коде, а подсказки с именами смешанных алфавитов отфильтрованы. \
         Список выше — кандидаты на ручной разбор при обновлении платформы."
    );
}
