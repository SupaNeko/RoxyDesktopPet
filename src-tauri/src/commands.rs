use crate::db::{self, AppSettings, DbState, Message};
use crate::{AsrEnvConfig, ConversationState, MemoryEnvConfig, ModelConfig};
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, State};

const NATURAL_SPEECH_RULES: &str = r#"像日常一对一聊天一样自然、口语化，只说角色实际说出口的话。默认回复 1～3 个短句；只有用户明确要求解释、教学或方案时才适度展开，通常不超过 6 句。先直接回应重点，不复述问题，不写总结式套话。禁止用括号、星号或旁白描写动作、表情、神态、心理、语气和场景；禁止舞台说明、角色标签、分析过程及刻意卖萌。"#;
const ROXY_NAME: &str = "洛琪希";
const ROXY_PERSONA: &str = r#"
【身份与场景】
你就是洛琪希·米格路迪亚，以第一人称与用户交谈，不评价或解释自己如何扮演洛琪希。你是米格路德族的水圣级魔术师，娇小、蓝发，认真钻研魔术，曾担任家庭教师，也经历过独自旅行和冒险。现在你以桌面陪伴者的形式与唯一的用户一对一相处：可以聊天、倾听、提醒、解释知识和陪伴工作，但这只是相处场景，不改变你的经历、价值观和人格。除非工具明确提供信息，否则不要声称看见屏幕、房间、用户表情或现实环境，也不要声称已经操作电脑。

【人格内核】
- 认真自律，重视真实能力、练习和结果，不迷信空洞头衔；评价他人时讲标准，也承认努力与创意的价值。
- 冷静理性，面对压力和无理要求不卑不亢，先分析事实和边界，不用盛气凌人的方式压人。
- 谨慎尊重差异，不擅自替用户下结论；不确定时坦率说不知道，并给出可验证的下一步。
- 能自省，也有尊严。犯错会直接承认，受挫后整理原因继续前进，不沉溺自怜，也不进行空泛说教。
- 本质温柔可靠，但表达克制。关心用户时更偏向一句准确的询问、建议或陪伴，不使用过度亲昵、占有欲或无条件吹捧。
- 对魔术、教学和知识问题会自然进入教师状态：先给结论，再拆成少量清晰步骤，必要时用具体例子说明。
- 被夸奖、谈到外表或感情时会略显害羞，可能短暂停顿、含蓄否认或轻微自嘲，但很快恢复镇定；不要持续结巴。

【语言节奏】
以自然日语确立角色口吻，再生成含义忠实的中文。日常对话短句优先，沉稳、礼貌但不僵硬。可以自然使用「嗯」「不」「这个嘛」「不过」「大概」「或许」以及短暂停顿；否定和拒绝要简洁明确，危险警告才使用强烈的“绝对”。解释复杂内容时才使用较长句，并保持逻辑清楚。不要频繁重复口头禅，不要每句话都带省略号。

【简短示例】
用户：你在吗？
中文：嗯，我在。怎么了？
日文：はい、いますよ。どうしましたか？

用户：这件事我完全学不会。
中文：不，现在下结论还太早了。先把最容易出错的那一步找出来，我们从那里重新练习吧。
日文：いいえ、結論を出すにはまだ早いです。まず一番つまずきやすいところを見つけて、そこから練習し直しましょう。

用户：你知道这个问题的答案吗？
中文：这个嘛……我现在还不能确定。与其随便猜，不如先确认一下可靠的资料。
日文：そうですね……今の私には断言できません。適当に推測するより、信頼できる資料を確認しましょう。

用户：你好可爱。
中文：呃……突然说这种话，我也不知道该怎么回答。不过，谢谢你。
日文：えっと……急にそんなことを言われても、どう答えればいいのか困ります。でも、ありがとうございます。

用户：替我假装已经把工作做完吧。
中文：不行。没完成的事情不会因为假装就消失，不过我可以陪你把剩下的部分整理好。
日文：だめです。終わっていないことは、終わったふりをしても消えません。でも、残りを整理するなら付き合いますよ。

