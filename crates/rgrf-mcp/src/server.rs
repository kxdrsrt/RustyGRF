//! MCP over stdio: newline-delimited JSON-RPC 2.0, tools only.
//! Spec: https://modelcontextprotocol.io/specification/2025-06-18 (transports, lifecycle, tools).

use std::io::{BufRead, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rgrf_core::progress::{CancelToken, Control, Progress};
use serde_json::{Value, json};

use crate::params::{Args, schema};
use crate::policy::Policy;
use crate::tools::{Ctx, TOOLS};

pub const SERVER_NAME: &str = "rgrf-mcp";
/// Newest first; a client asking for an older one that we know gets exactly that.
const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

pub type Emit<'a> = &'a (dyn Fn(Value) + Sync);

pub struct Server {
    policy: Policy,
}

fn error(id: &Value, code: i64, message: impl Into<String>) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message.into()}})
}

fn ok(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

impl Server {
    pub fn new(policy: Policy) -> Self {
        Self { policy }
    }

    fn instructions(&self) -> String {
        let mut s = String::from(
            "RustyGRF inspects and builds Ragnarok Online GRF archives. Archive names are bytes shown in \
             cp1252 by default (`names` argument: cp949 for Korean). Entry paths are matched \
             case-insensitively with / equal to \\. Large results are paged or cut (`truncated`). ",
        );
        if self.policy.write_enabled() {
            s.push_str("Writing tools (extract, build, repack, merge, patch) work only inside the folders the server was started with, and never replace existing files unless overwrite=true.");
        } else {
            s.push_str("The server is read-only: the tools that write files are not offered.");
        }
        s
    }

    fn tool_list(&self) -> Value {
        let tools: Vec<Value> = TOOLS
            .iter()
            .filter(|t| !t.writes || self.policy.write_enabled())
            .map(|t| {
                json!({
                    "name": t.name, "title": t.title, "description": t.description,
                    "inputSchema": schema(t.params),
                    "annotations": {
                        "readOnlyHint": !t.writes,
                        "destructiveHint": t.writes,
                        "idempotentHint": !t.writes,
                        "openWorldHint": false,
                    },
                })
            })
            .collect();
        json!({"tools": tools})
    }

    fn call_tool(
        &self,
        params: &Value,
        cancel: &CancelToken,
        emit: Emit,
    ) -> Result<Option<Value>, (i64, String)> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or((INVALID_PARAMS, "missing tool name".to_string()))?;
        let tool = TOOLS
            .iter()
            .find(|t| t.name == name && (!t.writes || self.policy.write_enabled()))
            .ok_or_else(|| (INVALID_PARAMS, format!("unknown tool '{name}'")))?;
        let text_result =
            |text: String| json!({"content": [{"type": "text", "text": text}], "isError": true});
        let args = match Args::validate(tool.params, params.get("arguments")) {
            Ok(a) => a,
            Err(m) => return Ok(Some(text_result(m))),
        };

        let token = params.pointer("/_meta/progressToken").cloned();
        let last = Mutex::new(Instant::now());
        let progress = |p: Progress| {
            let Some(token) = &token else { return };
            let Ok(mut at) = last.lock() else { return };
            if at.elapsed() < Duration::from_millis(500) && p.done < p.total {
                return;
            }
            *at = Instant::now();
            emit(json!({
                "jsonrpc": "2.0", "method": "notifications/progress",
                "params": {"progressToken": token, "progress": p.done, "total": p.total},
            }));
        };
        let ctx = Ctx {
            policy: &self.policy,
            control: Control {
                cancel: Some(cancel),
                progress: Some(&progress),
            },
        };
        let outcome = catch_unwind(AssertUnwindSafe(|| (tool.run)(&ctx, &args)))
            .unwrap_or_else(|_| Err("internal error in the tool".to_string()));
        if cancel.is_cancelled() {
            // a cancelled request gets no response (spec: cancellation)
            return Ok(None);
        }
        Ok(Some(match outcome {
            Ok(v) => json!({
                "content": [{"type": "text", "text": v.to_string()}],
                "structuredContent": v,
                "isError": false,
            }),
            Err(m) => text_result(m),
        }))
    }

    /// Handles one JSON-RPC message; `None` for notifications and cancelled calls.
    pub fn handle(&self, msg: &Value, cancel: &CancelToken, emit: Emit) -> Option<Value> {
        let Some(obj) = msg.as_object() else {
            return Some(error(
                &Value::Null,
                INVALID_REQUEST,
                "a message must be a JSON object",
            ));
        };
        let Some(method) = obj.get("method").and_then(Value::as_str) else {
            // a response to a request of ours; we send none
            return obj
                .get("id")
                .is_none()
                .then(|| error(&Value::Null, INVALID_REQUEST, "missing method"));
        };
        let id = obj.get("id")?; // notifications (no id) never get a reply
        let params = obj.get("params").unwrap_or(&Value::Null);
        match method {
            "initialize" => {
                let wanted = params.get("protocolVersion").and_then(Value::as_str);
                let version = PROTOCOL_VERSIONS
                    .iter()
                    .find(|v| Some(**v) == wanted)
                    .unwrap_or(&PROTOCOL_VERSIONS[0]);
                Some(ok(
                    id,
                    json!({
                        "protocolVersion": version,
                        "capabilities": {"tools": {"listChanged": false}},
                        "serverInfo": {"name": SERVER_NAME, "title": "RustyGRF", "version": rgrf_core::VERSION},
                        "instructions": self.instructions(),
                    }),
                ))
            }
            "ping" => Some(ok(id, json!({}))),
            "tools/list" => Some(ok(id, self.tool_list())),
            "tools/call" => match self.call_tool(params, cancel, emit) {
                Ok(Some(r)) => Some(ok(id, r)),
                Ok(None) => None,
                Err((code, m)) => Some(error(id, code, m)),
            },
            other => Some(error(
                id,
                METHOD_NOT_FOUND,
                format!("method '{other}' is not supported"),
            )),
        }
    }
}

