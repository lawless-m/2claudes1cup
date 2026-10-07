use crate::db::Db;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

pub fn error_response(id: Value, code: i32, message: &str) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.to_string(),
        }),
    }
}

pub fn handle_request(db: &Db, req: JsonRpcRequest) -> Option<JsonRpcResponse> {
    let id = match req.id.clone() {
        Some(id) => id,
        None => return None,
    };

    let result = match req.method.as_str() {
        "initialize" => handle_initialize(),
        "ping" => Ok(serde_json::json!({})),
        "tools/list" => Ok(tools_list()),
        "tools/call" => handle_tools_call(db, req.params),
        _ => Err((-32601, format!("Method not found: {}", req.method))),
    };

    Some(match result {
        Ok(value) => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: Some(value),
            error: None,
        },
        Err((code, msg)) => error_response(id, code, &msg),
    })
}

fn handle_initialize() -> Result<Value, (i32, String)> {
    Ok(serde_json::json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "c2c-server",
            "version": "0.1.0"
        }
    }))
}

fn tools_list() -> Value {
    serde_json::json!({
        "tools": [
            {
                "name": "send_message",
                "description": "Send a message to another peer.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "from": {"type": "string", "description": "Sender's peer name"},
                        "to": {"type": "string", "description": "Recipient's peer name"},
                        "body": {"type": "string", "description": "The message content"}
                    },
                    "required": ["from", "to", "body"]
                }
            },
            {
                "name": "check_messages",
                "description": "Check for messages addressed to you.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "for": {"type": "string", "description": "Your peer name"},
                        "status": {
                            "type": "string",
                            "description": "Filter: pending, answered, or all. Default: pending",
                            "enum": ["pending", "answered", "all"]
                        }
                    },
                    "required": ["for"]
                }
            },
            {
                "name": "reply_to_message",
                "description": "Reply to a specific message.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "from": {"type": "string", "description": "Your peer name"},
                        "message_id": {"type": "string", "description": "The message being replied to"},
                        "body": {"type": "string", "description": "The reply content"}
                    },
                    "required": ["from", "message_id", "body"]
                }
            },
            {
                "name": "list_messages",
                "description": "Browse message history.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "from": {"type": "string", "description": "Filter by sender"},
                        "to": {"type": "string", "description": "Filter by recipient"},
                        "limit": {"type": "integer", "description": "Max messages to return. Default: 20"}
                    }
                }
            },
            {
                "name": "purge_messages",
                "description": "Clean up old messages.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "older_than": {"type": "integer", "description": "Delete messages older than N hours. Default: 72"},
                        "status": {"type": "string", "description": "Only purge messages with this status. Default: all"}
                    }
                }
            }
        ]
    })
}

fn handle_tools_call(db: &Db, params: Option<Value>) -> Result<Value, (i32, String)> {
    let params = params.ok_or((-32602, "Missing params".to_string()))?;
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or((-32602, "Missing tool name".to_string()))?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    let result = match name {
        "send_message" => tool_send_message(db, &args),
        "check_messages" => tool_check_messages(db, &args),
        "reply_to_message" => tool_reply_to_message(db, &args),
        "list_messages" => tool_list_messages(db, &args),
        "purge_messages" => tool_purge_messages(db, &args),
        _ => return Err((-32602, format!("Unknown tool: {name}"))),
    };

    match result {
        Ok(value) => Ok(serde_json::json!({
            "content": [{"type": "text", "text": value.to_string()}]
        })),
        Err(e) => Ok(serde_json::json!({
            "content": [{"type": "text", "text": e}],
            "isError": true
        })),
    }
}

fn get_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Missing required parameter: {key}"))
}

fn tool_send_message(db: &Db, args: &Value) -> Result<Value, String> {
    let from = get_str(args, "from")?;
    let to = get_str(args, "to")?;
    let body = get_str(args, "body")?;
    let msg = db.send_message(from, to, body).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "message_id": msg.id,
        "status": msg.status,
        "created_at": msg.created_at,
    }))
}

fn tool_check_messages(db: &Db, args: &Value) -> Result<Value, String> {
    let for_peer = get_str(args, "for")?;
    let status = args.get("status").and_then(|v| v.as_str());
    let messages = db
        .check_messages(for_peer, status)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({"messages": messages}))
}

fn tool_reply_to_message(db: &Db, args: &Value) -> Result<Value, String> {
    let from = get_str(args, "from")?;
    let message_id = get_str(args, "message_id")?;
    let body = get_str(args, "body")?;
    let msg = db
        .reply_to_message(message_id, from, body)
        .map_err(|e| e.to_string())?;
    match msg {
        Some(m) => Ok(serde_json::json!({
            "message_id": m.id,
            "status": m.status,
            "reply_body": m.reply_body,
            "replied_at": m.replied_at,
        })),
        None => Err(format!(
            "Message {message_id} not found or not addressed to {from}"
        )),
    }
}

fn tool_list_messages(db: &Db, args: &Value) -> Result<Value, String> {
    let from = args.get("from").and_then(|v| v.as_str());
    let to = args.get("to").and_then(|v| v.as_str());
    let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);
    let messages = db
        .list_messages(from, to, limit)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!(messages))
}

fn tool_purge_messages(db: &Db, args: &Value) -> Result<Value, String> {
    let older_than = args
        .get("older_than")
        .and_then(|v| v.as_i64())
        .unwrap_or(72);
    let status = args.get("status").and_then(|v| v.as_str());
    let count = db
        .purge_messages(older_than, status)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({"deleted_count": count}))
}