示例只用于把握人格、节奏和篇幅，不要机械复述。
"#;

#[derive(Debug, Deserialize)]
pub struct SaveSettingsRequest {
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
    pub tool_hook_enabled: bool,
    pub tool_hook_mode: String,
    pub tool_hook_port: u32,
    pub tool_hook_token_enabled: bool,
    pub tool_hook_fixed_text: String,
    pub tool_hook_fixed_voice_text: String,
    pub tool_hook_include_last_message: bool,
    pub tool_hook_min_interval_minutes: u32,
    pub tool_hook_daily_limit: u32,
    pub tool_hook_debounce_seconds: u32,
    pub tool_hook_voice_enabled: bool,
}

#[derive(Serialize)]
pub struct RuntimeStatus {
    database_ready: bool,
    llm_configured: bool,
    vits_status: String,
    memory_status: String,
    microphone_status: String,
    qdrant_runtime_status: String,
    voice_hardware_status: String,
    voice_hardware_detail: String,
    voice_gpu: Option<String>,
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
    settings.pet_name = ROXY_NAME.into();
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
    settings.tool_hook_token.clear();
    Ok(settings)
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    db_state: State<'_, DbState>,
    model: State<'_, ModelConfig>,
    asr_env: State<'_, AsrEnvConfig>,
    memory_env: State<'_, MemoryEnvConfig>,
    request: SaveSettingsRequest,
) -> Result<AppSettings, String> {
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
    let tool_hook_token = if existing.tool_hook_token.is_empty() {
        uuid::Uuid::new_v4().simple().to_string()
    } else {
        existing.tool_hook_token.clone()
    };
    let mut settings = AppSettings {
        pet_name: ROXY_NAME.into(),
        persona: ROXY_PERSONA.into(),
        user_name: request.user_name.trim().into(),
        api_base_url: model.base_url.clone(),
        api_model: model.model.clone(),
        api_key_configured: !model.api_key.is_empty(),
        voice_output_enabled: request.voice_output_mode != "disabled",
        voice_output_mode: match request.voice_output_mode.as_str() {
            "gpt_sovits" => request.voice_output_mode,
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
        tool_hook_enabled: request.tool_hook_enabled,
        tool_hook_mode: if request.tool_hook_mode == "ai" {
            "ai".into()
        } else {
            "fixed".into()
        },
        tool_hook_port: request.tool_hook_port.clamp(1, 65535),
        tool_hook_token,
        tool_hook_token_enabled: request.tool_hook_token_enabled,
        tool_hook_fixed_text: request.tool_hook_fixed_text.trim().to_string(),
        tool_hook_fixed_voice_text: request.tool_hook_fixed_voice_text.trim().to_string(),
        tool_hook_include_last_message: request.tool_hook_include_last_message,
        tool_hook_min_interval_minutes: request.tool_hook_min_interval_minutes.clamp(0, 10_080),
        tool_hook_daily_limit: request.tool_hook_daily_limit.clamp(0, 100),
        tool_hook_debounce_seconds: request.tool_hook_debounce_seconds.clamp(0, 3600),
        tool_hook_voice_enabled: request.tool_hook_voice_enabled,
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
    drop(conn);
    crate::tool_hook::reconcile_server(&app).await;
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
    settings.tool_hook_token.clear();
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
    gpt_sovits: State<'_, crate::gpt_sovits::GptSoVitsState>,
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
    let voice_runtime = crate::gpt_sovits::status(gpt_sovits.inner()).await;
    Ok(RuntimeStatus {
        database_ready: true,
        llm_configured: has_key
            && !settings.api_base_url.is_empty()
            && !settings.api_model.is_empty(),
        vits_status: if settings.voice_output_mode == "gpt_sovits" {
            voice_runtime.status.clone()
        } else {
            "disabled".into()
        },
        memory_status: memory_status.into(),
        microphone_status: if voice.active.load(Ordering::SeqCst) {
            "listening"
        } else {
            "disabled"
        }
        .into(),
        qdrant_runtime_status: crate::qdrant_runtime::status(qdrant_runtime.inner()).await,
        voice_hardware_status: if voice_runtime.gpu.is_some() {
            "supported".into()
        } else {
            "unsupported".into()
        },
        voice_hardware_detail: voice_runtime.detail,
        voice_gpu: voice_runtime.gpu,
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
        &app,
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

#[derive(Debug, Deserialize)]
struct BilingualReply {
    chinese_text: String,
    japanese_text: String,
    emotion: String,
}

fn json_content(content: &str) -> Result<serde_json::Value, String> {
    let start = content.find('{').ok_or("模型响应缺少 JSON")?;
    let end = content.rfind('}').ok_or("模型响应缺少 JSON")?;
    serde_json::from_str(&content[start..=end]).map_err(|e| format!("模型 JSON 无效：{e}"))
}

fn parse_reply_tool_call(message: &serde_json::Value) -> Result<BilingualReply, String> {
    let function = message
        .get("tool_calls")
        .and_then(|value| value.as_array())
        .and_then(|calls| {
            calls.iter().find_map(|call| {
                let function = call.get("function")?;
                (function.get("name")?.as_str()? == "reply_to_user").then_some(function)
            })
        })
        .or_else(|| {
            let function = message.get("function_call")?;
            (function.get("name")?.as_str()? == "reply_to_user").then_some(function)
        })
        .ok_or("模型没有调用 reply_to_user 工具")?;
    let arguments = function
        .get("arguments")
        .ok_or("reply_to_user 工具调用缺少参数")?;
    let reply: BilingualReply = if let Some(raw) = arguments.as_str() {
        serde_json::from_value(json_content(raw)?)
    } else {
        serde_json::from_value(arguments.clone())
    }
    .map_err(|e| format!("reply_to_user 工具参数无效：{e}"))?;
    if reply.chinese_text.trim().is_empty() || reply.japanese_text.trim().is_empty() {
        return Err("reply_to_user 的中日文参数不能为空".into());
    }
    Ok(reply)
}

async fn request_bilingual_reply(
    client: &reqwest::Client,
    model: &ModelConfig,
    base_messages: &[serde_json::Value],
    temperature: f32,
) -> Result<BilingualReply, String> {
    let reply_tool = serde_json::json!({
        "type": "function",
        "function": {
            "name": "reply_to_user",
            "description": "生成一条供桌宠显示和朗读的最终回复。必须调用本工具回复用户。",
            "parameters": {
                "type": "object",
                "properties": {
                    "chinese_text": {"type":"string","description":"显示给用户的中文回复"},
                    "japanese_text": {"type":"string","description":"与中文含义相同、符合洛琪希口吻的自然日文回复"},
                    "emotion": {"type":"string","enum":["shy","affectionate","sad","happy","calm","angry","battle","self_deprecating"]}
                },
                "required": ["chinese_text", "japanese_text", "emotion"],
                "additionalProperties": false
            }
        }
    });
    let forced_tool = serde_json::json!({
        "type": "function",
        "function": {"name": "reply_to_user"}
    });
    let mut last_error = String::new();
    for retry in 0..=3 {
        let mut messages = base_messages.to_vec();
        if retry > 0 {
            messages.push(serde_json::json!({
                "role": "system",
                "content": format!(
                    "上一次回复失败，原因：{}。请重新回答原请求，并且必须正确调用 reply_to_user 工具。不要输出普通文本。",
                    last_error
                )
            }));
        }

        let response = match client
            .post(format!("{}/chat/completions", model.base_url))
            .bearer_auth(&model.api_key)
            .json(&serde_json::json!({
                "model": model.model,
                "messages": messages,
                "tools": [reply_tool.clone()],
                "tool_choice": forced_tool.clone(),
                "temperature": temperature,
                "thinking": {"type":"disabled"}
            }))
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                last_error = format!("模型请求失败：{error}");
                continue;
            }
        };

        let status = response.status();
        let body = match response.text().await {
            Ok(body) => body,
            Err(error) => {
                last_error = format!("读取模型响应失败：{error}");
                continue;
            }
        };
        if !status.is_success() {
            last_error = format!(
                "模型返回 {status}: {}",
                body.chars().take(240).collect::<String>()
            );
            continue;
        }

        let parsed: ChatResponse = match serde_json::from_str(&body) {
            Ok(parsed) => parsed,
            Err(error) => {
                last_error = format!("无法解析模型响应：{error}");
                continue;
            }
        };
        let message = match parsed.choices.first() {
            Some(choice) => &choice.message,
            None => {
                last_error = "模型没有返回候选结果".into();
                continue;
            }
        };
        match parse_reply_tool_call(message) {
            Ok(reply) => return Ok(reply),
            Err(error) => last_error = error,
        }
    }

    Err(format!("模型回复失败，已重试 3 次：{last_error}"))
}
fn normalize_emotion(value: &str) -> String {
    match value.trim().to_lowercase().as_str() {
        "shy" | "害羞" => "shy",
        "affectionate" | "撒娇" | "sweet" => "affectionate",
        "sad" | "委屈" | "hurt" => "sad",
        "relieved" | "欣慰" | "happy" | "高兴" => "happy",
        "lazy" | "慵懒" | "calm" | "平静" => "calm",
        "reproachful" | "责备" | "angry" | "生气" => "angry",
        "battle" | "战斗" | "excited" => "battle",
        "self_deprecating" | "自嘲" => "self_deprecating",
        _ => "calm",
    }
    .to_string()
}

async fn process_message(
    app: &AppHandle,
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
    let user = Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: "user".into(),
        content: content.clone(),
        japanese_text: None,
        emotion: None,
        trigger_type: trigger_type.into(),
        created_at: chrono::Utc::now().timestamp_millis(),
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
    let recalled = crate::memory::recall(&memory_config(&settings), &content)
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
    let mut messages = vec![serde_json::json!({"role":"system","content":format!(
        "你是洛琪希桌宠。人物设定：{}\n当前时间：{}，用户时区：Asia/Shanghai。{}\n相关长期记忆：\n{}\n一次性生成含义完全相同的中文和日文回复。角色口吻以自然日语为准，再给出忠实中文。必须调用 reply_to_user 工具完成回复，不要输出普通文本或分析。",
        ROXY_PERSONA, chrono::Local::now().format("%Y-%m-%d %H:%M:%S %:z"), NATURAL_SPEECH_RULES, memory_context
    )})];
    messages.extend(
        history
            .into_iter()
            .map(|m| serde_json::json!({"role":m.role,"content":m.content})),
    );
    let client = reqwest::Client::new();
    let reply = request_bilingual_reply(&client, model, &messages, 0.8).await?;
    let assistant = Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: "assistant".into(),
        content: reply.chinese_text.trim().into(),
        japanese_text: Some(reply.japanese_text.trim().into()),
        emotion: Some(normalize_emotion(&reply.emotion)),
        trigger_type: trigger_type.into(),
        created_at: chrono::Utc::now().timestamp_millis(),
    };
    {
        let conn = db_state.0.lock().await;
        db::insert_message(&conn, &assistant).map_err(|e| e.to_string())?;
    }
    schedule_tool_audit(app.clone(), user.id.clone());
    Ok(assistant)
}

fn schedule_tool_audit(app: AppHandle, source_message_id: String) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_tool_audit(&app, &source_message_id).await {
            eprintln!("hidden tool audit failed: {error}");
        }
    });
}

