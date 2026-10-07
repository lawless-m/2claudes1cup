use std::io::{self, BufRead, Write};
use std::sync::Arc;

use crate::db::Db;
use crate::mcp::{self, JsonRpcRequest};

pub fn run(db: Arc<Db>) {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("stdin read error: {e}");
                break;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let error =
                    mcp::error_response(serde_json::Value::Null, -32700, &format!("Parse error: {e}"));
                serde_json::to_writer(&mut stdout, &error).unwrap();
                stdout.write_all(b"\n").unwrap();
                stdout.flush().unwrap();
                continue;
            }
        };

        if let Some(response) = mcp::handle_request(&db, request) {
            serde_json::to_writer(&mut stdout, &response).unwrap();
            stdout.write_all(b"\n").unwrap();
            stdout.flush().unwrap();
        }
    }
}
