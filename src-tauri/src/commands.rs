use crate::db::{self, AppSettings, DbState, Message};
use crate::{AsrEnvConfig, ConversationState, MemoryEnvConfig, ModelConfig};
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, State};

const NATURAL_SPEECH_RULES: &str = "像日常聊天一样自然、口语化地说话，只输出角色实际说出口的内容。禁止使用任何括号或类似格式描写动作、表情、神态、心理、语气和场景；禁止输出舞台说明、旁白、动作标签、表情标签或其他非对白内容。不要为了表现人设而堆砌形容词、书面语或刻意卖萌。";

#[derive(Debug, Deserialize)]
pub struct SaveSettingsRequest {
    pub pet_name: String,
    pub persona: String,
    pub user_name: String,
    pub voice_output_mode: String,
    pub tts_api_protocol: String,
    pub tts_api_base_url: String,
    pub tts_api_model: String,
    pub tts_api_key: String,
    pub tts_api_voice: String,
    pub tts_api_language: String,
    pub vits_model_name: String,
    pub vits_model_path: String,
    pub vits_speaker_id: Option<String>,
    pub vits_target_language: String,
    pub vits_speed: f64,
    pub vits_emotion_params: String,
    pub vits_translate_enabled: bool,
    pub microphone_device_name: Option<String>,
    pub asr_app_id: String,
    pub asr_api_key: String,
    pub asr_api_secret: String,
    pub proactive_enabled: bool,
    pub proactive_min_minutes: u32,
    pub proactive_max_minutes: u32,
    pub proactive_daily_limit: u32,
    pub qdrant_url: String,
    pub embedding_base_url: String,
    pub embedding_model: String,
    pub embedding_api_key: String,
    pub embedding_dimension: u32,
    pub memory_observer_enabled: bool,
    pub memory_observer_interval: u32,
}

#[derive(Serialize)]
pub struct RuntimeStatus {
    database_ready: bool,
    llm_configured: bool,
    vits_status: String,
    memory_status: String,
    microphone_status: String,
    qdrant_runtime_status: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}
#[derive(Deserialize)]
struct Choice {
    message: serde_json::Value,
}

fn apply_memory_env(settings: &mut AppSettings, env: &MemoryEnvConfig) {
    if settings.qdrant_url.is_empty() {
        settings.qdrant_url = env.qdrant_url.clone();
    }
    if settings.embedding_base_url.is_empty() {
        settings.embedding_base_url = env.embedding_base_url.clone();
    }
    if settings.embedding_model.is_empty() {
        settings.embedding_model = env.embedding_model.clone();
    }
    if settings.embedding_api_key.is_empty() {
        settings.embedding_api_key = env.embedding_api_key.clone();
    }
    if settings.embedding_dimension == 0 {
        settings.embedding_dimension = env.embedding_dimension;
    }
    settings.memory_configured = !settings.qdrant_url.is_empty()
        && !settings.embedding_base_url.is_empty()
        && !settings.embedding_model.is_empty()
        && !settings.embedding_api_key.is_empty()
        && settings.embedding_dimension > 0;
}

fn apply_tts_defaults(settings: &mut AppSettings) {
    if settings.tts_api_key.is_empty() {
        settings.tts_api_key = settings.embedding_api_key.clone();
    }
    if settings.tts_api_protocol == "dashscope"
        && settings.tts_api_base_url == "https://dashscope.aliyuncs.com/api/v1"
        && settings.embedding_base_url.ends_with("/compatible-mode/v1")
    {
        settings.tts_api_base_url = settings
            .embedding_base_url
            .trim_end_matches("/compatible-mode/v1")
            .to_string()
            + "/api/v1";
    }
}

pub(crate) fn memory_config(settings: &AppSettings) -> crate::memory::MemoryConfig {
    crate::memory::MemoryConfig {
        qdrant_url: settings.qdrant_url.clone(),
        embedding_base_url: settings.embedding_base_url.clone(),
        embedding_model: settings.embedding_model.clone(),
        embedding_api_key: settings.embedding_api_key.clone(),
        embedding_dimension: settings.embedding_dimension,
    }
}

