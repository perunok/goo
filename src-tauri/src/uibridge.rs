//! Questions only the window can answer (the note on screen with its unsaved text and selection, whether Mermaid
//! diagrams render), asked from Rust: emit `ui-query {id, kind, args}`, the UI answers with `ui_answer(id, answer)`.
//! Used by MCP tools. Same idea as Slinger's main-to-renderer MCP bridge.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
pub struct UiBridge {
    next: AtomicU64,
    pending: Mutex<HashMap<u64, Sender<Value>>>,
}

/// Asks the window and waits for its answer. The answer is `{ok: value}` or `{error: message}`.
pub fn ask(app: &AppHandle, kind: &str, args: Value, timeout: Duration) -> Result<Value, String> {
    let bridge = app.state::<UiBridge>();
    let id = bridge.next.fetch_add(1, Ordering::SeqCst) + 1;
    let (tx, rx) = mpsc::channel();
    bridge.pending.lock().unwrap().insert(id, tx);
    let sent = app.emit("ui-query", json!({ "id": id, "kind": kind, "args": args }));
    let answer = match sent {
        Ok(()) => rx.recv_timeout(timeout).map_err(|_| "goo's window did not answer (is it open?)".to_string()),
        Err(e) => Err(e.to_string()),
    };
    bridge.pending.lock().unwrap().remove(&id);
    let answer = answer?;
    match answer.get("error").and_then(Value::as_str) {
        Some(e) => Err(e.to_string()),
        None => Ok(answer.get("ok").cloned().unwrap_or(Value::Null)),
    }
}

#[tauri::command]
pub fn ui_answer(id: u64, answer: Value, bridge: State<'_, UiBridge>) {
    if let Some(tx) = bridge.pending.lock().unwrap().remove(&id) {
        let _ = tx.send(answer);
    }
}
