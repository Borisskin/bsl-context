//! Золотые ответы инструментов: выявление регрессий в выдаче.
//!
//! Каждый инструмент вызывается с фиксированными аргументами, и его полный
//! ответ (Markdown или JSON) сверяется с эталоном в `tests/golden/`. Любое
//! изменение формата, порядка, текстов сообщений или состава полей — красный
//! тест с указанием первой разошедшейся строки.
//!
//! Два набора эталонов:
//! - `tests/golden/empty/` — пустой платформенный индекс; гоняется всегда, в том
//!   числе в CI, и ловит регрессии формата «не найдено» и текстов ошибок;
//! - `tests/golden/hbk-<версия>/` — реальный `shcntx_ru.hbk` (env-гейт, как у
//!   остальных интеграционных тестов). Эталоны снимаются с конкретной версии
//!   платформы: у другой версии отличается и справка, поэтому чужой каталог
//!   эталонов тест не трогает, а пропускается с подсказкой.
//!
//! Обновление эталонов после намеренного изменения выдачи:
//! ```pwsh
//! $env:BSL_CONTEXT_PLATFORM_PATH = 'C:\Program Files\1cv8\8.3.27.2342'
//! $env:BSL_CONTEXT_UPDATE_GOLDEN = '1'
//! cargo test -p bsl-context-server --test tool_responses
//! ```
//! Файлы эталонов после обновления — проверить глазами и закоммитить вместе с
//! изменением, которое их поменяло.

use std::fs;
use std::path::{Path, PathBuf};

use bsl_context_server::mcp_server::{
    BslContextServer, GetMemberParams, InfoParams, RebuildSymbolIndexParams,
    ReconnectSymbolSourceParams, SearchParams, TypeNameParams, ValidateEnumParams,
    ValidateMethodCallParams, ValidateModuleParams,
};
use bsl_validator::Profile;
use platform_index::{load_from_hbk, PlatformIndex};
use rmcp::handler::server::wrapper::Parameters;

/// Модуль с заведомыми находками: несуществующий тип и неверное число
/// аргументов. На пустом индексе сработают другие правила — это тоже фиксируется
/// эталоном, поэтому набор общий для обоих тестов.
const MODULE_WITH_FINDINGS: &str =
    "Процедура Проверка()\n    Х = Новый НетТакогоТипа;\n    Сообщить();\nКонецПроцедуры\n";

/// Минимальный корректный модуль: ответ обязан остаться «без находок».
const CLEAN_MODULE: &str = "Процедура Проверка()\n    Сообщить(\"ок\");\nКонецПроцедуры\n";

fn validate_params(source: &str) -> ValidateModuleParams {
    ValidateModuleParams {
        source: source.to_string(),
        level: Some(3),
        profile: Some("full".to_string()),
        path: None,
        module_path: None,
        form_attributes: None,
        repo: None,
    }
}