#[tauri::command]
pub async fn get_settings(
    db_state: State<'_, DbState>,
    model: State<'_, ModelConfig>,
    asr_env: State<'_, AsrEnvConfig>,
    memory_env: State<'_, MemoryEnvConfig>,
) -> Result<AppSettings, String> {
    let has_key = !model.api_key.is_empty();
    let conn = db_state.0.lock().await;
    let mut settings = db::get_settings(&conn, has_key).map_err(|e| e.to_string())?;
    settings.api_base_url = model.base_url.clone();
    settings.api_model = model.model.clone();
    settings.asr_configured = (!settings.asr_app_id.is_empty() || !asr_env.app_id.is_empty())
        && (!settings.asr_api_key.is_empty() || !asr_env.api_key.is_empty())
        && (!settings.asr_api_secret.is_empty() || !asr_env.api_secret.is_empty());
    if settings.asr_app_id.is_empty() {
        settings.asr_app_id = asr_env.app_id.clone();
    }
    settings.asr_api_key.clear();
    settings.asr_api_secret.clear();
    apply_memory_env(&mut settings, memory_env.inner());
    apply_tts_defaults(&mut settings);
    settings.embedding_api_key.clear();
    settings.tts_api_configured = !settings.tts_api_key.is_empty();
    settings.tts_api_key.clear();
    Ok(settings)
}

