use std::error::Error;

use actix_web::{
    HttpRequest, HttpResponse, routes,
    web::{Bytes, Data, ServiceConfig},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{ApateSpecs, ApateState};

pub const MCP_API: &str = "/mcp";

const RPC_ERROR_INVALID_REQUEST: isize = -32600;
const RPC_ERROR_INVALID_PARAMS: isize = -32602;

pub fn mcp_service_config(cfg: &mut ServiceConfig) {
    cfg.service(mcp_server);
}

#[routes]
#[post("")]
async fn mcp_server(req: HttpRequest, body: Bytes, state: Data<ApateState>) -> HttpResponse {
    match mcp_server_handler(req, body, state).await {
        Ok(ok) => ok,
        Err(err) => rpc_error_response(None, -32603, format!("Internal error: {err}")),
    }
}

/// Build a JSON-RPC error response (protocol-level failure).
fn rpc_error_response(id: Option<RpcId>, code: isize, message: String) -> HttpResponse {
    let payload = JsonRpcErrorPayload {
        code,
        message,
        data: None,
    };
    let envelope = JsonRpcError {
        jsonrpc: "2.0".to_string(),
        id,
        error: payload,
    };
    let body = serde_json::to_string(&envelope).expect("JsonRpcError is always serializable");
    HttpResponse::Ok()
        .insert_header(("Content-Type", "application/json"))
        .body(body)
}

/// Wrap a typed, serializable result in a JSON-RPC success response.
fn rpc_success_response<T: Serialize>(id: Option<RpcId>, result: T) -> HttpResponse {
    let envelope = JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(result),
    };
    let body = serde_json::to_string(&envelope).expect("success envelope is always serializable");
    HttpResponse::Ok()
        .insert_header(("Content-Type", "application/json"))
        .body(body)
}

async fn mcp_server_handler(
    _req: HttpRequest,
    body: Bytes,
    state: Data<ApateState>,
) -> Result<HttpResponse, Box<dyn Error>> {
    // Body that does not parse is a Parse error (-32700), per spec §14/§15.
    let raw: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return Ok(rpc_error_response(
                None,
                -32700,
                format!("Parse error: invalid JSON ({e})"),
            ));
        }
    };

    // Batch requests (a JSON array) are not supported (spec §4).
    if raw.is_array() {
        return Ok(rpc_error_response(
            None,
            RPC_ERROR_INVALID_REQUEST,
            "Invalid Request: batch requests are not supported".to_string(),
        ));
    }

    let msg: JsonRpcRequest<Value> = match serde_json::from_value(raw) {
        Ok(m) => m,
        Err(e) => {
            return Ok(rpc_error_response(
                None,
                RPC_ERROR_INVALID_REQUEST,
                format!("Invalid Request: {e}"),
            ));
        }
    };
    let id = msg.id.clone();

    // Spec §5: the `jsonrpc` field must be exactly "2.0".
    if msg.jsonrpc != "2.0" {
        return Ok(rpc_error_response(
            id,
            RPC_ERROR_INVALID_REQUEST,
            "Invalid Request: jsonrpc must be \"2.0\"".to_string(),
        ));
    }

    match msg.method.as_str() {
        "initialize" => Ok(rpc_success_response(id, mcp_initialize())),
        "notifications/initialized" => Ok(HttpResponse::Accepted().finish()),
        "ping" => Ok(rpc_success_response(
            id,
            Value::Object(serde_json::Map::new()),
        )),
        "tools/list" => Ok(rpc_success_response(id, mcp_tools_list())),
        "tools/call" => Ok(mcp_dispatch_tools_call(id, msg.params, state).await?),
        // Notification: no JSON-RPC response, return empty 202.
        _ => Ok(rpc_error_response(
            id,
            -32601,
            format!("Method not found: {}", msg.method),
        )),
    }
}

/// Bind a present `params` value to the concrete request type.
fn bind_params<T: serde::de::DeserializeOwned>(
    params: Option<Value>,
    method: &str,
) -> Result<T, String> {
    let value = params.ok_or_else(|| format!("params is required for {method}"))?;
    serde_json::from_value(value).map_err(|e| format!("Invalid params for {method}: {e}"))
}

/// Convert a param-binding failure into a JSON-RPC "Invalid params" response.
fn rpc_invalid_params(id: Option<RpcId>, message: String) -> HttpResponse {
    rpc_error_response(id, RPC_ERROR_INVALID_PARAMS, message)
}

async fn mcp_dispatch_tools_call(
    id: Option<RpcId>,
    params: Option<Value>,
    state: Data<ApateState>,
) -> Result<HttpResponse, Box<dyn Error>> {
    let params = match bind_params::<McpToolParams<Value>>(params, "tools/call") {
        Ok(p) => p,
        Err(msg) => return Ok(rpc_invalid_params(id, msg)),
    };
    let result = match mcp_tools_call(params, state).await {
        Ok(r) => r,
        Err(e) => return Ok(rpc_error_response(id, e.code, e.message)),
    };
    Ok(rpc_success_response(id, result))
}

