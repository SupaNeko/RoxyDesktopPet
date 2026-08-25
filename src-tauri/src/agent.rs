//! 旁路任务代理（subagent）：主聊天通过 follow_up_task 委派的复杂任务在此执行。
//!
//! 代理带着当前启用的 MCP 工具跑多轮工具循环，产出文字结论后落库，
//! 并再次触发主聊天把结果转告用户（agent 调用与回复由此进入对话上下文）。

use crate::db::{self, DbState};
use crate::mcp::{self, McpState};
use crate::ModelConfig;
use tauri::{AppHandle, Emitter, Manager};

pub fn schedule(app: AppHandle, run_id: i64, task: String) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_agent_task(&app, run_id, &task).await {
            log_error!("agent task {run_id} failed: {error}");
            let db_state = app.state::<DbState>();
            let conn = db_state.0.lock().await;
            let _ = db::finish_agent_run(&conn, run_id, "error", &error);
        }
    });
}

async fn run_agent_task(app: &AppHandle, run_id: i64, task: &str) -> Result<(), String> {
    log_info!("agent task {run_id} start: {task}");
    let model = app.state::<ModelConfig>();
    if model.api_key.is_empty() {
        return Err("未配置模型 API Key".into());
    }
    let mcp_state = app.state::<McpState>();
    let tools = mcp::available_tools(mcp_state.inner()).await;
    let (search_config, max_rounds) = {
        let db_state = app.state::<DbState>();
        let conn = db_state.0.lock().await;
        let settings = db::get_settings(&conn, true).map_err(|e| e.to_string())?;
        (
            crate::search::SearchConfig::from_settings(&settings),
            settings.agent_max_tool_rounds.clamp(1, 100) as usize,
        )
    };
    if tools.is_empty() && search_config.is_none() {
        return Err("没有可用的 MCP 工具或联网搜索".into());
    }
    let mut tool_json: Vec<serde_json::Value> = tools.iter().map(|t| t.to_openai_tool()).collect();
    let mut summary_lines: Vec<String> = Vec::new();
    if search_config.is_some() {
        tool_json.push(crate::search::web_search_tool_schema());
        summary_lines.push("- web_search：联网搜索，获取天气、新闻、资料等时效性信息".into());
    }
    summary_lines.extend(
        tools
            .iter()
            .map(|t| format!("- {}：{}", t.prefixed_name, t.description)),
    );
    let tool_summary = summary_lines.join("\n");
    let mut messages = vec![
        serde_json::json!({"role":"system","content":format!("你是桌宠洛琪希的后台任务执行代理。当前时间：{}，时区 Asia/Shanghai。你的任务是使用可用工具完成用户委托的事项，并给出简洁准确的文字结论。\n可用工具：\n{}\n规则：优先调用工具获取真实结果，不要编造；工具失败后换一种参数或工具重试，仍失败就在结论里说明失败原因；结论用中文，直接给结果，不要复述任务。", crate::commands::local_time_description(), tool_summary)}),
        serde_json::json!({"role":"user","content":task}),
    ];
    let client = reqwest::Client::new();
    let mut summary = String::new();
    for _ in 0..max_rounds {
        let response = client
            .post(format!("{}/chat/completions", model.base_url))
            .bearer_auth(&model.api_key)
            .json(&serde_json::json!({
                "model": model.model,
                "messages": messages,
                "tools": tool_json,
                "tool_choice": "auto",
                "temperature": 0.2,
                "thinking": {"type":"disabled"}
            }))
            .send()
            .await
            .map_err(|e| format!("代理请求失败：{e}"))?;
        if !response.status().is_success() {
            return Err(format!("代理模型返回 {}", response.status()));
        }
        let body: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
        let message = body
            .pointer("/choices/0/message")
            .cloned()
            .ok_or("代理模型无结果")?;
        let calls = message
            .get("tool_calls")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if calls.is_empty() {
            summary = message
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            break;
        }
        messages.push(message);
        for call in calls {
            let id = call
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let name = call
                .pointer("/function/name")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let args: serde_json::Value = serde_json::from_str(
                call.pointer("/function/arguments")
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}"),
            )
            .unwrap_or(serde_json::json!({}));
            log_info!("agent task {run_id} call: {name}");
            let result = if name == "web_search" {
                let query = args.get("query").and_then(|v| v.as_str()).unwrap_or_default();
                match &search_config {
                    Some(config) => crate::search::web_search(&client, config, query)
                        .await
                        .unwrap_or_else(|error| format!("搜索失败：{error}")),
                    None => "搜索失败：联网搜索未配置".into(),
                }
            } else {
                match mcp::call_tool(mcp_state.inner(), name, args).await {
                    Ok(text) => text,
                    Err(error) => format!("调用失败：{error}"),
                }
            };
            log_info!(
                "agent task {run_id} result: {}",
                result.chars().take(200).collect::<String>()
            );
            messages.push(serde_json::json!({"role":"tool","tool_call_id":id,"content":result}));
        }
    }
    if summary.trim().is_empty() {
        // 轮数耗尽：不带工具再请求一次，强制模型基于已获得的信息给出结论。
        log_warn!("agent task {run_id} reached max tool rounds ({max_rounds}), forcing final summary");
        messages.push(serde_json::json!({"role":"user","content":"工具调用次数已用完。请根据目前已经获得的信息直接给出文字结论；如果工具一直在报错或信息不足，就在结论里如实说明原因，不要再尝试调用工具。"}));
        let response = client
            .post(format!("{}/chat/completions", model.base_url))
            .bearer_auth(&model.api_key)
            .json(&serde_json::json!({
                "model": model.model,
                "messages": messages,
                "temperature": 0.2,
                "thinking": {"type":"disabled"}
            }))
            .send()
            .await;
        if let Ok(response) = response {
            if let Ok(body) = response.json::<serde_json::Value>().await {
                summary = body
                    .pointer("/choices/0/message/content")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .trim()
                    .to_string();
            }
        }
    }
    if summary.trim().is_empty() {
        summary = "（代理未返回文字结论，可能已达到最大工具调用轮数）".into();
    }
    log_info!("agent task {run_id} done: {}", summary.chars().take(120).collect::<String>());
    {
        let db_state = app.state::<DbState>();
        let conn = db_state.0.lock().await;
        db::finish_agent_run(&conn, run_id, "done", &summary).map_err(|e| e.to_string())?;
    }
    // 再次触发主聊天：把任务与结果注入上下文，由主聊天转告用户。
    let event = format!(
        "【后台代理任务完成】你之前委派给后台代理的任务：「{task}」。代理执行结果：{summary}。请把结果自然地转告用户，不要提及“代理”“工具”这类实现细节。"
    );
    let message = crate::commands::generate_scheduled_message(app, "agent_followup", &event).await?;
    let _ = app.emit("assistant-message", &message);
    crate::voice_output::schedule(app.clone(), message);
    Ok(())
}