#[tauri::command]
pub async fn save_settings(
    db_state: State<'_, DbState>,
    model: State<'_, ModelConfig>,
    asr_env: State<'_, AsrEnvConfig>,
    memory_env: State<'_, MemoryEnvConfig>,
    request: SaveSettingsRequest,
) -> Result<AppSettings, String> {
    if request.pet_name.trim().is_empty() {
        return Err("桌宠名字不能为空".into());
    }
    let existing = {
        let conn = db_state.0.lock().await;
        db::get_settings(&conn, !model.api_key.is_empty()).map_err(|e| e.to_string())?
    };
    let previous_proactive = (
        existing.proactive_enabled,
        existing.proactive_min_minutes,
        existing.proactive_max_minutes,
        existing.proactive_daily_limit,
    );
    let asr_app_id = if request.asr_app_id.trim().is_empty() {
        existing.asr_app_id
    } else {
        request.asr_app_id.trim().into()
    };
    let asr_api_key = if request.asr_api_key.trim().is_empty() {
        existing.asr_api_key
    } else {
        request.asr_api_key.trim().into()
    };
    let asr_api_secret = if request.asr_api_secret.trim().is_empty() {
        existing.asr_api_secret
    } else {
        request.asr_api_secret.trim().into()
    };
    let embedding_api_key = if request.embedding_api_key.trim().is_empty() {
        existing.embedding_api_key
    } else {
        request.embedding_api_key.trim().into()
    };
    let tts_api_key = if request.tts_api_key.trim().is_empty() {
        if existing.tts_api_key.is_empty() {
            if embedding_api_key.is_empty() {
                memory_env.embedding_api_key.clone()
            } else {
                embedding_api_key.clone()
            }
        } else {
            existing.tts_api_key
        }
    } else {
        request.tts_api_key.trim().into()
    };
    let mut settings = AppSettings {
        pet_name: request.pet_name.trim().into(),
        persona: request.persona.trim().into(),
        user_name: request.user_name.trim().into(),
        api_base_url: model.base_url.clone(),
        api_model: model.model.clone(),
        api_key_configured: !model.api_key.is_empty(),
        voice_output_enabled: request.voice_output_mode != "disabled",
        voice_output_mode: match request.voice_output_mode.as_str() {
            "api" | "vits" => request.voice_output_mode,
            _ => "disabled".into(),
        },
        tts_api_protocol: if request.tts_api_protocol == "openai" {
            "openai".into()
        } else {
            "dashscope".into()
        },
        tts_api_base_url: request
            .tts_api_base_url
            .trim()
            .trim_end_matches('/')
            .to_string(),
        tts_api_model: request.tts_api_model.trim().to_string(),
        tts_api_key,
        tts_api_configured: false,
        tts_api_voice: request.tts_api_voice.trim().to_string(),
        tts_api_language: request.tts_api_language.trim().to_string(),
        vits_model_name: request.vits_model_name.trim().to_string(),
        vits_model_path: request.vits_model_path.trim().to_string(),
        vits_speaker_id: request.vits_speaker_id.filter(|v| !v.trim().is_empty()),
        vits_target_language: request.vits_target_language.trim().to_string(),
        vits_speed: request.vits_speed.clamp(0.5, 2.0),
        vits_emotion_params: request.vits_emotion_params.trim().to_string(),
        vits_translate_enabled: request.vits_translate_enabled,
        microphone_device_name: request
            .microphone_device_name
            .filter(|name| !name.trim().is_empty()),
        asr_app_id,
        asr_api_key,
        asr_api_secret,
        asr_configured: false,
        proactive_enabled: request.proactive_enabled,
        proactive_min_minutes: request.proactive_min_minutes.clamp(1, 10_080),
        proactive_max_minutes: request.proactive_max_minutes.clamp(1, 10_080),
        proactive_daily_limit: request.proactive_daily_limit.clamp(1, 100),
        qdrant_url: request.qdrant_url.trim().trim_end_matches('/').to_string(),
        embedding_base_url: request
            .embedding_base_url
            .trim()
            .trim_end_matches('/')
            .to_string(),
        embedding_model: request.embedding_model.trim().to_string(),
        embedding_api_key,
        embedding_dimension: request.embedding_dimension,
        memory_configured: false,
        memory_observer_enabled: request.memory_observer_enabled,
        memory_observer_interval: request.memory_observer_interval.clamp(2, 500),
    };
    if settings.proactive_min_minutes > settings.proactive_max_minutes {
        return Err("主动消息最短间隔不能大于最长间隔".into());
    }
    let conn = db_state.0.lock().await;
    db::save_settings(&conn, &settings).map_err(|e| e.to_string())?;
    let current_proactive = (
        settings.proactive_enabled,
        settings.proactive_min_minutes,
        settings.proactive_max_minutes,
        settings.proactive_daily_limit,
    );
    if previous_proactive != current_proactive {
        db::reset_proactive_schedule(&conn, &settings, chrono::Utc::now().timestamp_millis())
            .map_err(|e| e.to_string())?;
    }
    settings.asr_configured = (!settings.asr_app_id.is_empty() || !asr_env.app_id.is_empty())
        && (!settings.asr_api_key.is_empty() || !asr_env.api_key.is_empty())
        && (!settings.asr_api_secret.is_empty() || !asr_env.api_secret.is_empty());
    if settings.asr_app_id.is_empty() {
        settings.asr_app_id = asr_env.app_id.clone();
    }
    settings.asr_api_key.clear();
    settings.asr_api_secret.clear();
    apply_memory_env(&mut settings, memory_env.inner());
    apply_tts_defaults(&mut settings);
    settings.embedding_api_key.clear();
    settings.tts_api_configured = !settings.tts_api_key.is_empty();
    settings.tts_api_key.clear();
    Ok(settings)
}

