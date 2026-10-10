//! MCP over JSON-RPC 2.0, the small part goo needs: `initialize`, `ping`, `tools/list`, `tools/call`. Stateless:
//! every request is answered on its own (the HTTP transport runs in JSON-response mode, no sessions, no SSE).

use crate::tools::{self, Host};
use serde_json::{json, Value};

/// Newest first. The client's requested version is used when goo knows it, otherwise the newest.
pub const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

pub const INSTRUCTIONS: &str = "goo is a local Markdown notes editor. Notes are plain .md files in folders the user \
opened in goo; only those folders are reachable. Start with list_workspaces, then list_notes or search_notes; \
tools use the folder open in goo unless you pass \"workspace\". Paths are relative to the folder (\"projects/plan.md\"). \
Changes appear in the goo window at once. Prefer edit_note for small changes and read a note before rewriting it. \
Notes with unsaved edits in goo are never overwritten: if a write is refused, ask the user to save first. \
Diagrams: goo renders ```mermaid code blocks; check them with check_mermaid after writing. \
\"This note\" or \"the selection\" means get_open_note. If the folder syncs with goo cloud, writes are synced \
automatically; sync_status lists conflicts to resolve. There is no delete tool.";

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;

pub fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub fn tool_list() -> Value {
    let tools: Vec<Value> = tools::tools()
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "title": t.title,
                "description": t.description,
                "inputSchema": (t.input)(),
                "annotations": {
                    "title": t.title,
                    "readOnlyHint": t.read_only,
                    "destructiveHint": t.destructive,
                    "idempotentHint": t.read_only,
                    "openWorldHint": t.open_world
                }
            })
        })
        .collect();
    json!({ "tools": tools })
}

/// Answers one JSON-RPC message (or a batch). None for notifications, which get no answer.
pub fn handle(host: &dyn Host, msg: Value) -> Option<Value> {
    if let Value::Array(batch) = msg {
        let answers: Vec<Value> = batch.into_iter().filter_map(|m| handle(host, m)).collect();
        return (!answers.is_empty()).then_some(Value::Array(answers));
    }
    let Value::Object(m) = &msg else {
        return Some(error_response(Value::Null, INVALID_REQUEST, "expected a JSON-RPC object"));
    };
    let method = m.get("method").and_then(Value::as_str);
    let Some(id) = m.get("id").cloned() else {
        // a notification (`notifications/initialized`, `notifications/cancelled`, ...) or a stray response
        return None;
    };
    let Some(method) = method else {
        return Some(error_response(id, INVALID_REQUEST, "missing method"));
    };
    let params = m.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str);
            let version = asked.filter(|v| PROTOCOL_VERSIONS.contains(v)).unwrap_or(PROTOCOL_VERSIONS[0]);
            ok(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "goo", "title": "goo", "version": crate::app_version() },
                    "instructions": INSTRUCTIONS
                }),
            )
        }
        "ping" => ok(id, json!({})),
        "tools/list" => ok(id, tool_list()),
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(error_response(id, INVALID_PARAMS, "tools/call needs a tool name"));
            };
            if !tools::tools().iter().any(|t| t.name == name) {
                return Some(error_response(id, INVALID_PARAMS, &format!("Unknown tool \"{name}\"")));
            }
            let args = params.get("arguments").cloned().unwrap_or(Value::Null);
            match tools::call(host, name, &args) {
                Ok(out) => {
                    let mut result = json!({ "content": [{ "type": "text", "text": out.text }], "isError": false });
                    if let Some(s) = out.structured {
                        result["structuredContent"] = s;
                    }
                    ok(id, result)
                }
                Err(message) => ok(id, json!({ "content": [{ "type": "text", "text": message }], "isError": true })),
            }
        }
        _ => error_response(id, METHOD_NOT_FOUND, &format!("goo does not support \"{method}\"")),
    })
}
