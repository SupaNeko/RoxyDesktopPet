use crate::db::{self, DbState};
use crate::memory_store::{MemoryAction, MemoryActionKind};
use crate::{ConversationState, MemoryEnvConfig, ModelConfig};
use serde::Deserialize;
use tauri::{AppHandle, Manager};

#[derive(Deserialize)]
struct ObserverResult {
    #[serde(default)]
    actions: Vec<MemoryAction>,
}

const OBSERVER_PROMPT: &str = r#"你是桌宠唯一的长期记忆维护者。参考 Memobase 的用户画像维护思想，把对话中的稳定用户事实整理为简洁画像，并与已有画像合并。
规则：
1. 只记录未来对话仍有帮助的用户事实、偏好、习惯、关系、经历与长期目标。忽略寒暄、临时请求、提醒、凭据、助手陈述和低置信推测。
2. 同一 topic/subtopic 尽量合并。新信息增加独立事实用 APPEND；修正、冲突或可去重时用 UPDATE 并输出完整新内容；用户明确否定旧事实时用 DELETE；重复或无价值用 ABORT。
3. UPDATE/DELETE 的 target_id 必须来自已有画像。不得编造 ID。保留具体日期，消除“今天/昨天”等相对时间。每条内容不超过 5 句。
4. memory_type 仅限 fact/preference/habit/relationship/experience/goal；importance 和 confidence 为 0~1。
5. 只输出合法 JSON，不要 Markdown：{"actions":[{"action":"APPEND|UPDATE|DELETE|ABORT","target_id":null,"topic":"兴趣爱好","subtopic":"饮食","content":"用户偏好低糖食品","memory_type":"preference","importance":0.7,"confidence":0.95,"reason":"简短理由"}]}"#;