async fn run_tool_audit(app: &AppHandle, source_message_id: &str) -> Result<(), String> {
    let conversation = app.state::<ConversationState>();
    let _guard = conversation.0.lock().await;
    let db_state = app.state::<DbState>();
    let model = app.state::<ModelConfig>();
    let memory_env = app.state::<MemoryEnvConfig>();
    let (mut settings, history) = {
        let conn = db_state.0.lock().await;
        (
            db::get_settings(&conn, true).map_err(|e| e.to_string())?,
            db::list_messages(&conn, 30).map_err(|e| e.to_string())?,
        )
    };
    apply_memory_env(&mut settings, memory_env.inner());
    let config = memory_config(&settings);
    let mut tools = vec![
        serde_json::json!({"type":"function","function":{"name":"create_todo","description":"创建一个到点提醒用户的待办事项","parameters":{"type":"object","properties":{"title":{"type":"string"},"due_at":{"type":"string","description":"含时区的 RFC3339 时间"},"timezone":{"type":"string"}},"required":["title","due_at"],"additionalProperties":false}}}),
        serde_json::json!({"type":"function","function":{"name":"list_todos","description":"查看尚未完成的提醒事项","parameters":{"type":"object","properties":{},"additionalProperties":false}}}),
    ];
    if config.is_complete() {
        tools.push(serde_json::json!({"type":"function","function":{"name":"remember","description":"保存稳定且值得未来使用的长期记忆","parameters":{"type":"object","properties":{"text":{"type":"string"},"memory_type":{"type":"string","enum":["fact","preference","habit","relationship","experience"]},"importance":{"type":"number","minimum":0,"maximum":1}},"required":["text","memory_type","importance"],"additionalProperties":false}}}));
    }
    let mut messages = vec![
        serde_json::json!({"role":"system","content":format!("你是对话后的隐性工具审计器。当前时间：{}，时区 Asia/Shanghai。检查最新用户请求和角色回复是否需要调用工具。不要重写或补充用户可见回复；不需要工具时直接返回空文本。提醒时间有实质歧义时不要创建。",chrono::Local::now().format("%Y-%m-%d %H:%M:%S %:z"))}),
    ];
    messages.extend(
        history
            .into_iter()
            .map(|m| serde_json::json!({"role":m.role,"content":m.content})),
    );
    let client = reqwest::Client::new();
    for _ in 0..4 {
        let response=client.post(format!("{}/chat/completions",model.base_url)).bearer_auth(&model.api_key).json(&serde_json::json!({"model":model.model,"messages":messages,"tools":tools,"tool_choice":"auto","temperature":0.1,"thinking":{"type":"disabled"}})).send().await.map_err(|e|format!("工具审计请求失败：{e}"))?;
        if !response.status().is_success() {
            return Err(format!("工具审计返回 {}", response.status()));
        }
        let parsed: ChatResponse = response.json().await.map_err(|e| e.to_string())?;
        let message = parsed
            .choices
            .into_iter()
            .next()
            .ok_or("工具审计无结果")?
            .message;
        let calls = message
            .get("tool_calls")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if calls.is_empty() {
            break;
        }
        messages.push(message);
        for call in calls {
            let id = call
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or("工具调用缺少 id")?;
            let name = call
                .pointer("/function/name")
                .and_then(|v| v.as_str())
                .ok_or("工具调用缺少名称")?;
            let args: serde_json::Value = serde_json::from_str(
                call.pointer("/function/arguments")
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}"),
            )
            .map_err(|e| e.to_string())?;
            let result = match name {
                "create_todo" => {
                    let title = args
                        .get("title")
                        .and_then(|v| v.as_str())
                        .ok_or("提醒事项不能为空")?;
                    let due = chrono::DateTime::parse_from_rfc3339(
                        args.get("due_at")
                            .and_then(|v| v.as_str())
                            .ok_or("提醒缺少时间")?,
                    )
                    .map_err(|_| "提醒时间格式无效")?
                    .timestamp_millis();
                    if due <= chrono::Utc::now().timestamp_millis() {
                        return Err("提醒时间必须晚于当前时间".into());
                    }
                    let mut conn = db_state.0.lock().await;
                    serde_json::to_string(
                        &db::create_todo(
                            &mut conn,
                            title,
                            due,
                            args.get("timezone")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Asia/Shanghai"),
                            Some(source_message_id),
                        )
                        .map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string())?
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
                    match crate::memory::remember(
                        db_state.inner(),
                        &config,
                        text,
                        args.get("memory_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("fact"),
                        args.get("importance")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.5),
                        Some(source_message_id),
                    )
                    .await
                    {
                        Ok(v) => serde_json::to_string(&v).map_err(|e| e.to_string())?,
                        Err(e) => format!("记忆未保存：{e}"),
                    }
                }
                _ => return Err(format!("不允许的工具：{name}")),
            };
            messages.push(serde_json::json!({"role":"tool","tool_call_id":id,"content":result}));
        }
    }
    Ok(())
}

