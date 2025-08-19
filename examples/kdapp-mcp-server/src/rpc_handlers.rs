use std::sync::Arc;

use crate::jsonrpc::{Request, Response};
use crate::state::ServerState;

pub async fn handle_tools_list(request: Request, _state: Arc<ServerState>) -> Response {
    let tools = vec![
        serde_json::json!({
            "name": "kdapp_start_episode",
            "description": "Start a new episode with the given participants",
            "inputSchema": {"type": "object", "properties": {"participants": {"type":"array","items":{"type":"string"}}}, "required": ["participants"]}
        }),
        serde_json::json!({
            "name": "kdapp_execute_command",
            "description": "Execute a command in the specified episode",
            "inputSchema": {"type":"object","properties": {"episode_id":{"type":"string"},"command":{"type":"object"},"signature":{"type":"string"},"signer":{"type":"string"}},"required":["episode_id","command"]}
        }),
        serde_json::json!({
            "name": "kdapp_get_episode_state",
            "description": "Get the state of the specified episode",
            "inputSchema": {"type":"object","properties": {"episode_id":{"type":"string"}},"required":["episode_id"]}
        }),
        serde_json::json!({
            "name": "kdapp_generate_transaction",
            "description": "Generate a transaction from the given command",
            "inputSchema": {"type":"object","properties": {"command":{"type":"object"}},"required":["command"]}
        }),
        serde_json::json!({
            "name": "kdapp_get_agent_pubkeys",
            "description": "Return the server agent public keys (hex)",
            "inputSchema": {"type":"object","properties": {}}
        })
    ];
    Response::success(request.id, serde_json::Value::Array(tools))
}

pub async fn handle_tools_call(request: Request, state: Arc<ServerState>) -> Response {
    let params = match request.params { Some(p) => p, None => { return Response::error(request.id, -32602, "Invalid params".to_string(), Some(serde_json::json!("Missing params"))); } };
    let tool_name = match params.get("name").and_then(|n| n.as_str()) { Some(n) => n, None => { return Response::error(request.id, -32602, "Invalid params".to_string(), Some(serde_json::json!("Missing tool name"))); } };
    let empty = serde_json::json!({});
    let arguments = params.get("arguments").unwrap_or(&empty);

    let result = match tool_name {
        "kdapp_start_episode" => {
            let participants: Vec<String> = arguments.get("participants").and_then(|a| a.as_array()).unwrap_or(&vec![]).iter().map(|p| p.as_str().unwrap_or("").to_string()).collect();
            match crate::tools::start_episode(state.clone(), participants).await { Ok(id) => serde_json::json!(id), Err(e) => { return Response::error(request.id, -32000, "Tool execution error".to_string(), Some(serde_json::json!(e.to_string()))); } }
        }
        ,"kdapp_execute_command" => {
            let episode_id = arguments.get("episode_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let command = arguments.get("command").cloned().unwrap_or(serde_json::json!({}));
            let signature = arguments.get("signature").and_then(|v| v.as_str()).map(|s| s.to_string());
            let signer = arguments.get("signer").and_then(|v| v.as_str()).map(|s| s.to_string());
            match crate::tools::execute_command(state.clone(), episode_id, command, signature, signer).await { Ok(v) => v, Err(e) => { return Response::error(request.id, -32000, "Tool execution error".to_string(), Some(serde_json::json!(e.to_string()))); } }
        }
        ,"kdapp_get_episode_state" => {
            let episode_id = arguments.get("episode_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            match crate::tools::get_episode_state(state.clone(), episode_id).await { Ok(v) => v, Err(e) => { return Response::error(request.id, -32000, "Tool execution error".to_string(), Some(serde_json::json!(e.to_string()))); } }
        }
        ,"kdapp_generate_transaction" => {
            let command = arguments.get("command").cloned().unwrap_or(serde_json::json!({}));
            match crate::tools::generate_transaction(state.clone(), command).await { Ok(v) => v, Err(e) => { return Response::error(request.id, -32000, "Tool execution error".to_string(), Some(serde_json::json!(e.to_string()))); } }
        }
        ,"kdapp_get_agent_pubkeys" => {
            match crate::tools::get_agent_pubkeys(state.clone()).await { Ok(v) => v, Err(e) => { return Response::error(request.id, -32000, "Tool execution error".to_string(), Some(serde_json::json!(e.to_string()))); } }
        }
        , _ => { return Response::error(request.id, -32601, "Tool not found".to_string(), None); }
    };

    Response::success(request.id, result)
}