/// Фиксированный набор запросов, покрывающий все инструменты. Порядок и имена
/// кейсов — часть контракта: по имени файла эталона видно, что поменялось.
async fn collect_responses(srv: &BslContextServer) -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = Vec::new();

    out.push((
        "search_strnajti",
        srv.search(Parameters(SearchParams {
            query: "СтрНайти".to_string(),
            limit: Some(5),
        }))
        .await,
    ));
    // ВАЖНО для новых кейсов: многословные и беспрефиксные запросы уходят в
    // ветки `search`, итерирующие `HashMap` (word-order, подстрока) — порядок
    // выдачи между запусками случаен. Такие запросы в эталоны добавлять нельзя,
    // пока результаты не отсортированы; здешние однословные идут по BTreeMap и
    // стабильны.
    out.push((
        "search_array_en",
        srv.search(Parameters(SearchParams {
            query: "Array".to_string(),
            limit: Some(3),
        }))
        .await,
    ));
    out.push((
        "search_no_matches",
        srv.search(Parameters(SearchParams {
            query: "ЪНетТакогоЭлемента".to_string(),
            limit: Some(3),
        }))
        .await,
    ));

    out.push((
        "info_tablica_znachenij",
        srv.info(Parameters(InfoParams {
            name: "ТаблицаЗначений".to_string(),
            kind: None,
        }))
        .await,
    ));
    out.push((
        "info_unknown",
        srv.info(Parameters(InfoParams {
            name: "ЪНетТакогоЭлемента".to_string(),
            kind: None,
        }))
        .await,
    ));

    out.push((
        "get_member_add",
        srv.get_member(Parameters(GetMemberParams {
            type_name: "ТаблицаЗначений".to_string(),
            member_name: "Добавить".to_string(),
        }))
        .await,
    ));
    out.push((
        "get_members_massiv",
        srv.get_members(Parameters(TypeNameParams {
            type_name: "Массив".to_string(),
        }))
        .await,
    ));
    out.push((
        "get_constructors_tablica_znachenij",
        srv.get_constructors(Parameters(TypeNameParams {
            type_name: "ТаблицаЗначений".to_string(),
        }))
        .await,
    ));
    out.push((
        "get_enum_values_tip_razmeshcheniya",
        srv.get_enum_values(Parameters(TypeNameParams {
            type_name: "ТипРазмещенияТекстаТабличногоДокумента".to_string(),
        }))
        .await,
    ));

    out.push((
        "validate_enum_valid",
        srv.validate_enum(Parameters(ValidateEnumParams {
            type_name: "ТипРазмещенияТекстаТабличногоДокумента".to_string(),
            value_name: "Авто".to_string(),
        }))
        .await,
    ));
    out.push((
        "validate_enum_invalid",
        srv.validate_enum(Parameters(ValidateEnumParams {
            type_name: "ТипРазмещенияТекстаТабличногоДокумента".to_string(),
            value_name: "ЪНетТакогоЗначения".to_string(),
        }))
        .await,
    ));
    out.push((
        "validate_method_call_ok",
        srv.validate_method_call(Parameters(ValidateMethodCallParams {
            method_name: "СтрНайти".to_string(),
            arg_count: 2,
        }))
        .await,
    ));
    out.push((
        "validate_method_call_bad",
        srv.validate_method_call(Parameters(ValidateMethodCallParams {
            method_name: "СтрНайти".to_string(),
            arg_count: 0,
        }))
        .await,
    ));

    out.push((
        "validate_module_findings",
        srv.validate_module(Parameters(validate_params(MODULE_WITH_FINDINGS)))
            .await,
    ));
    out.push((
        "validate_module_clean",
        srv.validate_module(Parameters(validate_params(CLEAN_MODULE)))
            .await,
    ));

    // Issue #13: источник модуля — текст или файл, и оба отказа на неверный вызов.
    // Кейсы гоняются и на пустом индексе (в CI), поэтому сторожат тексты отказов.
    out.push((
        "validate_module_path_in_source",
        srv.validate_module(Parameters(validate_params(
            r"b:\projects\x\src\cf\CommonModules\НетТакогоМодуля\Ext\Module.bsl",
        )))
        .await,
    ));
    out.push((
        "validate_module_no_source",
        srv.validate_module(Parameters(ValidateModuleParams {
            path: None,
            ..validate_params("")
        }))
        .await,
    ));
    out.push((
        "validate_module_path_without_root",
        srv.validate_module(Parameters(ValidateModuleParams {
            path: Some("base/CommonModules/Х/Ext/Module.bsl".to_string()),
            ..validate_params("")
        }))
        .await,
    ));
    out.push((
        "validate_module_both_sources",
        srv.validate_module(Parameters(ValidateModuleParams {
            path: Some("base/CommonModules/Х/Ext/Module.bsl".to_string()),
            ..validate_params(CLEAN_MODULE)
        }))
        .await,
    ));

    out.push(("reserved_names", srv.reserved_names().await));
    out.push(("symbol_sources_status", srv.symbol_sources_status().await));
    out.push((
        "reconnect_missing",
        srv.reconnect_symbol_source(Parameters(ReconnectSymbolSourceParams {
            repo: Some("нет-такого".to_string()),
        }))
        .await,
    ));
    out.push((
        "rebuild_missing",
        srv.rebuild_symbol_index(Parameters(RebuildSymbolIndexParams {
            repo: Some("нет-такого".to_string()),
        }))
        .await,
    ));
    out.push(("reload_config_without_path", srv.reload_config().await));

    out
}

/// Каталог эталонов рядом с тестом (не зависит от рабочего каталога запуска).
fn golden_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

/// Версия платформы — компонент пути вида `8.3.27.2342`. По ней выбирается
/// каталог эталонов: у другой версии справка отличается, и сверять её с чужими
/// эталонами нельзя.
fn platform_version(root: &Path) -> Option<String> {
    root.components().rev().find_map(|component| {
        let std::path::Component::Normal(name) = component else {
            return None;
        };
        let name = name.to_string_lossy();
        let looks_like_version =
            name.starts_with(|c: char| c.is_ascii_digit()) && name.contains('.');
        looks_like_version.then(|| name.to_string())
    })
}

fn find_hbk(root: &Path) -> Option<PathBuf> {
    [
        root.join("shcntx_ru.hbk"),
        root.join("bin").join("shcntx_ru.hbk"),
    ]
    .into_iter()
    .find(|path| path.exists())
}

