//! Корпусный замер шума правила `unknown_type_member` на реальной конфигурации.
//!
//! Модульные тесты видят только тот код, под который правило задумано; масштаб
//! ложных срабатываний виден исключительно на корпусе. Замер 2026-10-03 на
//! выгрузке 14943 модулей (level=3): до правки имени-тени — 28536 находок этого
//! вида, после — 4980. Почти всё, что правило сообщало на КОМПИЛИРУЕМОЙ
//! конфигурации, было шумом.
//!
//! Корпус — выгрузка конфигурации, которая КОМПИЛИРУЕТСЯ. Настоящих «нет члена
//! у платформенного типа» в ней быть не может вовсе (разве что дефект самой
//! выгрузки), поэтому порог здесь — «сколько угодно шума, но не лавина»: он
//! ловит срыв правила, а не измеряет качество.
//!
//! ```pwsh
//! $env:BSL_CONTEXT_CORPUS_PATH = "C:\Repo1C"
//! $env:BSL_CONTEXT_PLATFORM_PATH = "C:\Program Files\1cv8\8.3.27.1786"
//! cargo test -p bsl-validator --test corpus_type_name_noise --release -- --ignored --nocapture
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use bsl_validator::{validate_module_with_profile, ExprErrorKind, Profile};
use platform_index::load_from_hbk;

const CORPUS_ENV: &str = "BSL_CONTEXT_CORPUS_PATH";

/// Потолок числа `unknown_type_member` на корпусе. Замер 2026-10-03 на 14943
/// модулях (level=3): до правки имени-тени 28536, после 4980. Порог с запасом: он
/// сторожит срыв правила, а не точное число (состав корпуса у каждого свой).
///
/// Остаток — ДРУГИЕ классы, к имени переменной отношения не имеющие. Два из них
/// закрыты в issue #36 — члены контекста набора записей (`Отбор.Регистратор`) и
/// параметры сеанса, объявленные в КОНФИГУРАЦИИ (`ПараметрыСеанса.<параметр>`):
/// обоим нужны метаданные конфигурации, и без источника имён они молчат. После
/// этого на выгрузке (9 989 модулей, `level=3`) счёт упал 867 → 171.
const MAX_UNKNOWN_TYPE_MEMBER: usize = 8000;

fn collect_bsl(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_bsl(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("bsl") {
            out.push(path);
        }
    }
}

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

#[test]
#[ignore = "требует выгрузку конфигурации; путь — в BSL_CONTEXT_CORPUS_PATH"]
fn type_member_noise_on_real_corpus() {
    let Ok(corpus) = std::env::var(CORPUS_ENV) else {
        eprintln!("skip: не задан {CORPUS_ENV}");
        return;
    };
    let Some(hbk) = hbk_path() else {
        eprintln!("skip: hbk не найден (задайте BSL_CONTEXT_PLATFORM_PATH)");
        return;
    };
    let root = PathBuf::from(&corpus);
    assert!(root.is_dir(), "корпус не найден: {corpus}");
    let root = root.as_path();

    let index = load_from_hbk(&hbk).expect("PlatformIndex");
    let mut files = Vec::new();
    collect_bsl(root, &mut files);
    // Ограничение объёма — для быстрых сравнений «до/после» на части корпуса.
    if let Ok(limit) = std::env::var("BSL_CONTEXT_CORPUS_LIMIT") {
        if let Ok(n) = limit.parse::<usize>() {
            files.truncate(n);
        }
    }
    // Дамп всех находок — для построчного сравнения двух состояний кода.
    let dump_path = std::env::var("BSL_CONTEXT_CORPUS_DUMP").ok();
    let mut dump: Vec<String> = Vec::new();
    // Шардинг для параллельного замера: `BSL_CONTEXT_CORPUS_SHARD="i/n"` оставляет
    // каждый n-й модуль. Список обходится в детерминированном порядке, поэтому
    // объединение шардов равно полному прогону, а время падает почти линейно:
    // 14 943 модуля в один процесс — ~21 минута, в четыре — ~6.
    let shard = std::env::var("BSL_CONTEXT_CORPUS_SHARD").ok();
    if let Some((index, total)) = shard.as_deref().and_then(|s| s.split_once('/')) {
        if let (Ok(index), Ok(total)) =
            (index.trim().parse::<usize>(), total.trim().parse::<usize>())
        {
            if total > 1 && index < total {
                files = files
                    .into_iter()
                    .enumerate()
                    .filter(|(i, _)| i % total == index)
                    .map(|(_, path)| path)
                    .collect();
            }
        }
    }

    // Имена, которые проверка членов может принять за платформенный тип, хотя
    // это свойство контекста: их состав показывает, сколько проверок правило
    // отдаёт молчанию ради отсутствия ложных находок.
    let both: Vec<String> = index
        .global_properties
        .iter()
        .filter(|p| index.find_type(&p.name_ru).is_some())
        .map(|p| format!("{} (тип свойства: {})", p.name_ru, p.type_name))
        .collect();
    println!("=== свойства контекста, имена которых совпали с типом ===");
    println!("всего: {}", both.len());
    for name in both.iter().take(20) {
        println!("   {name}");
    }

    let mut by_kind: HashMap<String, usize> = HashMap::new();
    let mut samples: Vec<String> = Vec::new();
    let mut modules = 0usize;

    println!("модулей найдено: {}", files.len());
    for (idx, path) in files.iter().enumerate() {
        if idx % 1000 == 0 {
            println!("обработано {idx}/{}", files.len());
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        modules += 1;
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();
        let result =
            validate_module_with_profile(&index, &text, Some(&rel), None, 3, Profile::Full);
        for err in &result.errors {
            let kind = format!("{:?}", err.kind);
            *by_kind.entry(kind.clone()).or_default() += 1;
            dump.push(format!(
                "{rel}|{}|{}|{kind}|{}",
                err.line, err.col, err.message
            ));
            if err.kind == ExprErrorKind::UnknownTypeMember && samples.len() < 15 {
                samples.push(format!("{rel}:{} — {}", err.line, err.message));
            }
        }
    }

    if let Some(path) = dump_path {
        let mut sorted = dump;
        sorted.sort();
        if let Err(e) = fs::write(&path, sorted.join("\n")) {
            eprintln!("не удалось записать дамп {path}: {e}");
        } else {
            println!("дамп находок: {path} ({} строк)", sorted.len());
        }
    }

    println!("\n=== ЗАМЕР: находки по видам на {modules} модулях (level=3) ===");
    let mut kinds: Vec<_> = by_kind.iter().collect();
    kinds.sort_by(|a, b| b.1.cmp(a.1));
    for (kind, count) in kinds {
        println!("{kind}: {count}");
    }
    println!("\n=== примеры unknown_type_member ===");
    for sample in &samples {
        println!("   {sample}");
    }

    let unknown = *by_kind.get("UnknownTypeMember").unwrap_or(&0);
    println!("\nИТОГО unknown_type_member: {unknown}");
    assert!(
        unknown <= MAX_UNKNOWN_TYPE_MEMBER,
        "unknown_type_member: {unknown} > {MAX_UNKNOWN_TYPE_MEMBER} — правило сорвалось"
    );
}
