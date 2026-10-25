//! End-to-end: real HTTP server, bearer auth, MCP initialize + tool calls
//! against the fixture journal folder.

use std::sync::Arc;

use holtzman::{
    config::Config,
    screen::Screen,
    server::{Ship, app},
};
use serde_json::{Value, json};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

/// A fake game window: a 1920x1080 gradient.
struct TestPattern;

impl Screen for TestPattern {
    fn capture(&self) -> anyhow::Result<image::RgbaImage> {
        Ok(image::RgbaImage::from_fn(1920, 1080, |x, y| {
            image::Rgba([(x % 256) as u8, (y % 256) as u8, 200, 255])
        }))
    }
}

async fn spawn() -> (String, String) {
    let mut config = Config::generate();
    config.journal_dir = Some(FIXTURES.into());
    config.dry_run = true;
    let token = config.token.clone();
    let mut ship = Ship::new(config, None).unwrap();
    ship.screen = Arc::new(TestPattern);
    let ship = Arc::new(ship);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app(ship)).await.unwrap() });
    (format!("http://{addr}/mcp"), token)
}

/// POST a JSON-RPC message; returns (session id, parsed result message).
async fn rpc(
    url: &str,
    token: &str,
    session: Option<&str>,
    body: Value,
) -> (Option<String>, Value) {
    let client = reqwest::Client::new();
    let mut req = client
        .post(url)
        .bearer_auth(token)
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json")
        .body(body.to_string());
    if let Some(s) = session {
        req = req
            .header("Mcp-Session-Id", s)
            .header("MCP-Protocol-Version", "2025-06-18");
    }
    let resp = req.send().await.unwrap();
    assert!(resp.status().is_success(), "status {}", resp.status());
    let sid = resp
        .headers()
        .get("mcp-session-id")
        .map(|v| v.to_str().unwrap().to_string());
    let text = resp.text().await.unwrap();
    // Either plain JSON or an SSE stream with `data:` lines.
    let msg = text
        .lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .filter_map(|d| serde_json::from_str::<Value>(d).ok())
        .find(|v| v.get("id").is_some())
        .or_else(|| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    (sid, msg)
}

#[tokio::test]
async fn rejects_missing_token() {
    let (url, _) = spawn().await;
    let resp = reqwest::Client::new()
        .post(&url)
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn lists_and_calls_tools() {
    let (url, token) = spawn().await;
    let (sid, init) = rpc(
        &url,
        &token,
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18","capabilities":{},
            "clientInfo":{"name":"test","version":"0"}}}),
    )
    .await;
    assert!(init["result"]["serverInfo"].is_object(), "{init}");
    let sid = sid.expect("session id");
    rpc(
        &url,
        &token,
        Some(&sid),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    )
    .await;

    let (_, list) = rpc(
        &url,
        &token,
        Some(&sid),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    )
    .await;
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for want in [
        "cursor_position",
        "record_click_target",
        "status",
        "market",
        "cargo",
        "nav_route",
        "recent_events",
        "plot_route",
        "press_binding",
        "screenshot",
    ] {
        assert!(names.contains(&want), "missing {want}: {names:?}");
    }

    let (_, status) = rpc(
        &url,
        &token,
        Some(&sid),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"status","arguments":{}}}),
    )
    .await;
    let text = status["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Pratchett Port"), "{text}");

    let (_, market) = rpc(
        &url,
        &token,
        Some(&sid),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"market","arguments":{"commodity":"cobalt"}}}),
    )
    .await;
    assert!(
        market["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("19770")
    );

    let (_, denied) = rpc(
        &url,
        &token,
        Some(&sid),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"press_binding","arguments":{"binding":"PrimaryFire"}}}),
    )
    .await;
    assert_eq!(denied["result"]["isError"], true, "{denied}");

    let (_, shot) = rpc(
        &url,
        &token,
        Some(&sid),
        json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"screenshot","arguments":{"region":"left","max_edge":512}}}),
    )
    .await;
    let content = &shot["result"]["content"];
    let info: Value = serde_json::from_str(content[0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(info["window"], json!([1920, 1080]), "{info}");
    // The left half is 960x1080; its long side shrinks to 512.
    assert_eq!(info["image"], json!([455, 512]), "{info}");
    assert!(info["journal"]["gui_focus"].is_string(), "{info}");
    assert_eq!(content[1]["type"], "image", "{shot}");
    assert_eq!(content[1]["mimeType"], "image/jpeg");
    assert!(content[1]["data"].as_str().unwrap().starts_with("/9j/")); // JPEG magic

    let (_, bad) = rpc(
        &url,
        &token,
        Some(&sid),
        json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"screenshot","arguments":{"region":"nowhere"}}}),
    )
    .await;
    assert_eq!(bad["result"]["isError"], true, "{bad}");
}