pub async fn process_voice_text(app: AppHandle, content: String) -> Result<Message, String> {
    let db_state = app.state::<DbState>();
    let model = app.state::<ModelConfig>();
    let conversation = app.state::<ConversationState>();
    let memory_env = app.state::<MemoryEnvConfig>();
    let result = process_message(
        &app,
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
    let history = {
        let conn = db_state.0.lock().await;
        db::list_messages(&conn, 20).map_err(|e| e.to_string())?
    };
    let mut messages = vec![
        serde_json::json!({"role":"system","content":format!("你是桌宠{}。人物设定：{}\n这是一次{}触发。请保持角色身份生成简短消息。{}\n一次性输出含义相同的中文和自然日文，并给出情绪。必须调用 reply_to_user 工具完成回复，不要输出普通文本。不要虚构电脑状态。",ROXY_NAME,ROXY_PERSONA,trigger_type,NATURAL_SPEECH_RULES)}),
    ];
    messages.extend(
        history
            .into_iter()
            .map(|m| serde_json::json!({"role":m.role,"content":m.content})),
    );
    messages.push(serde_json::json!({"role":"user","content":event}));
    let client = reqwest::Client::new();
    let reply = request_bilingual_reply(&client, model.inner(), &messages, 0.9).await?;
    let message = Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: "assistant".into(),
        content: reply.chinese_text.trim().to_string(),
        japanese_text: Some(reply.japanese_text.trim().to_string()),
        emotion: Some(normalize_emotion(&reply.emotion)),
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
pub fn start_gpt_sovits(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<crate::gpt_sovits::GptSoVitsState>();
        if let Err(error) = crate::gpt_sovits::ensure(state.inner()).await {
            eprintln!("GPT-SoVITS startup failed: {error}");
        }
    });
}