fn mcp_initialize() -> Value {
    json!({
        "protocolVersion": "2025-06-18",
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "apate",
            "version": env!("CARGO_PKG_VERSION"),
            "title": "Apate API mocking server",
            "websiteUrl": "https://github.com/rustrum/apate",
            "description": "Allows you to change API mocking serice specification on fly."
        },
       "instructions": "TOML specs documentation available here: https://raw.githubusercontent.com/rustrum/apate/refs/heads/main/README-AI.md"
    })
}

/// `tools/list`: advertise the two tools with a JSON-Schema `inputSchema` so
/// other LLMs can discover how to call them.
fn mcp_tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "specs_get",
                "description": "Returns the current active specirication as TOML text.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }
            },
            {
                "name": "specs_replace",
                "description": "Replaces the configuration TOML with the provided text input and returns the stored configuration TOML.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "toml": {
                            "type": "string",
                            "description": "Full TOML configuration as text."
                        }
                    },
                    "required": ["toml"],
                    "additionalProperties": false
                }
            }
        ]
    })
}

async fn mcp_tools_call(
    params: McpToolParams<Value>,
    state: Data<ApateState>,
) -> Result<McpToolResult, McpError> {
    match params.name.as_str() {
        "specs_get" => Ok(tool_specs_get(state).await),
        "specs_replace" => {
            let toml_text = params
                .arguments
                .get("toml")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| McpError {
                    code: RPC_ERROR_INVALID_PARAMS,
                    message: "Invalid params: toml is required".to_string(),
                })?;
            tools_specs_replace(toml_text, state).await
        }
        _ => Err(McpError {
            code: RPC_ERROR_INVALID_PARAMS,
            message: format!("Invalid params: unknown tool name {}", params.name),
        }),
    }
}

/// MCP copy of `specification_get` (admin.rs): read the specs and serialize to TOML.
async fn tool_specs_get(state: Data<ApateState>) -> McpToolResult {
    let specs = state.specs.read().await;
    match toml::to_string(&*specs) {
        Ok(toml) => McpToolResult {
            is_error: false,
            content: vec![McpToolContent::Text { text: toml }],
        },
        Err(err) => McpToolResult {
            is_error: true,
            content: vec![McpToolContent::Text {
                text: format!("Failed to serialize configuration: {err}"),
            }],
        },
    }
}

/// MCP copy of `specification_replace` (admin.rs): parse the TOML config,
/// replace the stored specs, then clear caches. Returns the stored config text.
///
/// A malformed TOML value is an invalid parameter, so it is surfaced as a
/// protocol-level `Invalid params` error (spec §14/§15) rather than a
/// tool-execution `isError` result.
async fn tools_specs_replace(
    toml_text: String,
    state: Data<ApateState>,
) -> Result<McpToolResult, McpError> {
    let new_specs = toml::from_str::<ApateSpecs>(&toml_text).map_err(|err| McpError {
        code: RPC_ERROR_INVALID_PARAMS,
        message: format!("Invalid params: configuration TOML is not valid: {err}"),
    })?;

    let mut specs = state.specs.write().await;
    *specs = new_specs;

    state.clear_cache();
    state.rhai.clear_and_update(specs.rhai.clone());

    let stored = toml::to_string(&*specs).unwrap_or_default();
    Ok(McpToolResult {
        is_error: false,
        content: vec![McpToolContent::Text { text: stored }],
    })
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum RpcId {
    Number(i64),
    String(String),
    Null,
}

/// JSON-RPC 2.0 request envelope.
#[derive(Deserialize)]
struct JsonRpcRequest<T> {
    jsonrpc: String,
    id: Option<RpcId>,
    method: String,
    params: Option<T>,
}

#[derive(Serialize)]
struct JsonRpcResponse<T> {
    jsonrpc: String,
    id: Option<RpcId>,
    result: Option<T>,
}

#[derive(Serialize)]
struct JsonRpcError {
    jsonrpc: String,
    id: Option<RpcId>,
    error: JsonRpcErrorPayload,
}

#[derive(Serialize)]
struct JsonRpcErrorPayload {
    code: isize,
    message: String,
    data: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct McpToolResult {
    is_error: bool,
    content: Vec<McpToolContent>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum McpToolContent {
    Text { text: String },
    // Image(data: String, mime_type: String),
    // Resource should be added later
}

#[derive(Deserialize)]
struct McpToolParams<T> {
    name: String,
    arguments: T,
}

#[derive(Debug)]
struct McpError {
    code: isize,
    message: String,
}
