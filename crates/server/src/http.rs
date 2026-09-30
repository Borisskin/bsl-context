//! HTTP-роутер: /health (для healthcheck-обёртки супервизора) и /mcp
//! (всегда Streamable HTTP; без индекса справочные инструменты отвечают отказом).

use std::sync::Arc;

use axum::{extract::State, response::Json, routing::get, Router};
use rmcp::transport::streamable_http_server::{
    session::never::NeverSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use serde::Serialize;

use crate::config::Config;
use crate::mcp_server::BslContextServer;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// Краткая статистика индекса для /health (заполнена, если индекс загружен).
    pub index_stats: Option<IndexStats>,
    /// Сервер хранит состояние индекса и актуальную карту источников имён:
    /// /health берёт снимок карты на каждый запрос, включая работу без индекса.
    server: BslContextServer,
}

#[derive(Clone, Serialize)]
pub struct IndexStats {
    pub global_methods: usize,
    pub global_properties: usize,
    pub types: usize,
    pub enum_types: usize,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
    started_at: String,
    uptime_sec: i64,
    /// Путь к платформе из конфига. None — пользователь не указал.
    platform_path: Option<String>,
    /// `true`, если индекс платформы успешно загружен.
    index_loaded: bool,
    /// Причина недоступности платформенного индекса, если он не загружен.
    #[serde(skip_serializing_if = "Option::is_none")]
    unavailable_reason: Option<String>,
    /// Статистика индекса (когда `index_loaded == true`).
    #[serde(skip_serializing_if = "Option::is_none")]
    index_stats: Option<IndexStats>,
    /// Дефолтный уровень валидации (из `config.toml`).
    default_validation_level: u8,
    /// Алиас → `describe()` подключённого источника имён этой конфигурации,
    /// либо "не собран" (слот настроен, но lite-база ещё не построена).
    /// Пусто — конфигураций не настроено вовсе.
    symbol_sources: std::collections::BTreeMap<String, String>,
}

/// Собрать роутер: /health и /mcp через Streamable HTTP при любом состоянии индекса.
pub fn router(config: Config, server: BslContextServer) -> Router {
    // Список разрешённых Host для /mcp (защита rmcp от DNS-rebinding). Клонируем
    // до перемещения config в AppState.
    let allowed_hosts = config.allowed_hosts.clone();
    let index_stats = server.index_loaded().then(|| IndexStats {
        global_methods: server.index.global_methods.len(),
        global_properties: server.index.global_properties.len(),
        types: server.index.types.len(),
        enum_types: server.index.enum_types_count(),
    });

    let state = AppState {
        config: Arc::new(config),
        started_at: chrono::Utc::now(),
        index_stats,
        server: server.clone(),
    };

    // Stateless Streamable HTTP — устраняет 404 Session not found при
    // рестарте сервера (см. карточку #1184 для mcp-cache-ci v0.3.0).
    let session_manager = Arc::new(NeverSessionManager::default());
    let service_factory = move || Ok(server.clone());
    let http_config = StreamableHttpServerConfig::default()
        .with_stateful_mode(false)
        .with_json_response(true)
        .with_allowed_hosts(allowed_hosts);
    let http_service = StreamableHttpService::new(service_factory, session_manager, http_config);
    Router::new()
        .route("/health", get(health))
        .nest_service("/mcp", http_service)
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let now = chrono::Utc::now();
    let uptime = (now - state.started_at).num_seconds();
    let mut symbol_sources = std::collections::BTreeMap::new();
    // Снимок карты берём и сразу отпускаем std-блокировку: ниже в цикле `await`,
    // а std::sync-блокировка не должна переживать его.
    let sources = state.server.sources_snapshot();
    for (name, slot) in sources.iter() {
        let status = slot
            .source
            .read()
            .await
            .as_ref()
            .map(|s| s.describe())
            .unwrap_or_else(|| "не собран".to_string());
        symbol_sources.insert(name.clone(), status);
    }
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        started_at: state.started_at.to_rfc3339(),
        uptime_sec: uptime,
        platform_path: state
            .config
            .platform_path
            .as_ref()
            .map(|p| p.display().to_string()),
        index_loaded: state.server.index_loaded(),
        unavailable_reason: state.server.unavailable_reason().map(str::to_string),
        index_stats: state.index_stats.clone(),
        default_validation_level: state.config.default_validation_level,
        symbol_sources,
    })
}