#[tauri::command]
pub async fn list_messages(
    db_state: State<'_, DbState>,
    limit: u32,
) -> Result<Vec<Message>, String> {
    let conn = db_state.0.lock().await;
    db::list_messages(&conn, limit).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_runtime_status(
    db_state: State<'_, DbState>,
    model: State<'_, ModelConfig>,
    voice: State<'_, crate::audio::VoiceState>,
    memory_env: State<'_, MemoryEnvConfig>,
    qdrant_runtime: State<'_, crate::qdrant_runtime::QdrantRuntime>,
) -> Result<RuntimeStatus, String> {
    let has_key = !model.api_key.is_empty();
    let conn = db_state.0.lock().await;
    let mut settings = db::get_settings(&conn, has_key).map_err(|e| e.to_string())?;
    drop(conn);
    apply_memory_env(&mut settings, memory_env.inner());
    let memory_status = if !settings.memory_configured {
        "not_configured"
    } else {
        let config = memory_config(&settings);
        let mut available = crate::memory::health(&config).await;
        // 设置窗口和 Qdrant Runtime 会在应用启动时并行初始化。首次检查如果
        // 恰好落在启动窗口内，短暂重试，避免返回随后不会自动更新的假阴性。
        if !available
            && matches!(
                crate::qdrant_runtime::status(qdrant_runtime.inner())
                    .await
                    .as_str(),
                "not_started" | "starting"
            )
        {
            for _ in 0..10 {
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                if crate::memory::health(&config).await {
                    available = true;
                    break;
                }
            }
        }
        if available {
            "available"
        } else {
            "unavailable"
        }
    };
    let vits = crate::data_dir()
        .join("vits_runtime")
        .join("vits_runtime.exe");
    Ok(RuntimeStatus {
        database_ready: true,
        llm_configured: has_key
            && !settings.api_base_url.is_empty()
            && !settings.api_model.is_empty(),
        vits_status: if settings.voice_output_mode != "vits" {
            "disabled"
        } else if vits.exists() {
            "available"
        } else {
            "not_configured"
        }
        .into(),
        memory_status: memory_status.into(),
        microphone_status: if voice.active.load(Ordering::SeqCst) {
            "listening"
        } else {
            "disabled"
        }
        .into(),
        qdrant_runtime_status: crate::qdrant_runtime::status(qdrant_runtime.inner()).await,
    })
}

#[tauri::command]
pub async fn send_text_message(
    app: AppHandle,
    db_state: State<'_, DbState>,
    model: State<'_, ModelConfig>,
    conversation: State<'_, ConversationState>,
    memory_env: State<'_, MemoryEnvConfig>,
    content: String,
) -> Result<Message, String> {
    let result = process_message(
        db_state.inner(),
        model.inner(),
        conversation.inner(),
        memory_env.inner(),
        content,
        "user_text",
    )
    .await;
    if result.is_ok() {
        crate::observer::schedule(app.clone());
        crate::voice_output::schedule(app, result.as_ref().unwrap().clone());
    }
    result
}

async fn process_message(
    db_state: &DbState,
    model: &ModelConfig,
    conversation: &ConversationState,
    memory_env: &MemoryEnvConfig,
    content: String,
    trigger_type: &str,
) -> Result<Message, String> {
    let _conversation_guard = conversation.0.lock().await;
    let content = content.trim().to_string();
    if content.is_empty() {
        return Err("消息不能为空".into());
    }
    let now = chrono::Utc::now().timestamp_millis();
    let user = Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: "user".into(),
        content: content.clone(),
        trigger_type: trigger_type.into(),
        created_at: now,
    };
    let (mut settings, history) = {
        let conn = db_state.0.lock().await;
        db::insert_message(&conn, &user).map_err(|e| e.to_string())?;
        (
            db::get_settings(&conn, true).map_err(|e| e.to_string())?,
            db::list_messages(&conn, 30).map_err(|e| e.to_string())?,
        )
    };
    apply_memory_env(&mut settings, memory_env);
    if model.api_key.is_empty() {
        return Err("未在 .env 中配置 DEEPSEEK_API_KEY".into());
    }
    let now_text = chrono::Local::now().format("%Y-%m-%d %H:%M:%S %:z");
    let config = memory_config(&settings);
    let recalled = crate::memory::recall(&config, &content)
        .await
        .unwrap_or_default();
    let memory_context = if recalled.is_empty() {
        "无可用长期记忆".into()
    } else {
        recalled
            .iter()
            .map(|m| format!("- {m}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let memory_instruction = if config.is_complete() {
        "用户表达稳定事实、偏好、习惯、关系或重要经历时，调用 remember 保存精炼事实；不要保存临时请求或敏感凭据。"
    } else {
        "长期记忆当前未配置，不要声称已经记住了信息。"
    };
    let mut api_messages = vec![
        serde_json::json!({"role":"system","content": format!("你是桌宠{}。人物设定：{}\n当前时间：{}，用户时区：Asia/Shanghai。{}\n使用中文简洁回应。不要声称拥有未提供的电脑信息。用户明确要求提醒时必须调用 create_todo；时间有实质歧义时先追问，不要创建。{}\n相关长期记忆：\n{}", settings.pet_name, settings.persona, now_text,NATURAL_SPEECH_RULES,memory_instruction,memory_context)}),
    ];
    api_messages.extend(
        history
            .into_iter()
            .map(|m| serde_json::json!({"role":m.role,"content":m.content})),
    );
    let mut tools = vec![
        serde_json::json!({"type":"function","function":{"name":"create_todo","description":"创建一个到点提醒用户的待办事项","parameters":{"type":"object","properties":{"title":{"type":"string","description":"需要提醒用户做的事情"},"due_at":{"type":"string","description":"含时区的 RFC3339 时间，例如 2026-08-21T19:00:00+08:00"},"timezone":{"type":"string","description":"IANA 时区，默认 Asia/Shanghai"}},"required":["title","due_at"],"additionalProperties":false}}}),
        serde_json::json!({"type":"function","function":{"name":"list_todos","description":"查看尚未完成的提醒事项","parameters":{"type":"object","properties":{},"additionalProperties":false}}}),
    ];
    if config.is_complete() {
        tools.push(serde_json::json!({"type":"function","function":{"name":"remember","description":"保存值得未来对话使用的长期记忆","parameters":{"type":"object","properties":{"text":{"type":"string"},"memory_type":{"type":"string","enum":["fact","preference","habit","relationship","experience"]},"importance":{"type":"number","minimum":0,"maximum":1}},"required":["text","memory_type","importance"],"additionalProperties":false}}}));
    }
    let client = reqwest::Client::new();
    let source_message_id = user.id.clone();
    let mut reply = None;
    for _ in 0..4 {
        let response = client.post(format!("{}/chat/completions", model.base_url)).bearer_auth(&model.api_key).json(&serde_json::json!({"model":model.model,"messages":api_messages,"tools":tools,"tool_choice":"auto","temperature":0.8,"thinking":{"type":"disabled"}})).send().await.map_err(|e| format!("模型请求失败：{e}"))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!(
                "模型返回 {status}: {}",
                body.chars().take(240).collect::<String>()
            ));
        }
        let parsed: ChatResponse = response
            .json()
            .await
            .map_err(|e| format!("无法解析模型响应：{e}"))?;
        let message = parsed
            .choices
            .into_iter()
            .next()
            .ok_or("模型没有返回结果")?
            .message;
        let tool_calls = message
            .get("tool_calls")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if tool_calls.is_empty() {
            reply = message
                .get("content")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            break;
        }
        api_messages.push(message);
        for call in tool_calls {
            let call_id = call
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or("工具调用缺少 id")?;
            let name = call
                .pointer("/function/name")
                .and_then(|v| v.as_str())
                .ok_or("工具调用缺少名称")?;
            let arguments = call
                .pointer("/function/arguments")
                .and_then(|v| v.as_str())
                .unwrap_or("{}");
            let args: serde_json::Value = serde_json::from_str(arguments)
                .map_err(|e| format!("工具参数不是合法 JSON：{e}"))?;
            let result = match name {
                "create_todo" => {
                    let title = args
                        .get("title")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .ok_or("提醒事项不能为空")?;
                    let due_at = args
                        .get("due_at")
                        .and_then(|v| v.as_str())
                        .ok_or("提醒缺少到期时间")?;
                    let due = chrono::DateTime::parse_from_rfc3339(due_at)
                        .map_err(|_| "提醒时间必须是含时区的 RFC3339 格式")?
                        .timestamp_millis();
                    if due <= chrono::Utc::now().timestamp_millis() {
                        return Err("提醒时间必须晚于当前时间".into());
                    }
                    let timezone = args
                        .get("timezone")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Asia/Shanghai");
                    let mut conn = db_state.0.lock().await;
                    let todo =
                        db::create_todo(&mut conn, title, due, timezone, Some(&source_message_id))
                            .map_err(|e| e.to_string())?;
                    serde_json::to_string(&todo).map_err(|e| e.to_string())?
                }
                "list_todos" => {
                    let conn = db_state.0.lock().await;
                    serde_json::to_string(
                        &db::list_pending_todos(&conn).map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string())?
                }
                "remember" => {
                    let text = args
                        .get("text")
                        .and_then(|v| v.as_str())
                        .ok_or("记忆缺少内容")?;
                    let kind = args
                        .get("memory_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("fact");
                    let importance = args
                        .get("importance")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.5);
                    match crate::memory::remember(
                        db_state,
                        &config,
                        text,
                        kind,
                        importance,
                        Some(&source_message_id),
                    )
                    .await
                    {
                        Ok(memory) => serde_json::to_string(&memory).map_err(|e| e.to_string())?,
                        Err(error) => format!("记忆未保存：{error}"),
                    }
                }
                _ => return Err(format!("模型请求了不允许的工具：{name}")),
            };
            api_messages
                .push(serde_json::json!({"role":"tool","tool_call_id":call_id,"content":result}));
        }
    }
    let reply = reply.ok_or("模型工具调用超过上限或没有返回文本")?;
    let assistant = Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: "assistant".into(),
        content: reply,
        trigger_type: trigger_type.into(),
        created_at: chrono::Utc::now().timestamp_millis(),
    };
    let conn = db_state.0.lock().await;
    db::insert_message(&conn, &assistant).map_err(|e| e.to_string())?;
    Ok(assistant)
}

