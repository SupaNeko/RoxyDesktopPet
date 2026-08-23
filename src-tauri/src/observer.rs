use crate::db::{self, DbState};
use crate::{ConversationState, MemoryEnvConfig, ModelConfig};
use serde::Deserialize;
use tauri::{AppHandle, Manager};

#[derive(Debug, Deserialize)]
struct Observation {
    text: String,
    memory_type: String,
    importance: f64,
}

#[derive(Debug, Deserialize)]
struct ObservationResult {
    memories: Vec<Observation>,
}

pub fn schedule(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        log_info!("memory observer scheduled");
        if let Err(error) = run(&app).await {
            log_error!("memory observer skipped: {error}");
        }
    });
}

async fn run(app: &AppHandle) -> Result<(), String> {
    let db_state = app.state::<DbState>();
    let model = app.state::<ModelConfig>();
    let memory_env = app.state::<MemoryEnvConfig>();
    let conversation = app.state::<ConversationState>();
    let _guard = conversation.0.lock().await;

    let (mut settings, batch) = {
        let conn = db_state.0.lock().await;
        let settings =
            db::get_settings(&conn, !model.api_key.is_empty()).map_err(|e| e.to_string())?;
        let batch = db::observer_message_batch(&conn, settings.memory_observer_interval.max(2))
            .map_err(|e| e.to_string())?;
        (settings, batch)
    };

    if batch.is_empty() {
        return Ok(());
    }
    log_info!("memory observer processing {} messages", batch.len());
    if !settings.memory_observer_enabled {
        let conn = db_state.0.lock().await;
        db::advance_memory_observer(&conn, batch.last().unwrap().0).map_err(|e| e.to_string())?;
        return Ok(());
    }
    let interval = settings.memory_observer_interval.clamp(2, 500) as usize;
    if batch.len() < interval {
        return Ok(());
    }
    if settings.qdrant_url.is_empty() {
        settings.qdrant_url = memory_env.qdrant_url.clone();
    }
    if settings.embedding_base_url.is_empty() {
        settings.embedding_base_url = memory_env.embedding_base_url.clone();
    }
    if settings.embedding_model.is_empty() {
        settings.embedding_model = memory_env.embedding_model.clone();
    }
    if settings.embedding_api_key.is_empty() {
        settings.embedding_api_key = memory_env.embedding_api_key.clone();
    }
    if settings.embedding_dimension == 0 {
        settings.embedding_dimension = memory_env.embedding_dimension;
    }
    let config = crate::commands::memory_config(&settings);
    if model.api_key.is_empty() || !config.is_complete() {
        return Ok(());
    }

    let existing = {
        let conn = db_state.0.lock().await;
        db::list_memories(&conn)
            .map_err(|e| e.to_string())?
            .into_iter()
            .take(80)
            .map(|m| format!("- {}", m.text))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let transcript = batch
        .iter()
        .map(|(_, m)| {
            let role = if m.role == "user" { "用户" } else { "桌宠" };
            format!("{role}：{}", m.content)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let prompt = format!(
        r#"分析下面一段桌宠对话，只提取未来对话仍有用、由用户明确表达或能可靠推断的长期信息。
允许类型：fact、preference、habit、relationship、experience。
不要保存临时请求、提醒事项、寒暄、桌宠自己说的话、敏感凭据，也不要重复已有记忆。
将每条记忆改写成独立、简洁、无歧义的中文事实，主语明确。没有值得保存的内容时返回空数组。
严格返回 JSON：{{"memories":[{{"text":"...","memory_type":"preference","importance":0.7}}]}}

已有记忆：
{}

待观察对话：
{}"#,
        if existing.is_empty() {
            "（无）"
        } else {
            &existing
        },
        transcript
    );
    let response = reqwest::Client::new()
        .post(format!(
            "{}/chat/completions",
            model.base_url.trim_end_matches('/')
        ))
        .bearer_auth(&model.api_key)
        .json(&serde_json::json!({
            "model": model.model,
            "messages": [
                {"role":"system","content":"你是严格的长期记忆观察器，只输出合法 JSON。"},
                {"role":"user","content":prompt}
            ],
            "temperature": 0.1,
            "thinking": {"type":"disabled"},
            "response_format": {"type":"json_object"}
        }))
        .send()
        .await
        .map_err(|e| format!("观察模型请求失败：{e}"))?;
    if !response.status().is_success() {
        return Err(format!("观察模型返回 {}", response.status()));
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("观察响应无效：{e}"))?;
    let content = value
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .ok_or("观察模型没有返回内容")?;
    let parsed: ObservationResult =
        serde_json::from_str(content.trim()).map_err(|e| format!("观察 JSON 无效：{e}"))?;
    let source_id = batch.last().map(|(_, m)| m.id.as_str());
    for item in parsed.memories.into_iter().take(12) {
        let text = item.text.trim();
        if text.is_empty()
            || !matches!(
                item.memory_type.as_str(),
                "fact" | "preference" | "habit" | "relationship" | "experience"
            )
        {
            continue;
        }
        let exists = {
            let conn = db_state.0.lock().await;
            db::memory_text_exists(&conn, text).map_err(|e| e.to_string())?
        };
        if !exists {
            let _ = crate::memory::remember(
                db_state.inner(),
                &config,
                text,
                &item.memory_type,
                item.importance,
                source_id,
            )
            .await;
        }
    }
    let conn = db_state.0.lock().await;
    db::advance_memory_observer(&conn, batch.last().unwrap().0).map_err(|e| e.to_string())?;
    Ok(())
}
