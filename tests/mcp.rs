use apate::test::{ApateTestServer, DEFAULT_PORT};
use apate::{ApateConfig, ApateSpecs};
use serde_json::{Value, json};
use serial_test::serial;

const INIT_DELAY_MS: usize = 1;

fn mcp_url() -> String {
    format!("http://localhost:{DEFAULT_PORT}/mcp")
}

fn default_config() -> ApateConfig {
    ApateConfig {
        specs: ApateSpecs {
            ..Default::default()
        },
        ..Default::default()
    }
}

fn post_mcp(client: &reqwest::blocking::Client, body: Value) -> Value {
    let response = client
        .post(mcp_url())
        .json(&body)
        .send()
        .expect("Request to /mcp failed");
    assert!(
        response.status().is_success(),
        "Expected a successful status, got {}",
        response.status()
    );
    let text = response.text().expect("Failed to read response body");
    if text.is_empty() {
        return Value::Null;
    }
    serde_json::from_str(&text).expect("Response body is not valid JSON")
}

#[test]
#[serial]
fn mcp_initialize_test() {
    let _apate = ApateTestServer::start(default_config(), INIT_DELAY_MS);
    let client = reqwest::blocking::Client::new();

    let value = post_mcp(
        &client,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "example-client", "version": "1.0.0" }
            }
        }),
    );

    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], 1);
    assert!(
        value.get("error").is_none(),
        "initialize must not error: {value}"
    );

    let result = &value["result"];
    assert_eq!(result["protocolVersion"], "2025-06-18");
    assert!(result["capabilities"]["tools"].is_object());
    assert_eq!(result["serverInfo"]["name"], "apate");
    // Version is taken from Cargo.toml at build time.
    let version = result["serverInfo"]["version"].as_str().unwrap_or("");
    assert!(!version.is_empty(), "serverInfo.version must be populated");
}

#[test]
#[serial]
fn mcp_initialize_always_succeeds_test() {
    let _apate = ApateTestServer::start(default_config(), INIT_DELAY_MS);
    let client = reqwest::blocking::Client::new();

    // Even with empty/absent params, initialize must return a success result.
    let value = post_mcp(
        &client,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" }),
    );
    assert!(
        value.get("error").is_none(),
        "initialize must succeed: {value}"
    );
    assert!(value["result"]["serverInfo"]["name"].is_string());
}

#[test]
#[serial]
fn mcp_ping_test() {
    let _apate = ApateTestServer::start(default_config(), INIT_DELAY_MS);
    let client = reqwest::blocking::Client::new();

    let value = post_mcp(
        &client,
        json!({ "jsonrpc": "2.0", "id": 2, "method": "ping" }),
    );
    assert_eq!(value["id"], 2);
    assert!(value.get("error").is_none());
    assert_eq!(value["result"], json!({}));
}

#[test]
#[serial]
fn mcp_tools_list_test() {
    let _apate = ApateTestServer::start(default_config(), INIT_DELAY_MS);
    let client = reqwest::blocking::Client::new();

    let value = post_mcp(
        &client,
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" }),
    );
    assert!(value.get("error").is_none(), "{value}");

    let tools = value["result"]["tools"]
        .as_array()
        .expect("result.tools must be an array");
    assert_eq!(tools.len(), 2, "expected exactly two tools");

    let specs_get = tools
        .iter()
        .find(|t| t["name"] == "specs_get")
        .expect("specs_get tool missing");
    assert!(specs_get["description"].as_str().map(str::is_empty) != Some(true));
    let props = specs_get["inputSchema"]["properties"]
        .as_object()
        .expect("specs_get inputSchema.properties must be an object");
    assert!(props.is_empty(), "specs_get must take no arguments");

    let specs_replace = tools
        .iter()
        .find(|t| t["name"] == "specs_replace")
        .expect("specs_replace tool missing");
    assert!(
        !specs_replace["description"]
            .as_str()
            .unwrap_or("")
            .is_empty()
    );
    let schema = &specs_replace["inputSchema"];
    assert_eq!(schema["type"], "object");
    let required = schema["required"]
        .as_array()
        .expect("specs_replace required array");
    assert!(
        required.iter().any(|v| v == &json!("toml")),
        "specs_replace must require `toml`"
    );
    assert!(schema["properties"]["toml"].is_object());
}

#[test]
#[serial]
fn mcp_specs_get_test() {
    let _apate = ApateTestServer::start(default_config(), INIT_DELAY_MS);
    let client = reqwest::blocking::Client::new();

    let value = post_mcp(
        &client,
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": { "name": "specs_get", "arguments": {} }
        }),
    );
    assert!(value.get("error").is_none(), "{value}");
    assert_eq!(value["result"]["isError"], false);
    let content = value["result"]["content"]
        .as_array()
        .expect("result.content must be an array");
    assert_eq!(content[0]["type"], "text");
    assert!(content[0]["text"].is_string());
}

#[test]
#[serial]
fn mcp_specs_replace_test() {
    let _apate = ApateTestServer::start(default_config(), INIT_DELAY_MS);
    let client = reqwest::blocking::Client::new();

    let new_toml = "\n[[deceit]]\nuris = [\"/ping\"]\n";
    let value = post_mcp(
        &client,
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": { "name": "specs_replace", "arguments": { "toml": new_toml } }
        }),
    );
    assert!(value.get("error").is_none(), "{value}");
    assert_eq!(value["result"]["isError"], false);
    let content = value["result"]["content"]
        .as_array()
        .expect("result.content must be an array");
    let text = content[0]["text"].as_str().expect("stored config text");
    assert!(
        text.contains("/ping"),
        "stored config should round-trip the new deceit: {text}"
    );
}

#[test]
#[serial]
fn mcp_specs_replace_missing_toml_test() {
    let _apate = ApateTestServer::start(default_config(), INIT_DELAY_MS);
    let client = reqwest::blocking::Client::new();

    let value = post_mcp(
        &client,
        json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "tools/call",
            "params": { "name": "specs_replace", "arguments": {} }
        }),
    );
    assert_eq!(
        value["error"]["code"], -32602,
        "missing `toml` must be Invalid params: {value}"
    );
}

#[test]
#[serial]
fn mcp_specs_replace_invalid_toml_test() {
    let _apate = ApateTestServer::start(default_config(), INIT_DELAY_MS);
    let client = reqwest::blocking::Client::new();

    let value = post_mcp(
        &client,
        json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": { "name": "specs_replace", "arguments": { "toml": "not = [valid" } }
        }),
    );
    assert_eq!(
        value["error"]["code"], -32602,
        "invalid toml must be Invalid params: {value}"
    );
}

#[test]
#[serial]
fn mcp_unknown_method_test() {
    let _apate = ApateTestServer::start(default_config(), INIT_DELAY_MS);
    let client = reqwest::blocking::Client::new();

    let value = post_mcp(
        &client,
        json!({ "jsonrpc": "2.0", "id": 8, "method": "foo/bar" }),
    );
    assert_eq!(value["error"]["code"], -32601, "unknown method: {value}");
}

#[test]
#[serial]
fn mcp_invalid_json_test() {
    let _apate = ApateTestServer::start(default_config(), INIT_DELAY_MS);
    let client = reqwest::blocking::Client::new();

    let response = client
        .post(mcp_url())
        .header("Content-Type", "application/json")
        .body("{ not valid json")
        .send()
        .expect("Request to /mcp failed");
    assert_eq!(response.status(), 200);
    let value: Value = response
        .json()
        .expect("invalid JSON must still return a JSON-RPC parse error");
    assert_eq!(value["error"]["code"], -32700, "{value}");
}