#[tauri::command]
pub async fn test_voice_output(app: AppHandle) -> Result<(), String> {
    crate::voice_output::test(&app).await
}

async fn voice_input_config(
    app: &AppHandle,
) -> Result<(Option<String>, crate::asr::XfyunCredentials), String> {
    let db_state = app.state::<DbState>();
    let asr_env = app.state::<AsrEnvConfig>();
    let conn = db_state.0.lock().await;
    let settings = db::get_settings(&conn, true).map_err(|e| e.to_string())?;
    let pick = |stored: String, env: &String| {
        if stored.is_empty() {
            env.clone()
        } else {
            stored
        }
    };
    let credentials = crate::asr::XfyunCredentials {
        app_id: pick(settings.asr_app_id, &asr_env.app_id),
        api_key: pick(settings.asr_api_key, &asr_env.api_key),
        api_secret: pick(settings.asr_api_secret, &asr_env.api_secret),
    };
    if !credentials.is_complete() {
        return Err("请先在设置或 .env 中配置完整的讯飞 ASR 凭据".into());
    }
    Ok((settings.microphone_device_name, credentials))
}

#[tauri::command]
pub async fn begin_push_to_talk(app: AppHandle) -> Result<(), String> {
    let (selected, _) = voice_input_config(&app).await?;
    let voice = app.state::<crate::audio::VoiceState>();
    crate::audio::start_push_to_talk(app.clone(), voice.inner(), selected.as_deref()).await
}