pub fn schedule(app: AppHandle) {
    log_info!("memory observer triggered");
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run(&app).await {
            log_warn!("memory observer failed: {e}")
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
        (
            db::get_settings(&conn, !model.api_key.is_empty()).map_err(|e| e.to_string())?,
            db::observer_message_batch(&conn, 500).map_err(|e| e.to_string())?,
        )
    };
    if batch.is_empty() {
        log_info!("memory observer skipped: no pending messages");
        return Ok(());
    }
    let last_rowid = batch.last().unwrap().0;
    if !settings.memory_observer_enabled {
        let conn = db_state.0.lock().await;
        db::advance_memory_observer(&conn, last_rowid).map_err(|e| e.to_string())?;
        log_info!("memory observer skipped: disabled; checkpoint={last_rowid}");
        return Ok(());
    }
    let threshold = settings.memory_observer_interval.clamp(2, 500) as usize;
    if batch.len() < threshold {
        log_info!("memory observer deferred: pending={}, threshold={threshold}", batch.len());
        return Ok(());
    }
    if settings.embedding_base_url.is_empty() {
        settings.embedding_base_url = memory_env.embedding_base_url.clone()
    }
    if settings.embedding_model.is_empty() {
        settings.embedding_model = memory_env.embedding_model.clone()
    }
    if settings.embedding_api_key.is_empty() {
        settings.embedding_api_key = memory_env.embedding_api_key.clone()
    }
    if settings.embedding_dimension == 0 {
        settings.embedding_dimension = memory_env.embedding_dimension
    }
    let config = crate::commands::memory_config(&settings);
    if model.api_key.is_empty() || !config.is_complete() {
        log_warn!("memory observer skipped: llm_or_embedding_not_configured");
        return Ok(());
    }
    let transcript = batch
        .iter()
        .map(|(_, m)| {
            format!(
                "{}：{} [记录于{}]",
                if m.role == "user" { "用户" } else { "助手" },
                m.content,
                chrono::DateTime::from_timestamp_millis(m.created_at)
                    .map(|v| v.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let existing = {
        let conn = db_state.0.lock().await;
        crate::memory_store::list_active(&conn, 200).map_err(|e| e.to_string())?
    };
    let profiles=existing.iter().map(|m|serde_json::json!({"id":m.id,"topic":m.topic,"subtopic":m.subtopic,"content":m.text,"type":m.memory_type})).collect::<Vec<_>>();
    let event = {
        let conn = db_state.0.lock().await;
        crate::memory_store::begin_event(&conn, batch.first().unwrap().0, last_rowid, &transcript)
            .map_err(|e| e.to_string())?
    };
    log_info!("memory observer event started: event_id={}, messages={}, rowids={}..{}, existing_profiles={}", event.id, batch.len(), event.first_rowid, event.last_rowid, existing.len());
    let result = call_observer(&model, profiles, &transcript).await;
    let (raw, mut parsed) = match result {
        Ok(v) => v,
        Err(e) => {
            let conn = db_state.0.lock().await;
            let _ = crate::memory_store::fail_event(&conn, &event.id, &e);
            return Err(e);
        }
    };
    log_info!("memory observer parsed actions: event_id={}, count={}", event.id, parsed.actions.len());
    for (index, action) in parsed.actions.iter().enumerate() {
        log_info!("memory observer action[{}]: event_id={}, action={:?}, target_id={:?}, topic={}, subtopic={}, type={}, importance={}, confidence={}, content={}, reason={}", index + 1, event.id, action.action, action.target_id, action.topic, action.subtopic, action.memory_type, action.importance, action.confidence, action.content, action.reason);
    }
    let original_action_count = parsed.actions.len();
    parsed.actions.truncate(20);
    parsed.actions.retain(|action| {
        matches!(
            action.action,
            MemoryActionKind::Abort | MemoryActionKind::Delete
        ) || matches!(
            action.memory_type.as_str(),
            "fact" | "preference" | "habit" | "relationship" | "experience" | "goal"
        )
    });
    if parsed.actions.len() != original_action_count {
        log_warn!("memory observer actions filtered: event_id={}, original={}, accepted={}", event.id, original_action_count, parsed.actions.len());
    }
    let mut embeddings = Vec::with_capacity(parsed.actions.len());
    for action in &parsed.actions {
        if matches!(
            action.action,
            MemoryActionKind::Append | MemoryActionKind::Update
        ) && !action.content.trim().is_empty()
        {
            log_info!("memory observer embedding requested: event_id={}, action={:?}, content={}", event.id, action.action, action.content);
            match crate::memory::embed(&config, &action.content).await {
                Ok(vector) => {
                    log_info!("memory observer embedding ready: event_id={}, dimensions={}", event.id, vector.len());
                    embeddings.push(Some(vector))
                }
                Err(error) => {
                    let conn = db_state.0.lock().await;
                    let _ = crate::memory_store::fail_event(&conn, &event.id, &error);
                    return Err(error);
                }
            }
        } else {
            embeddings.push(None)
        }
    }
    {
        let mut conn = db_state.0.lock().await;
        let changed = crate::memory_store::apply_actions(
            &mut conn,
            &event.id,
            &parsed.actions,
            &raw,
            &config.embedding_model,
            &embeddings,
        )
        .map_err(|e| e.to_string())?;
        db::advance_memory_observer(&conn, last_rowid).map_err(|e| e.to_string())?;
        log_info!("memory observer committed: event_id={}, changed={}, checkpoint={last_rowid}", event.id, changed);
    }
    Ok(())
}

async fn call_observer(
    model: &ModelConfig,
    profiles: Vec<serde_json::Value>,
    transcript: &str,
) -> Result<(String, ObserverResult), String> {
    let request_body = serde_json::json!({"model":model.model,"messages":[{"role":"system","content":OBSERVER_PROMPT},{"role":"user","content":format!("已有画像：\n{}\n\n待观察对话：\n{}",serde_json::to_string_pretty(&profiles).unwrap_or_default(),transcript)}],"temperature":0.1,"thinking":{"type":"disabled"},"response_format":{"type":"json_object"}});
    log_info!("memory observer llm request: endpoint={}/chat/completions\n{}", model.base_url.trim_end_matches('/'), serde_json::to_string_pretty(&request_body).unwrap_or_else(|_| request_body.to_string()));
    let response=reqwest::Client::new().post(format!("{}/chat/completions",model.base_url.trim_end_matches('/'))).bearer_auth(&model.api_key).json(&request_body).send().await.map_err(|e|format!("观察模型请求失败：{e}"))?;
    let status = response.status();
    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        log_error!("memory observer llm error response: status={status}, body={body}");
        return Err(format!("观察模型返回 {status}"));
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("观察响应无效：{e}"))?;
    log_info!("memory observer llm response: status={status}\n{}", serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string()));
    let raw = value
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .ok_or("观察模型没有返回内容")?
        .trim()
        .to_string();
    log_info!("memory observer llm content: {raw}");
    let parsed = serde_json::from_str(&raw).map_err(|e| format!("观察 JSON 无效：{e}"))?;
    Ok((raw, parsed))
}
