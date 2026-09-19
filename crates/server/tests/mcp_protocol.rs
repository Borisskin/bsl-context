//! Сквозная проверка MCP-транспорта без внешнего файла справки платформы.
//!
//! Сервер поднимается на случайном loopback-порту с пустым `PlatformIndex`.
//! Содержательные результаты инструментов покрывают отдельные тесты; здесь
//! проверяется именно публичный контракт: initialize, tools/list и диспетч всех
//! инструментов через Streamable HTTP.

use bsl_context_server::{config::Config, http, mcp_server::BslContextServer};
use platform_index::PlatformIndex;
use serde_json::{json, Value};

fn post_json(url: &str, body: Value) -> (u16, String) {
    let response = ureq::post(url)
        .set("Content-Type", "application/json")
        .set("Accept", "application/json, text/event-stream")
        .send_json(body);

    match response {
        Ok(response) => {
            let status = response.status();
            let body = response.into_string().unwrap_or_default();
            (status, body)
        }
        Err(ureq::Error::Status(_, response)) => {
            let status = response.status();
            let body = response.into_string().unwrap_or_default();
            (status, body)
        }
        Err(error) => panic!("MCP HTTP request failed: {error}"),
    }
}

async fn rpc(url: String, body: Value) -> Value {
    let (status, response_body) = tokio::task::spawn_blocking(move || post_json(&url, body))
        .await
        .expect("HTTP client task panicked");
    assert_eq!(status, 200, "unexpected HTTP status; body: {response_body}");
    serde_json::from_str(&response_body)
        .unwrap_or_else(|error| panic!("invalid JSON-RPC response ({error}): {response_body}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn all_tools_are_reachable_over_streamable_http() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback listener");
    let address = listener.local_addr().expect("listener address");
    let app = http::router(
        Config::default(),
        Some(BslContextServer::new(PlatformIndex::new())),
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("acceptance server failed");
    });
    let url = format!("http://{address}/mcp");

    let initialize = rpc(
        url.clone(),
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "bsl-context-acceptance", "version": "1"}
            }
        }),
    )
    .await;
    assert_eq!(
        initialize.pointer("/result/serverInfo/name"),
        Some(&json!("bsl-context-rs"))
    );

    let listed = rpc(
        url.clone(),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
    )
    .await;
    let mut actual: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .expect("tools/list result")
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name"))
        .collect();
    actual.sort_unstable();

    let calls = [
        ("get_constructors", json!({"type_name": "НетТакогоТипа"})),
        ("get_enum_values", json!({"type_name": "НетТакогоТипа"})),
        (
            "get_member",
            json!({"type_name": "НетТакогоТипа", "member_name": "НетТакогоЧлена"}),
        ),
        ("get_members", json!({"type_name": "НетТакогоТипа"})),
        ("info", json!({"name": "НетТакогоЭлемента"})),
        ("rebuild_symbol_index", json!({"repo": "нет-такого"})),
        ("reconnect_symbol_source", json!({"repo": "нет-такого"})),
        ("reload_config", json!({})),
        ("reserved_names", json!({})),
        ("search", json!({"query": "НетТакогоЭлемента"})),
        ("symbol_sources_status", json!({})),
        (
            "validate_enum",
            json!({"type_name": "НетТакогоТипа", "value_name": "НетТакогоЗначения"}),
        ),
        (
            "validate_method_call",
            json!({"method_name": "НетТакогоМетода", "arg_count": 0}),
        ),
        ("validate_module", json!({"source": "Сообщить(1);"})),
    ];
    let mut expected: Vec<&str> = calls.iter().map(|(name, _)| *name).collect();
    expected.sort_unstable();
    assert_eq!(
        actual, expected,
        "tools/list differs from acceptance matrix"
    );

    for (offset, (name, arguments)) in calls.into_iter().enumerate() {
        let response = rpc(
            url.clone(),
            json!({
                "jsonrpc": "2.0",
                "id": offset + 10,
                "method": "tools/call",
                "params": {"name": name, "arguments": arguments}
            }),
        )
        .await;
        assert!(
            response.get("error").is_none(),
            "{name} returned JSON-RPC error: {response}"
        );
        assert!(
            response.pointer("/result/content/0/text").is_some(),
            "{name} returned no text content: {response}"
        );
    }

    server.abort();
}