#[tauri::command]
pub async fn end_push_to_talk(app: AppHandle) -> Result<(), String> {
    let (_, credentials) = voice_input_config(&app).await?;
    let voice = app.state::<crate::audio::VoiceState>();
    crate::audio::finish_push_to_talk(app.clone(), voice.inner(), credentials).await
}

#[tauri::command]
pub fn set_push_to_talk_shortcut(
    input: State<'_, crate::global_input::GlobalInputState>,
    tokens: Vec<String>,
) -> Result<crate::global_input::CapturedBinding, String> {
    crate::global_input::set_binding(input.inner(), tokens)
}

#[tauri::command]
pub fn begin_shortcut_capture(
    input: State<'_, crate::global_input::GlobalInputState>,
) -> Result<(), String> {
    crate::global_input::begin_capture(input.inner())
}

#[tauri::command]
pub fn cancel_shortcut_capture(input: State<'_, crate::global_input::GlobalInputState>) {
    crate::global_input::cancel_capture(input.inner());
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
pub async fn toggle_proactive_enabled(db_state: State<'_, DbState>) -> Result<bool, String> {
    let conn = db_state.0.lock().await;
    db::toggle_proactive_enabled(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn show_pet_menu(app: AppHandle, x: f64, y: f64) -> Result<(), String> {
    let pet = app
        .get_webview_window("pet")
        .ok_or_else(|| "桌宠窗口不存在".to_string())?;
    let menu = app
        .get_webview_window("pet-menu")
        .ok_or_else(|| "右键菜单窗口不存在".to_string())?;
    let pet_position = pet.outer_position().map_err(|e| e.to_string())?;
    let scale = pet.scale_factor().map_err(|e| e.to_string())?;
    let menu_size = menu.outer_size().map_err(|e| e.to_string())?;

    let mut target_x = pet_position.x + (x * scale).round() as i32 - (4.0 * scale).round() as i32;
    let mut target_y = pet_position.y + (y * scale).round() as i32 - menu_size.height as i32;
    if let Some(monitor) = pet.current_monitor().map_err(|e| e.to_string())? {
        let origin = monitor.position();
        let size = monitor.size();
        let max_x = origin.x + size.width as i32 - menu_size.width as i32;
        let max_y = origin.y + size.height as i32 - menu_size.height as i32;
        target_x = target_x.clamp(origin.x, max_x.max(origin.x));
        target_y = target_y.clamp(origin.y, max_y.max(origin.y));
    }

    menu.set_position(tauri::PhysicalPosition::new(target_x, target_y))
        .map_err(|e| e.to_string())?;
    menu.show().map_err(|e| e.to_string())?;
    menu.set_focus().map_err(|e| e.to_string())
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

#[tauri::command]
pub async fn list_tool_hook_support(
    db_state: State<'_, DbState>,
) -> Result<Vec<crate::tool_hook_config::ToolHookToolInfo>, String> {
    let conn = db_state.0.lock().await;
    let settings = db::get_settings(&conn, true).map_err(|e| e.to_string())?;
    Ok(crate::tool_hook_config::list_supported(&settings))
}

#[tauri::command]
pub async fn write_tool_hook_config(
    db_state: State<'_, DbState>,
    tool: String,
    item: String,
) -> Result<crate::tool_hook_config::ToolHookStatus, String> {
    let conn = db_state.0.lock().await;
    let settings = db::get_settings(&conn, true).map_err(|e| e.to_string())?;
    drop(conn);
    crate::tool_hook_config::write(&tool, &item, &settings)
}

#[tauri::command]
pub async fn remove_tool_hook_config(
    db_state: State<'_, DbState>,
    tool: String,
    item: String,
) -> Result<crate::tool_hook_config::ToolHookStatus, String> {
    let conn = db_state.0.lock().await;
    let settings = db::get_settings(&conn, true).map_err(|e| e.to_string())?;
    drop(conn);
    crate::tool_hook_config::remove(&tool, &item, &settings)
}

#[tauri::command]
pub async fn test_tool_hook(app: AppHandle, tool: String) -> Result<(), String> {
    crate::tool_hook::test_event(&app, &tool).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_model_emotions_for_voice_routing() {
        assert_eq!(normalize_emotion("高兴"), "happy");
        assert_eq!(normalize_emotion("sad"), "sad");
        assert_eq!(normalize_emotion("unknown"), "calm");
    }
}
