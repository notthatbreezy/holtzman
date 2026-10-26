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
    spawn_with(|_| {}, None).await
}

async fn spawn_with(
    tweak: impl FnOnce(&mut Config),
    config_path: Option<std::path::PathBuf>,
) -> (String, String) {
    let mut config = Config::generate();
    config.journal_dir = Some(FIXTURES.into());
    config.dry_run = true;
    tweak(&mut config);
    let token = config.token.clone();
    let mut ship = Ship::new(config, config_path).unwrap();
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
    let instructions = init["result"]["instructions"].as_str().unwrap_or_default();
    for want in [
        "SEEING THE SCREEN",
        "observe=true",
        "read_guide",
        "ship_computer",
    ] {
        assert!(instructions.contains(want), "instructions lack {want}");
    }
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
        "fleet",
        "market",
        "cargo",
        "nav_route",
        "recent_events",
        "plot_route",
        "press_binding",
        "screenshot",
        "nav_target_start",
        "nav_target_move",
        "nav_target_confirm",
        "nav_target_cancel",
        "read_guide",
        "trade_commodities",
        "find_commodity_market",
        "plan_trade_route",
    ] {
        assert!(names.contains(&want), "missing {want}: {names:?}");
    }
    for name in [
        "trade_commodities",
        "find_commodity_market",
        "plan_trade_route",
    ] {
        let tool = list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap();
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        assert_eq!(tool["annotations"]["openWorldHint"], true);
    }
    let shot_tool = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "screenshot")
        .unwrap();
    assert_eq!(shot_tool["annotations"]["readOnlyHint"], true);
    assert_eq!(shot_tool["annotations"]["openWorldHint"], false);

    let (_, prompts) = rpc(
        &url,
        &token,
        Some(&sid),
        json!({"jsonrpc":"2.0","id":20,"method":"prompts/list"}),
    )
    .await;
    assert_eq!(
        prompts["result"]["prompts"][0]["name"], "ship_computer",
        "{prompts}"
    );
    let (_, prompt) = rpc(
        &url,
        &token,
        Some(&sid),
        json!({"jsonrpc":"2.0","id":21,"method":"prompts/get","params":{"name":"ship_computer"}}),
    )
    .await;
    let text = prompt["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("{prompt}"));
    assert!(text.starts_with("You are **HOLT**"), "{text}");
    assert!(text.contains("Rules of engagement"));
    let (_, missing) = rpc(
        &url,
        &token,
        Some(&sid),
        json!({"jsonrpc":"2.0","id":22,"method":"prompts/get","params":{"name":"nope"}}),
    )
    .await;
    assert!(missing["error"].is_object(), "{missing}");

    let (_, guide) = rpc(
        &url,
        &token,
        Some(&sid),
        json!({"jsonrpc":"2.0","id":30,"method":"tools/call","params":{"name":"read_guide","arguments":{"name":"exploration-for-profit"}}}),
    )
    .await;
    let text = guide["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.starts_with("# Playbook"), "{text}");
    let (_, resources) = rpc(
        &url,
        &token,
        Some(&sid),
        json!({"jsonrpc":"2.0","id":31,"method":"resources/list"}),
    )
    .await;
    let uris: Vec<&str> = resources["result"]["resources"]
        .as_array()
        .unwrap_or_else(|| panic!("{resources}"))
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect();
    assert!(uris.contains(&"guide://fuel"), "{uris:?}");
    let (_, heat) = rpc(
        &url,
        &token,
        Some(&sid),
        json!({"jsonrpc":"2.0","id":32,"method":"resources/read","params":{"uri":"guide://heat"}}),
    )
    .await;
    assert_eq!(
        heat["result"]["contents"][0]["mimeType"], "text/markdown",
        "{heat}"
    );
    assert!(
        heat["result"]["contents"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("# Heat")
    );

    let (_, status) = rpc(
        &url,
        &token,
        Some(&sid),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"status","arguments":{}}}),
    )
    .await;
    let text = status["result"]["content"][0]["text"].as_str().unwrap();
    let (_, fleet) = rpc(
        &url, &token, Some(&sid),
        json!({"jsonrpc":"2.0","id":34,"method":"tools/call","params":{"name":"fleet","arguments":{"ship_id":3,"include_modules":true}}}),
    ).await;
    let inventory: Value =
        serde_json::from_str(fleet["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(inventory["commander"], "TESTCMDR");
    assert_eq!(inventory["ships"][0]["ship_id"], 3);
    assert_eq!(inventory["ships"][0]["type"], "type9");
    assert!(inventory["ships"][0]["loadout"]["modules"].is_array());
    let fleet_tool = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "fleet")
        .unwrap();
    assert_eq!(fleet_tool["annotations"]["readOnlyHint"], true);
    assert_eq!(fleet_tool["annotations"]["openWorldHint"], false);
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

    // Invalid planning arguments fail locally, without an external request or game input.
    let (_, invalid) = rpc(
        &url, &token, Some(&sid),
        json!({"jsonrpc":"2.0","id":8,"method":"tools/call","params":{
            "name":"plan_trade_route","arguments":{"system":"Sol","station":"Galileo","cargo_tonnes":0,"budget_credits":100000}
        }}),
    ).await;
    assert_eq!(invalid["result"]["isError"], true, "{invalid}");
    assert!(
        invalid["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("cargo_tonnes")
    );
}

#[tokio::test]
async fn ship_prompt_override_is_read_relative_to_config() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("my-droid.md"),
        "You are K-2, a reprogrammed droid.",
    )
    .unwrap();
    let (url, token) = spawn_with(
        |c| c.ship_prompt = Some("my-droid.md".into()),
        Some(dir.path().join("holtzman.toml")),
    )
    .await;
    let (sid, _) = rpc(
        &url,
        &token,
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18","capabilities":{},
            "clientInfo":{"name":"test","version":"0"}}}),
    )
    .await;
    let sid = sid.expect("session id");
    let (_, prompt) = rpc(
        &url,
        &token,
        Some(&sid),
        json!({"jsonrpc":"2.0","id":2,"method":"prompts/get","params":{"name":"ship_computer"}}),
    )
    .await;
    assert_eq!(
        prompt["result"]["messages"][0]["content"]["text"], "You are K-2, a reprogrammed droid.",
        "{prompt}"
    );
}