fn update_mode() -> bool {
    // Строго «1»: `BSL_CONTEXT_UPDATE_GOLDEN=0` не должен молча переписывать
    // эталоны и «проходить» вместо сверки.
    std::env::var("BSL_CONTEXT_UPDATE_GOLDEN").is_ok_and(|value| value == "1")
}

/// Первая разошедшаяся строка — этого достаточно, чтобы понять регрессию,
/// и не заваливать вывод полным диффом многокилобайтных ответов.
fn first_difference(expected: &str, actual: &str) -> String {
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    for index in 0..expected_lines.len().max(actual_lines.len()) {
        let left = expected_lines.get(index).copied().unwrap_or("<строки нет>");
        let right = actual_lines.get(index).copied().unwrap_or("<строки нет>");
        if left != right {
            return format!(
                "  строка {}:\n    эталон: {left:?}\n    ответ:  {right:?}",
                index + 1
            );
        }
    }
    "  (расхождение только в переводе строк?)".to_string()
}

/// Сверить ответы с эталонами каталога. В режиме обновления эталоны
/// перезаписываются и тест проходит — решение об изменении принимает человек,
/// просмотрев дифф в контроле версий.
fn check_responses(dir: &Path, responses: Vec<(&'static str, String)>) {
    let update = update_mode();
    let total = responses.len();
    let mut failures: Vec<String> = Vec::new();
    let mut written = 0usize;

    for (name, actual) in responses {
        let path = dir.join(format!("{name}.txt"));
        let actual = actual.replace("\r\n", "\n");
        if update {
            fs::create_dir_all(dir).expect("создание каталога эталонов");
            fs::write(&path, &actual).expect("запись эталона");
            written += 1;
            continue;
        }
        match fs::read_to_string(&path) {
            Ok(expected) => {
                let expected = expected.replace("\r\n", "\n");
                // Хвостовые переводы строк не значимы: правка эталона
                // редактором не должна выглядеть регрессией ответа.
                let expected = expected.trim_end_matches('\n');
                let actual = actual.trim_end_matches('\n');
                if expected != actual {
                    failures.push(format!(
                        "{name}: ответ разошёлся с эталоном {}\n{}",
                        path.display(),
                        first_difference(expected, actual)
                    ));
                }
            }
            Err(_) => failures.push(format!(
                "{name}: нет эталона {} — создайте: BSL_CONTEXT_UPDATE_GOLDEN=1",
                path.display()
            )),
        }
    }

    if update {
        eprintln!("эталоны обновлены: {written} шт в {}", dir.display());
        return;
    }
    assert!(
        failures.is_empty(),
        "регрессия ответов инструментов ({} из {total}):\n{}\n\
         Если изменение намеренное — обновите эталоны: BSL_CONTEXT_UPDATE_GOLDEN=1",
        failures.len(),
        failures.join("\n\n")
    );
}

/// Пустой индекс: набор эталонов не зависит от установленной платформы и
/// гоняется в CI. Ловит регрессии формата «не найдено» и текстов ошибок.
#[tokio::test]
async fn empty_index_responses_match_golden() {
    let srv = BslContextServer::new(PlatformIndex::new());
    let responses = collect_responses(&srv).await;
    check_responses(&golden_root().join("empty"), responses);
}

/// Реальный `shcntx_ru.hbk`: полные ответы на живых данных, включая разбор
/// справки. Условие — та же env-переменная, что у остальных интеграционных
/// тестов; каталог эталонов привязан к версии платформы.
#[tokio::test]
async fn real_hbk_responses_match_golden() {
    let Some(root) = std::env::var("BSL_CONTEXT_PLATFORM_PATH")
        .ok()
        .map(PathBuf::from)
    else {
        eprintln!("skip: BSL_CONTEXT_PLATFORM_PATH не задан");
        return;
    };
    let Some(hbk) = find_hbk(&root) else {
        eprintln!("skip: shcntx_ru.hbk не найден в {}", root.display());
        return;
    };
    let Some(version) = platform_version(&root) else {
        eprintln!(
            "skip: не удалось определить версию платформы в {} (ожидается компонент вида 8.3.27.2342)",
            root.display()
        );
        return;
    };
    let dir = golden_root().join(format!("hbk-{version}"));
    if !dir.exists() && !update_mode() {
        eprintln!(
            "skip: нет эталонов для версии {version} ({}). Создать: \
             $env:BSL_CONTEXT_UPDATE_GOLDEN='1'; cargo test -p bsl-context-server --test tool_responses",
            dir.display()
        );
        return;
    }

    let index = load_from_hbk(&hbk).expect("PlatformIndex из hbk");
    let srv = BslContextServer::with_defaults(index, 3, Profile::Full);
    let responses = collect_responses(&srv).await;
    check_responses(&dir, responses);
}
