/// WebSocket handler for real-time query execution

use axum::extract::ws::{Message, WebSocket};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

use super::server::AppState;
use kstone_api::KeystoneValue;

/// WebSocket message types
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    Execute { id: String, query: String },
    Cancel { id: String },
    Ping,
}

/// WebSocket response types
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum WsResponse {
    Result {
        id: String,
        rows: Vec<HashMap<String, serde_json::Value>>,
        execution_time_ms: u64,
        row_count: usize,
    },
    Error {
        id: String,
        message: String,
    },
    Progress {
        id: String,
        message: String,
    },
    Pong,
}

/// Handle WebSocket connection
pub async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Spawn a task to handle incoming messages
    tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    // Parse the message
                    match serde_json::from_str::<WsMessage>(&text) {
                        Ok(ws_msg) => {
                            let response = handle_message(ws_msg, &state).await;

                            // Send response
                            if let Ok(response_text) = serde_json::to_string(&response) {
                                if sender.send(Message::Text(response_text)).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            let error = WsResponse::Error {
                                id: "unknown".to_string(),
                                message: format!("Invalid message: {}", e),
                            };

                            if let Ok(error_text) = serde_json::to_string(&error) {
                                if sender.send(Message::Text(error_text)).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                }
                Ok(Message::Ping(ping)) => {
                    if sender.send(Message::Pong(ping)).await.is_err() {
                        break;
                    }
                }
                Ok(Message::Close(_)) => {
                    break;
                }
                _ => {}
            }
        }
    });
}

/// Handle individual WebSocket messages
async fn handle_message(msg: WsMessage, state: &AppState) -> WsResponse {
    match msg {
        WsMessage::Execute { id, query } => {
            execute_query(id, query, state).await
        }
        WsMessage::Cancel { id } => {
            // Query cancellation not yet implemented
            WsResponse::Error {
                id,
                message: "Query cancellation not implemented".to_string(),
            }
        }
        WsMessage::Ping => WsResponse::Pong,
    }
}

/// Execute a query and return results
async fn execute_query(id: String, query: String, state: &AppState) -> WsResponse {
    let start = std::time::Instant::now();

    // Execute the query
    match state.db.execute_statement(&query) {
        Ok(response) => {
            let execution_time_ms = start.elapsed().as_millis() as u64;

            // Handle different response types
            match response {
                kstone_api::ExecuteStatementResponse::Select { items, .. } => {
                    // Convert items to JSON-compatible format
                    let rows: Vec<HashMap<String, serde_json::Value>> = items
                        .iter()
                        .map(|item| {
                            let mut row = HashMap::new();
                            for (key, value) in item {
                                row.insert(key.clone(), keystone_value_to_json(value));
                            }
                            row
                        })
                        .collect();

                    WsResponse::Result {
                        id,
                        rows: rows.clone(),
                        execution_time_ms,
                        row_count: rows.len(),
                    }
                }
                kstone_api::ExecuteStatementResponse::Insert { success } => {
                    if success {
                        WsResponse::Result {
                            id,
                            rows: vec![],
                            execution_time_ms,
                            row_count: 1,
                        }
                    } else {
                        WsResponse::Error {
                            id,
                            message: "Insert failed".to_string(),
                        }
                    }
                }
                kstone_api::ExecuteStatementResponse::Update { .. } => {
                    WsResponse::Result {
                        id,
                        rows: vec![],
                        execution_time_ms,
                        row_count: 1,
                    }
                }
                kstone_api::ExecuteStatementResponse::Delete { success } => {
                    if success {
                        WsResponse::Result {
                            id,
                            rows: vec![],
                            execution_time_ms,
                            row_count: 1,
                        }
                    } else {
                        WsResponse::Error {
                            id,
                            message: "Delete failed".to_string(),
                        }
                    }
                }
            }
        }
        Err(e) => WsResponse::Error {
            id,
            message: e.to_string(),
        },
    }
}

/// Convert KeystoneValue to JSON
fn keystone_value_to_json(value: &KeystoneValue) -> serde_json::Value {
    match value {
        KeystoneValue::S(s) => json!(s),
        KeystoneValue::N(n) => {
            // Try to parse as number, otherwise as string
            n.parse::<f64>()
                .map(|v| json!(v))
                .unwrap_or_else(|_| json!(n))
        }
        KeystoneValue::B(bytes) => {
            use base64::Engine;
            json!(base64::engine::general_purpose::STANDARD.encode(bytes))
        }
        KeystoneValue::Bool(b) => json!(b),
        KeystoneValue::Null => json!(null),
        KeystoneValue::L(list) => {
            json!(list.iter().map(|v| value_to_json(v)).collect::<Vec<_>>())
        }
        KeystoneValue::M(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                obj.insert(k.clone(), keystone_value_to_json(v));
            }
            json!(obj)
        }
        KeystoneValue::VecF32(vec) => json!(vec),
        KeystoneValue::Ts(ts) => json!(ts),
    }
}

/// Convert Value to JSON (for lists)
fn value_to_json(value: &kstone_core::Value) -> serde_json::Value {
    use kstone_core::Value;

    match value {
        Value::S(s) => json!(s),
        Value::N(n) => {
            n.parse::<f64>()
                .map(|v| json!(v))
                .unwrap_or_else(|_| json!(n))
        }
        Value::B(bytes) => {
            use base64::Engine;
            json!(base64::engine::general_purpose::STANDARD.encode(bytes))
        }
        Value::Bool(b) => json!(b),
        Value::Null => json!(null),
        Value::L(list) => {
            json!(list.iter().map(|v| value_to_json(v)).collect::<Vec<_>>())
        }
        Value::M(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                obj.insert(k.clone(), keystone_value_to_json(v));
            }
            json!(obj)
        }
        Value::VecF32(vec) => json!(vec),
        Value::Ts(ts) => json!(ts),
    }
}