pub async fn process_voice_text(app: AppHandle, content: String) -> Result<Message, String> {
    let db_state = app.state::<DbState>();
    let model = app.state::<ModelConfig>();
    let conversation = app.state::<ConversationState>();
    let memory_env = app.state::<MemoryEnvConfig>();
    let result = process_message(
        db_state.inner(),
        model.inner(),
        conversation.inner(),
        memory_env.inner(),
        content,
        "user_voice",
    )
    .await;
    if result.is_ok() {
        crate::observer::schedule(app.clone());
        crate::voice_output::schedule(app, result.as_ref().unwrap().clone());
    }
    result
}

pub async fn generate_scheduled_message(
    app: &AppHandle,
    trigger_type: &str,
    event: &str,
) -> Result<Message, String> {
    let db_state = app.state::<DbState>();
    let model = app.state::<ModelConfig>();
    let conversation = app.state::<ConversationState>();
    let _guard = conversation.0.lock().await;
    if model.api_key.is_empty() {
        return Err("未配置 DeepSeek API Key".into());
    }
    let (settings, history) = {
        let conn = db_state.0.lock().await;
        (
            db::get_settings(&conn, true).map_err(|e| e.to_string())?,
            db::list_messages(&conn, 20).map_err(|e| e.to_string())?,
        )
    };
    let mut messages = vec![
        serde_json::json!({"role":"system","content":format!("你是桌宠{}。人物设定：{}\n这是一次{}触发。请保持角色身份，用中文输出一条简短消息。{}\n不要虚构电脑状态。",settings.pet_name,settings.persona,trigger_type,NATURAL_SPEECH_RULES)}),
    ];
    messages.extend(
        history
            .into_iter()
            .map(|m| serde_json::json!({"role":m.role,"content":m.content})),
    );
    messages.push(serde_json::json!({"role":"user","content":event}));
    let response = reqwest::Client::new().post(format!("{}/chat/completions", model.base_url)).bearer_auth(&model.api_key).json(&serde_json::json!({"model":model.model,"messages":messages,"temperature":0.9,"thinking":{"type":"disabled"}})).send().await.map_err(|e| format!("模型请求失败：{e}"))?;
    if !response.status().is_success() {
        return Err(format!("模型返回 {}", response.status()));
    }
    let parsed: ChatResponse = response
        .json()
        .await
        .map_err(|e| format!("无法解析模型响应：{e}"))?;
    let content = parsed
        .choices
        .first()
        .and_then(|c| c.message.get("content"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("模型没有返回文本")?
        .to_string();
    let message = Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: "assistant".into(),
        content,
        trigger_type: trigger_type.into(),
        created_at: chrono::Utc::now().timestamp_millis(),
    };
    let conn = db_state.0.lock().await;
    db::insert_message(&conn, &message).map_err(|e| e.to_string())?;
    Ok(message)
}

#[tauri::command]
pub fn scan_vits_models() -> Result<Vec<crate::voice_output::VitsModelInfo>, String> {
    crate::voice_output::scan_models(&crate::data_dir())
}

#[tauri::command]
pub async fn test_voice_output(app: AppHandle) -> Result<(), String> {
    crate::voice_output::test(&app).await
}

#[tauri::command]
pub async fn start_voice_listening(
    app: AppHandle,
    voice: State<'_, crate::audio::VoiceState>,
    db_state: State<'_, DbState>,
    asr_env: State<'_, AsrEnvConfig>,
) -> Result<(), String> {
    let (selected, credentials) = {
        let conn = db_state.0.lock().await;
        let settings = db::get_settings(&conn, true).map_err(|e| e.to_string())?;
        let pick = |stored: String, env: &String| {
            if stored.is_empty() {
                env.clone()
            } else {
                stored
            }
        };
        (
            settings.microphone_device_name,
            crate::asr::XfyunCredentials {
                app_id: pick(settings.asr_app_id, &asr_env.app_id),
                api_key: pick(settings.asr_api_key, &asr_env.api_key),
                api_secret: pick(settings.asr_api_secret, &asr_env.api_secret),
            },
        )
    };
    if !credentials.is_complete() {
        return Err("请先在设置或 .env 中配置完整的讯飞 ASR 凭据".into());
    }
    crate::audio::start(app, voice.inner(), selected.as_deref(), credentials).await
}

#[tauri::command]
pub async fn stop_voice_listening(
    voice: State<'_, crate::audio::VoiceState>,
) -> Result<(), String> {
    crate::audio::stop(voice.inner()).await;
    Ok(())
}

#[tauri::command]
pub fn list_microphone_devices() -> Result<Vec<String>, String> {
    crate::audio::input_devices()
}

#[tauri::command]
pub async fn list_memories(db_state: State<'_, DbState>) -> Result<Vec<db::Memory>, String> {
    let conn = db_state.0.lock().await;
    db::list_memories(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_todos(db_state: State<'_, DbState>) -> Result<Vec<db::Todo>, String> {
    let conn = db_state.0.lock().await;
    db::list_pending_todos(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_app_window(app: AppHandle, label: String) -> Result<(), String> {
    if !matches!(label.as_str(), "settings" | "todos") {
        return Err("不允许打开该窗口".into());
    }
    let window = app
        .get_webview_window(&label)
        .ok_or_else(|| format!("窗口不存在：{label}"))?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "requires real DeepSeek credentials and network"]
    async fn live_model_creates_todo_with_tool() {
        let dir = tempfile::tempdir().unwrap();
        let db = DbState(tokio::sync::Mutex::new(
            db::open(&dir.path().join("test.db")).unwrap(),
        ));
        let model = ModelConfig {
            api_key: std::env::var("DEEPSEEK_API_KEY").unwrap(),
            base_url: std::env::var("DEEPSEEK_BASE_URL")
                .unwrap_or_else(|_| "https://api.deepseek.com".into()),
            model: std::env::var("DEEPSEEK_MODEL").unwrap_or_else(|_| "deepseek-v4-flash".into()),
        };
        let conversation = ConversationState(tokio::sync::Mutex::new(()));
        let memory_env = MemoryEnvConfig {
            qdrant_url: "".into(),
            embedding_base_url: "".into(),
            embedding_model: "".into(),
            embedding_api_key: "".into(),
            embedding_dimension: 0,
        };
        let reply = process_message(
            &db,
            &model,
            &conversation,
            &memory_env,
            "提醒我两分钟后喝水".into(),
            "user_text",
        )
        .await
        .unwrap();
        assert!(!reply.content.is_empty());
        let conn = db.0.lock().await;
        assert_eq!(db::list_pending_todos(&conn).unwrap().len(), 1);
    }
}