type Shared<T> = Arc<Mutex<T>>;

/// Serves until the input ends. A reader thread watches for `notifications/cancelled` so a long
/// call can be stopped while it runs.
pub fn serve<R, W>(server: &Server, input: R, output: W) -> std::io::Result<()>
where
    R: BufRead + Send + 'static,
    W: Write + Send + 'static,
{
    let out: Shared<W> = Arc::new(Mutex::new(output));
    let emit = {
        let out = Arc::clone(&out);
        move |v: Value| {
            if let Ok(mut w) = out.lock() {
                // a closed pipe ends the session through the reader; nothing to do here
                let _ = writeln!(w, "{v}").and_then(|()| w.flush());
            }
        }
    };
    let current: Shared<Option<(Value, CancelToken)>> = Arc::new(Mutex::new(None));
    let (tx, rx) = mpsc::channel::<Value>();

    let reader = {
        let (current, emit_out) = (Arc::clone(&current), Arc::clone(&out));
        std::thread::spawn(move || {
            for line in input.lines() {
                let Ok(line) = line else { break };
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<Value>(&line) {
                    Err(e) => {
                        if let Ok(mut w) = emit_out.lock() {
                            let _ =
                                writeln!(w, "{}", error(&Value::Null, PARSE_ERROR, e.to_string()))
                                    .and_then(|()| w.flush());
                        }
                    }
                    Ok(msg)
                        if msg.get("method").and_then(Value::as_str)
                            == Some("notifications/cancelled") =>
                    {
                        let id = msg.pointer("/params/requestId");
                        if let Ok(cur) = current.lock()
                            && let Some((cid, token)) = cur.as_ref()
                            && Some(cid) == id
                        {
                            token.cancel();
                        }
                    }
                    Ok(msg) => {
                        if tx.send(msg).is_err() {
                            break;
                        }
                    }
                }
            }
        })
    };

    for msg in rx {
        let batch = msg.is_array();
        let items = match msg {
            Value::Array(items) => items,
            single => vec![single],
        };
        let mut replies = Vec::new();
        for item in &items {
            let token = CancelToken::new();
            if let Some(id) = item.get("id")
                && let Ok(mut c) = current.lock()
            {
                *c = Some((id.clone(), token.clone()));
            }
            if let Some(r) = server.handle(item, &token, &emit) {
                replies.push(r);
            }
            if let Ok(mut c) = current.lock() {
                *c = None;
            }
        }
        if batch {
            if !replies.is_empty() {
                emit(Value::Array(replies));
            }
        } else {
            for r in replies {
                emit(r);
            }
        }
    }
    let _ = reader.join();
    Ok(())
}
