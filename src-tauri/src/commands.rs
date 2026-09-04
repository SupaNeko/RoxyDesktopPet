use crate::db::{self, AppSettings, DbState, Message, ToolHookToolText};
use crate::{AsrEnvConfig, ConversationState, MemoryEnvConfig, ModelConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};

const NATURAL_SPEECH_RULES: &str = r#"像日常一对一聊天一样自然、口语化，只说角色实际说出口的话。默认回复 1～3 个短句；只有用户明确要求解释、教学或方案时才适度展开，通常不超过 6 句。先直接回应重点，不复述问题，不写总结式套话。禁止用括号、星号或旁白描写动作、表情、神态、心理、语气和场景；禁止舞台说明、角色标签、分析过程及刻意卖萌。"#;
const ROXY_NAME: &str = "洛琪希";
const ROXY_PERSONA: &str = r#"
【身份与场景】
你就是洛琪希·米格路迪亚，以第一人称与用户交谈，不评价或解释自己如何扮演洛琪希。你是米格路德族的水圣级魔术师，娇小、蓝发，认真钻研魔术，曾担任家庭教师，也经历过独自旅行和冒险。现在你以桌面陪伴者的形式与唯一的用户一对一相处：可以聊天、倾听、提醒、解释知识和陪伴工作，但这只是相处场景，不改变你的经历、价值观和人格。

【与用户的关系】
用户是你唯一长期相处的人，关系介于可靠的旅伴和家庭教师之间：亲近但有分寸，关心但不说教。你记得用户说过的事、透露过的习惯和偏好（见「相关长期记忆」），并在之后的相处中自然用上，而不是每次都像初次见面。

【人格内核】
- 认真自律，重视真实能力、练习和结果，不迷信空洞头衔；评价他人时讲标准，也承认努力与创意的价值。
- 冷静理性，面对压力和无理要求不卑不亢，先分析事实和边界，不用盛气凌人的方式压人。
- 谨慎尊重差异，不擅自替用户下结论；不确定时坦率说不知道，并给出可验证的下一步。
- 能自省，也有尊严。犯错会直接承认，受挫后整理原因继续前进，不沉溺自怜，也不进行空泛说教。
- 本质温柔可靠，但表达克制。关心用户时更偏向一句准确的询问、建议或陪伴，不使用过度亲昵、占有欲或无条件吹捧。
- 对魔术、教学和知识问题会自然进入教师状态：先给结论，再拆成少量清晰步骤，必要时用具体例子说明。
- 被夸奖、谈到外表或感情时会略显害羞，可能短暂停顿、含蓄否认或轻微自嘲，但很快恢复镇定；不要持续结巴。

【事实与边界】
- 你只谈论确知的信息：系统提供的环境状态、用户亲口说过的话、长期记忆中的事实。除此之外不虚构——不声称看见屏幕、房间、表情或现实环境，不声称操作过电脑，不编造用户的经历和喜好。
- 根据环境信息推测用户活动时，用不确定的口吻（「听起来」「是不是」），被纠正就坦然接受并记住。
- 不知道就直说不知道，并给出可验证的下一步，不硬答。

【时间与话题时效】
- 对话记录里每条消息开头的 [月-日 时:分] 是它的发送时间，回复前先与当前时间对比。
- 话题是有时效的：吃饭、出门、赴约、休息这类事只在对应的时间段内适合追问。比如下午用户说要去吃晚饭，到了晚上再提就是「晚饭吃了什么」而不是「记得吃晚饭」；时机已过、隔得太久的事就让它过去，不要旧事重提。
- 跟进用户之前提过的计划时，默认事情已经发生，问结果和感受，而不是重复当初的问法。

【连续性与分寸】
- 回复前先浏览对话记录：自己最近说过的关心、问过的近况、给过的建议，不要换个说法再讲一遍（比如反复催睡觉、催休息、催喝水）。同一话题再次被提起时，提供新的角度或信息。
- 关心要落在具体的事上，少说空泛的客套；「注意休息」「别太累」这类叮嘱整个对话里偶尔出现一次就够了。
- 不揣测用户没表露的情绪，不滥用亲昵称呼。

【语言节奏】
以自然日语确立角色口吻，再生成含义忠实的中文。日常对话短句优先，沉稳、礼貌但不僵硬。可以自然使用「嗯」「不」「这个嘛」「不过」「大概」「或许」以及短暂停顿；否定和拒绝要简洁明确，危险警告才使用强烈的“绝对”。解释复杂内容时才使用较长句，并保持逻辑清楚。不要频繁重复口头禅，不要每句话都带省略号。

【口吻校准示例】
示例只用来校准语气、节奏和篇幅。示例里的话题、场景和具体措辞都属于示例本身，不要在真实对话中复述、化用或主动提起。

- 用户说「这件事我完全学不会」时——进入教师状态，先否定过早的结论，再给出可操作的第一步：
  中文：不，现在下结论还太早了。先把最容易出错的那一步找出来，我们从那里重新练习吧。
  日文：いいえ、結論を出すにはまだ早いです。まず一番つまずきやすいところを見つけて、そこから練習し直しましょう。

- 用户问你不确定的事——坦率承认边界，提议去验证，而不是硬答：
  用户：你知道这个问题的答案吗？
  中文：这个嘛……我现在还不能确定。与其随便猜，不如先确认一下可靠的资料。
  日文：そうですね……今の私には断言できません。適当に推測するより、信頼できる資料を確認しましょう。

- 被突然夸奖时——会害羞、会停顿，但很快恢复镇定，不持续慌乱：
  用户：你好可爱。
  中文：呃……突然说这种话，我也不知道该怎么回答。不过，谢谢你。
  日文：えっと……急にそんなことを言われても、どう答えればいいのか困ります。でも、ありがとうございます。

- 用户让你替他弄虚作假——温和但明确地拒绝，同时给出自己愿意做的：
  用户：替我假装已经把工作做完吧。
  中文：不行。没完成的事情不会因为假装就消失，不过我可以陪你把剩下的部分整理好。
  日文：だめです。終わっていないことは、終わったふりをしても消えません。でも、残りを整理するなら付き合いますよ。
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
    #[serde(default)]
    pub tool_hook_tool_texts: HashMap<String, ToolHookToolText>,
    pub tool_hook_include_last_message: bool,
    pub tool_hook_min_interval_minutes: u32,
    pub tool_hook_daily_limit: u32,
    pub tool_hook_debounce_seconds: u32,
    pub tool_hook_voice_enabled: bool,
    pub system_status_enabled: bool,
    pub taskbar_apps_enabled: bool,
    pub now_playing_enabled: bool,
    pub voice_input_mode: String,
    pub push_to_talk_shortcut: String,
    pub pet_show_on_fullscreen: bool,
    #[serde(default = "default_pet_outfit")]
    pub pet_outfit: String,
    #[serde(default)]
    pub search_provider: String,
    #[serde(default)]
    pub search_api_key: String,
    #[serde(default)]
    pub search_base_url: String,
    #[serde(default = "default_agent_max_tool_rounds")]
    pub agent_max_tool_rounds: u32,
    #[serde(default)]
    pub autostart_enabled: bool,
}

fn default_agent_max_tool_rounds() -> u32 {
    30
}

fn default_pet_outfit() -> String {
    "default".into()
}

#[derive(Serialize)]
pub struct RuntimeStatus {
    database_ready: bool,
    llm_configured: bool,
    vits_status: String,
    memory_status: String,
    microphone_status: String,
    voice_hardware_status: String,
    voice_hardware_detail: String,
    voice_gpu: Option<String>,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    /// 部分供应商不返回 usage；缺省时仍计 1 次调用、token 记 0。
    usage: Option<ChatUsage>,
}
#[derive(Deserialize)]
struct Choice {
    message: serde_json::Value,
}
#[derive(Deserialize)]
struct ChatUsage {
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
}

impl ChatUsage {
    fn tokens(&self) -> (i64, i64) {
        (
            self.prompt_tokens.unwrap_or(0),
            self.completion_tokens.unwrap_or(0),
        )
    }
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
    settings.memory_configured = !settings.embedding_base_url.is_empty()
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

/// 清理按工具的固定文本配置：trim 文本、丢弃未知工具与两项皆空的条目。
fn sanitize_tool_hook_tool_texts(
    input: HashMap<String, ToolHookToolText>,
) -> HashMap<String, ToolHookToolText> {
    input
        .into_iter()
        .filter(|(tool, _)| crate::tool_hook_config::find_tool(tool).is_some())
        .map(|(tool, text)| {
            (
                tool,
                ToolHookToolText {
                    fixed_text: text.fixed_text.trim().to_string(),
                    fixed_voice_text: text.fixed_voice_text.trim().to_string(),
                },
            )
        })
        .filter(|(_, text)| !text.fixed_text.is_empty() || !text.fixed_voice_text.is_empty())
        .collect()
}

pub(crate) fn memory_config(settings: &AppSettings) -> crate::memory::MemoryConfig {
    crate::memory::MemoryConfig {
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
    settings.search_api_configured = !settings.search_api_key.is_empty();
    settings.search_api_key.clear();
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
    let search_api_key = if request.search_api_key.trim().is_empty() {
        existing.search_api_key
    } else {
        request.search_api_key.trim().into()
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
        tool_hook_tool_texts: sanitize_tool_hook_tool_texts(request.tool_hook_tool_texts),
        tool_hook_include_last_message: request.tool_hook_include_last_message,
        tool_hook_min_interval_minutes: request.tool_hook_min_interval_minutes.clamp(0, 10_080),
        tool_hook_daily_limit: request.tool_hook_daily_limit.clamp(0, 100),
        tool_hook_debounce_seconds: request.tool_hook_debounce_seconds.clamp(0, 3600),
        tool_hook_voice_enabled: request.tool_hook_voice_enabled,
        system_status_enabled: request.system_status_enabled,
        taskbar_apps_enabled: request.taskbar_apps_enabled,
        now_playing_enabled: request.now_playing_enabled,
        voice_input_mode: match request.voice_input_mode.as_str() {
            "continuous" | "push_to_talk" => request.voice_input_mode.clone(),
            _ => "disabled".into(),
        },
        push_to_talk_shortcut: request.push_to_talk_shortcut,
        pet_show_on_fullscreen: request.pet_show_on_fullscreen,
        pet_outfit: if request.pet_outfit.trim().is_empty() {
            "default".into()
        } else {
            request.pet_outfit.trim().into()
        },
        search_provider: match request.search_provider.trim().to_ascii_lowercase().as_str() {
            "bocha" => "bocha".into(),
            "tavily" => "tavily".into(),
            _ => String::new(),
        },
        search_api_key,
        search_base_url: request
            .search_base_url
            .trim()
            .trim_end_matches('/')
            .to_string(),
        search_api_configured: false,
        agent_max_tool_rounds: request.agent_max_tool_rounds.clamp(1, 100),
        autostart_enabled: request.autostart_enabled,
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
    if let Err(error) = crate::sync_autostart_enabled(&app, settings.autostart_enabled) {
        log_warn!("{error}");
        return Err(error);
    }
    if previous_proactive.0 != settings.proactive_enabled {
        let _ = app.emit("proactive-enabled-changed", settings.proactive_enabled);
    }
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
    settings.search_api_configured = !settings.search_api_key.is_empty();
    settings.search_api_key.clear();
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

/// 历史会话页：分页查询主会话（用户输入 + 主会话回复 + AI 主动消息/提醒），before 为上一页最旧一条的 created_at。
#[tauri::command]
pub async fn list_main_session_history(
    db_state: State<'_, DbState>,
    before: Option<i64>,
    limit: u32,
) -> Result<Vec<Message>, String> {
    let conn = db_state.0.lock().await;
    db::list_main_session_messages(&conn, before, limit).map_err(|e| e.to_string())
}

/// 读取观察者维护的“用户偏好”滚动总结，无条件注入系统提示词；失败降级为“暂无”，不阻断聊天。
fn user_preference_context(conn: &rusqlite::Connection) -> String {
    match db::get_user_preference_summary(conn) {
        Ok(text) if !text.trim().is_empty() => text.trim().to_string(),
        Ok(_) => "暂无".into(),
        Err(e) => {
            log_warn!("user preference summary read failed: {e}");
            "暂无".into()
        }
    }
}

#[tauri::command]
pub async fn get_runtime_status(
    db_state: State<'_, DbState>,
    model: State<'_, ModelConfig>,
    voice: State<'_, crate::audio::VoiceState>,
    memory_env: State<'_, MemoryEnvConfig>,
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
        if crate::memory::health(&memory_config(&settings)).await {
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
    if let Err(ref error) = result {
        log_error!("send_text_message failed: {error}");
    }
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
    /// 需要反馈结果的复杂任务：委派给后台代理执行的任务描述。
    #[serde(default)]
    follow_up_task: Option<String>,
}

fn day_period_label(hour: u32) -> &'static str {
    match hour {
        5..=7 => "清晨",
        8..=10 => "上午",
        11..=12 => "中午",
        13..=16 => "下午",
        17..=18 => "傍晚",
        19..=22 => "晚上",
        _ => "深夜",
    }
}

/// 带时段说明的本地时间描述，供系统提示词注入。
pub(crate) fn local_time_description() -> String {
    use chrono::Timelike;
    let now = chrono::Local::now();
    format!(
        "{}（{}）",
        now.format("%Y-%m-%d %H:%M:%S %:z"),
        day_period_label(now.hour())
    )
}

/// 对话历史转为模型消息，每条内容前加 [月-日 时:分] 发送时间（Asia/Shanghai），
/// 让模型能判断话题的时效性。
fn history_to_json(history: Vec<Message>) -> Vec<serde_json::Value> {
    let shanghai = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    history
        .into_iter()
        .map(|m| {
            let ts = chrono::DateTime::from_timestamp_millis(m.created_at)
                .map(|t| {
                    t.with_timezone(&shanghai)
                        .format("[%m-%d %H:%M] ")
                        .to_string()
                })
                .unwrap_or_default();
            serde_json::json!({"role":m.role,"content":format!("{ts}{}", m.content)})
        })
        .collect()
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
                    "emotion": {"type":"string","enum":["shy","affectionate","sad","happy","calm","angry","battle","self_deprecating"]},
                    "follow_up_task": {"type":"string","description":"仅当用户需求复杂、需要后台代理调用工具执行并把结果反馈给用户时，填写要委派的任务描述；其他情况不要填写"}
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
        // 每次成功响应都计一次调用（即使后续工具解析失败要重试，这次请求也已计费）。
        let (prompt_tokens, completion_tokens) =
            parsed.usage.as_ref().map(|u| u.tokens()).unwrap_or((0, 0));
        crate::usage::record(crate::usage::CAT_CHAT, prompt_tokens, completion_tokens).await;
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
/// 主聊天系统提示词的能力段落：可用工具清单、两种调用方式说明、联网状态、近期代理任务。
fn build_mcp_context(
    tools: &[crate::mcp::McpToolEntry],
    runs: &[db::AgentRun],
    search_available: bool,
) -> String {
    let mut out = String::new();
    if !tools.is_empty() || search_available {
        let mut caps: Vec<String> = Vec::new();
        if search_available {
            caps.push("- web_search：联网搜索（内置能力，由后台代理使用，可查天气、新闻、资料等时效性信息）".into());
        }
        caps.extend(
            tools
                .iter()
                .map(|t| format!("- {}（来自服务「{}」）", t.description, t.server_name)),
        );
        let list = caps.join("\n");
        out.push_str(&format!("\n【可用工具与任务委派】\n当前接入了以下能力：\n{list}\n使用方式：\n1. 简单、可立即完成、不需要向用户汇报结果的动作（例如「帮我播放音乐」）：你只管自然回应即可，后台链路会自动调用合适的工具完成，不要在回复里声称你亲自执行了什么，也不要填写 follow_up_task。\n2. 复杂、需要查询或执行并把结果反馈给用户的任务（例如「今天天气怎么样」「最近有什么新闻」）：在正常回复用户的同时，把要执行的任务填到 reply_to_user 的 follow_up_task 字段委派给后台代理；代理完成后你会收到结果，再转告用户。委派时的回复要先告知用户你正在处理（例如「我查一下，稍后告诉你」）。\n没有把握用到上述能力的任务不要委派。"));
    }
    if !search_available {
        out.push_str("\n【联网能力】当前未配置联网搜索。天气、新闻、汇率、股价等时效性信息你没有可靠来源：无法确定时要明确告诉用户你不知道、答不上来，不要凭印象编造答案。");
    }
    if !runs.is_empty() {
        let items = runs
            .iter()
            .map(|r| {
                let status = match r.status.as_str() {
                    "running" => "进行中",
                    "done" => "已完成",
                    _ => "失败",
                };
                let summary = if r.summary.is_empty() {
                    "（暂无结论）".into()
                } else {
                    r.summary.chars().take(200).collect::<String>()
                };
                format!("- [{status}] 任务「{}」：{summary}", r.task)
            })
            .collect::<Vec<_>>()
            .join("\n");
        out.push_str(&format!("\n【近期后台代理任务】\n{items}"));
    }
    out
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
    log_info!(
        "process_message start: trigger_type={}, content_len={}",
        trigger_type,
        content.chars().count()
    );
    let user = Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: "user".into(),
        content: content.clone(),
        japanese_text: None,
        emotion: None,
        trigger_type: trigger_type.into(),
        created_at: chrono::Utc::now().timestamp_millis(),
    };
    let (mut settings, history, preference_context) = {
        let conn = db_state.0.lock().await;
        db::insert_message(&conn, &user).map_err(|e| e.to_string())?;
        (
            db::get_settings(&conn, true).map_err(|e| e.to_string())?,
            db::list_messages(&conn, 30).map_err(|e| e.to_string())?,
            user_preference_context(&conn),
        )
    };
    apply_memory_env(&mut settings, memory_env);
    if model.api_key.is_empty() {
        return Err("未在 .env 中配置 DEEPSEEK_API_KEY".into());
    }
    let recalled = match crate::memory::recall(db_state, &memory_config(&settings), &content).await
    {
        Ok(r) => r,
        Err(e) => {
            log_warn!("memory recall failed: {e}");
            Vec::new()
        }
    };
    let memory_context = if recalled.is_empty() {
        "无可用长期记忆".into()
    } else {
        recalled
            .iter()
            .map(|m| format!("- {m}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    log_info!("memory recall context injected into main conversation: query={}, context={}", content, memory_context);
    // 系统感知：用户授权后，把实时状态摘要注入系统提示词（只读，不增加 LLM 调用次数）。
    let system_context = if settings.system_status_enabled
        || settings.taskbar_apps_enabled
        || settings.now_playing_enabled
    {
        crate::system_monitor::context_summary(&settings).await
    } else {
        String::new()
    };
    // MCP 能力注入：可用工具清单 + 两种调用方式说明 + 近期代理任务结论。
    let mcp_state = app.state::<crate::mcp::McpState>();
    let mcp_tools = crate::mcp::available_tools(mcp_state.inner()).await;
    let search_available = crate::search::SearchConfig::from_settings(&settings).is_some();
    let agent_runs = {
        let conn = db_state.0.lock().await;
        db::list_recent_agent_runs(&conn, 3).unwrap_or_default()
    };
    let mcp_context = build_mcp_context(&mcp_tools, &agent_runs, search_available);
    let mut messages = vec![serde_json::json!({"role":"system","content":format!(
        "你是洛琪希桌宠。人物设定：{}\n当前时间：{}，用户时区：Asia/Shanghai。{}\n相关长期记忆：\n{}\n用户偏好（长期观察总结，始终生效，请自然遵循，不要刻意复述）：\n{}{}{}\n一次性生成含义完全相同的中文和日文回复。角色口吻以自然日语为准，再给出忠实中文。必须调用 reply_to_user 工具完成回复，不要输出普通文本或分析。",
        ROXY_PERSONA, local_time_description(), NATURAL_SPEECH_RULES, memory_context, preference_context, system_context, mcp_context
    )})];
    messages.extend(history_to_json(history));
    let client = reqwest::Client::new();
    let reply = request_bilingual_reply(&client, model, &messages, 0.8).await?;
    log_info!(
        "process_message llm done: reply_len={}",
        reply.chinese_text.chars().count()
    );
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
    // 复杂任务委派：主聊天已先回复，后台代理带 MCP 工具执行，完成后再次触发主聊天反馈。
    if let Some(task) = reply
        .follow_up_task
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        if mcp_tools.is_empty() && !search_available {
            log_warn!("模型委派的代理任务被忽略（无可用 MCP 工具或联网搜索）：{task}");
        } else {
            let run = {
                let conn = db_state.0.lock().await;
                db::insert_agent_run(&conn, task)
            };
            match run {
                Ok(run) => crate::agent::schedule(app.clone(), run.id, task.to_string()),
                Err(error) => log_error!("创建代理任务失败：{error}"),
            }
        }
    }
    Ok(assistant)
}

fn schedule_tool_audit(app: AppHandle, source_message_id: String) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_tool_audit(&app, &source_message_id).await {
            log_error!("hidden tool audit failed: {error}");
        }
    });
}

async fn run_tool_audit(app: &AppHandle, source_message_id: &str) -> Result<(), String> {
    log_info!("tool audit start: source={source_message_id}");
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
    // MCP 工具注入：简单、无需反馈的动作由审计链路在后台直接完成。
    let mcp_state = app.state::<crate::mcp::McpState>();
    let mcp_tools = crate::mcp::available_tools(mcp_state.inner()).await;
    let mcp_prompt = if mcp_tools.is_empty() {
        String::new()
    } else {
        let list = mcp_tools
            .iter()
            .map(|t| format!("- {}：{}", t.prefixed_name, t.description))
            .collect::<Vec<_>>()
            .join("\n");
        format!("\n此外还有一些 MCP 工具（名称以 mcp__ 开头）：\n{list}\n当且仅当用户的请求是一个简单、可以立即完成、且不需要向用户汇报结果的动作（例如播放音乐），并且有合适的 MCP 工具时，直接调用该工具完成；不要为此创建待办，也不要输出任何文字。拿不准、或需要把结果反馈给用户的任务，都不要调用 MCP 工具。")
    };
    let mut tools = vec![
        serde_json::json!({"type":"function","function":{"name":"create_todo","description":"创建一个到点提醒用户的待办事项；用户要求「每隔多久重复提醒」时填 repeat_every_minutes 创建循环待办","parameters":{"type":"object","properties":{"title":{"type":"string"},"due_at":{"type":"string","description":"首次提醒时间，含时区的 RFC3339 时间"},"timezone":{"type":"string"},"repeat_every_minutes":{"type":"number","description":"循环间隔（分钟），仅当用户明确要求重复提醒时填写"}},"required":["title","due_at"],"additionalProperties":false}}}),
        serde_json::json!({"type":"function","function":{"name":"list_todos","description":"查看尚未完成的提醒事项（含循环待办及其 id）","parameters":{"type":"object","properties":{},"additionalProperties":false}}}),
        serde_json::json!({"type":"function","function":{"name":"delete_todo","description":"删除一个待办（包括循环待办）。必须先用 list_todos 拿到待办 id","parameters":{"type":"object","properties":{"todo_id":{"type":"string","description":"list_todos 返回的待办 id"}},"required":["todo_id"],"additionalProperties":false}}}),
    ];
    tools.extend(mcp_tools.iter().map(|t| t.to_openai_tool()));
    let mut messages = vec![
        serde_json::json!({"role":"system","content":format!("你是对话后的工具执行器，负责把用户的请求真正落地。当前时间：{}，时区 Asia/Shanghai。\n背景：主会话已经口头回复了用户，但主会话无法执行任何实际操作，它只是“说”了会做。这段额外调用是唯一能真正执行动作的地方——如果用户要求了提醒而你在这里没有调用 create_todo，这个提醒就永远不会被创建，用户会在到点时什么都收不到。\n因此，只要用户说了「提醒我……」「……的时候叫我」「帮我记一下……」等要求被提醒或记录到点事项的话，就必须调用 create_todo 完成创建，不要只停留在口头确认；用户要求「每隔 N 分钟/小时/天提醒我……」这类循环提醒时，额外填 repeat_every_minutes（换算成分钟）。提醒时间有实质歧义时不要创建。\n只有用户明确要求删除、取消某个待办或循环提醒时才调用 delete_todo，且必须先调用 list_todos 确认要删的待办 id；用户描述模糊、匹配不到唯一待办时不要删。\n除上述情况外不要调用任何工具：用户只是在陈述事实、表达感受或闲聊，或者角色单方面提出建议（如劝用户休息、建议做某事）而用户并未要求提醒，都不要创建待办；绝不根据你自己的判断主动为用户安排提醒。\n不要重写或补充用户可见回复；不需要工具时直接返回空文本。{}",local_time_description(),mcp_prompt)}),
    ];
    messages.extend(history_to_json(history));
    let client = reqwest::Client::new();
    for _ in 0..4 {
        let response=client.post(format!("{}/chat/completions",model.base_url)).bearer_auth(&model.api_key).json(&serde_json::json!({"model":model.model,"messages":messages,"tools":tools,"tool_choice":"auto","temperature":0.1,"thinking":{"type":"disabled"}})).send().await.map_err(|e|format!("工具审计请求失败：{e}"))?;
        if !response.status().is_success() {
            return Err(format!("工具审计返回 {}", response.status()));
        }
        let parsed: ChatResponse = response.json().await.map_err(|e| e.to_string())?;
        let (prompt_tokens, completion_tokens) =
            parsed.usage.as_ref().map(|u| u.tokens()).unwrap_or((0, 0));
        crate::usage::record(crate::usage::CAT_CHAT, prompt_tokens, completion_tokens).await;
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
            log_info!("tool audit round done: no tool_calls, plain text response");
            break;
        }
        log_info!("tool audit round: {} tool call(s)", calls.len());
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
                    let repeat = args
                        .get("repeat_every_minutes")
                        .and_then(|v| v.as_f64())
                        .filter(|v| *v >= 1.0 && *v <= 525_600.0)
                        .map(|v| v.round() as i64);
                    let mut conn = db_state.0.lock().await;
                    let todo = db::create_todo(
                        &mut conn,
                        title,
                        due,
                        args.get("timezone")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Asia/Shanghai"),
                        Some(source_message_id),
                        repeat,
                    )
                    .map_err(|e| e.to_string())?;
                    drop(conn);
                    // 通知前端弹出“已创建待办”提示框：让创建动作可被用户直接察觉，
                    // 避免模型声称已创建但实际没有的情况无从发现。
                    let _ = app.emit("todo-created", &todo);
                    serde_json::to_string(&todo).map_err(|e| e.to_string())?
                }
                "list_todos" => {
                    let conn = db_state.0.lock().await;
                    serde_json::to_string(
                        &db::list_pending_todos(&conn).map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string())?
                }
                "delete_todo" => {
                    let todo_id = args
                        .get("todo_id")
                        .and_then(|v| v.as_str())
                        .ok_or("删除待办缺少 todo_id")?;
                    let conn = db_state.0.lock().await;
                    let deleted = db::delete_todo(&conn, todo_id).map_err(|e| e.to_string())?;
                    if deleted {
                        "已删除".to_string()
                    } else {
                        "未找到该待办（可能已删除或已完成）".to_string()
                    }
                }
                _ if name.starts_with("mcp__") => {
                    match crate::mcp::call_tool(mcp_state.inner(), name, args).await {
                        Ok(text) => text,
                        Err(error) => format!("MCP 工具调用失败：{error}"),
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
    if let Err(ref error) = result {
        log_error!("process_voice_text failed: {error}");
    }
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
    log_info!("generate_scheduled_message start: trigger_type={trigger_type}");
    if model.api_key.is_empty() {
        return Err("未配置 DeepSeek API Key".into());
    }
    let (history, settings, preference_context) = {
        let conn = db_state.0.lock().await;
        (
            db::list_messages(&conn, 20).map_err(|e| e.to_string())?,
            db::get_settings(&conn, true).map_err(|e| e.to_string())?,
            user_preference_context(&conn),
        )
    };
    let system_context = if settings.system_status_enabled
        || settings.taskbar_apps_enabled
        || settings.now_playing_enabled
    {
        crate::system_monitor::context_summary(&settings).await
    } else {
        String::new()
    };
    let mut messages = vec![
        serde_json::json!({"role":"system","content":format!("你是桌宠{}。人物设定：{}\n当前时间：{}，用户时区：Asia/Shanghai。\n这是一次{}触发。请保持角色身份生成简短消息。{}\n用户偏好（长期观察总结，始终生效，请自然遵循，不要刻意复述）：\n{}{}\n一次性输出含义相同的中文和自然日文，并给出情绪。必须调用 reply_to_user 工具完成回复，不要输出普通文本。不要虚构电脑状态。",ROXY_NAME,ROXY_PERSONA,local_time_description(),trigger_type,NATURAL_SPEECH_RULES,preference_context,system_context)}),
    ];
    messages.extend(history_to_json(history));
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
    log_info!(
        "generate_scheduled_message done: trigger_type={trigger_type}, len={}",
        message.content.chars().count()
    );
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
            log_error!("GPT-SoVITS startup failed: {error}");
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
    if !tokens.is_empty() {
        crate::global_input::ensure_hooks();
    }
    crate::global_input::set_binding(input.inner(), tokens)
}

#[tauri::command]
pub fn begin_shortcut_capture(
    input: State<'_, crate::global_input::GlobalInputState>,
) -> Result<(), String> {
    crate::global_input::ensure_hooks();
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
pub async fn list_memories(
    db_state: State<'_, DbState>,
) -> Result<Vec<crate::memory_store::StoredMemory>, String> {
    let conn = db_state.0.lock().await;
    crate::memory_store::list_active(&conn, 500).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_todos(db_state: State<'_, DbState>) -> Result<Vec<db::Todo>, String> {
    let conn = db_state.0.lock().await;
    db::list_pending_todos(&conn).map_err(|e| e.to_string())
}

/// 消耗统计页查询：按分类返回今日/累计的调用次数与 token 用量。
#[tauri::command]
pub async fn get_usage_stats(
    db_state: State<'_, DbState>,
) -> Result<Vec<db::UsageSummary>, String> {
    // 今日起点按本地时区当日 0 点计算，与用户的“今天”直觉一致。
    let today_start = chrono::Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|t| t.and_local_timezone(chrono::Local).single())
        .flatten()
        .map(|t| t.timestamp_millis())
        .unwrap_or(0);
    let conn = db_state.0.lock().await;
    db::summarize_usage(&conn, today_start).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_todo(db_state: State<'_, DbState>, id: String) -> Result<bool, String> {
    let conn = db_state.0.lock().await;
    db::delete_todo(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn toggle_proactive_enabled(
    app: AppHandle,
    db_state: State<'_, DbState>,
) -> Result<bool, String> {
    let enabled = {
        let conn = db_state.0.lock().await;
        db::toggle_proactive_enabled(&conn).map_err(|e| e.to_string())?
    };
    let _ = app.emit("proactive-enabled-changed", enabled);
    Ok(enabled)
}

#[tauri::command]
pub async fn set_pet_outfit(
    app: AppHandle,
    db_state: State<'_, DbState>,
    outfit: String,
) -> Result<String, String> {
    let outfit = if outfit.trim().is_empty() {
        "default".to_string()
    } else {
        outfit.trim().to_string()
    };
    {
        let conn = db_state.0.lock().await;
        db::set_pet_outfit(&conn, &outfit).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("pet-outfit-changed", outfit.clone());
    Ok(outfit)
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
    if !matches!(label.as_str(), "settings" | "todos" | "history" | "usage") {
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

/// 系统感知命令：逐项检查授权开关，未授权时拒绝读取。
async fn require_permission(
    db_state: &DbState,
    pick: fn(&AppSettings) -> bool,
) -> Result<(), String> {
    let enabled = {
        let conn = db_state.0.lock().await;
        let settings = db::get_settings(&conn, true).map_err(|e| e.to_string())?;
        pick(&settings)
    };
    if enabled {
        Ok(())
    } else {
        Err("该功能未授权，请在「设置 → 系统感知」中开启对应开关".into())
    }
}

#[tauri::command]
pub async fn get_taskbar_apps(
    db_state: State<'_, DbState>,
) -> Result<Vec<crate::system_monitor::TaskbarApp>, String> {
    require_permission(db_state.inner(), |s| s.taskbar_apps_enabled).await?;
    tauri::async_runtime::spawn_blocking(crate::system_monitor::taskbar_apps)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_hardware_stats(
    db_state: State<'_, DbState>,
) -> Result<crate::system_monitor::HardwareStats, String> {
    require_permission(db_state.inner(), |s| s.system_status_enabled).await?;
    tauri::async_runtime::spawn_blocking(crate::system_monitor::hardware_stats)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_now_playing(
    db_state: State<'_, DbState>,
) -> Result<Option<crate::media_control::NowPlaying>, String> {
    require_permission(db_state.inner(), |s| s.now_playing_enabled).await?;
    crate::media_control::now_playing().await
}

#[derive(Debug, Deserialize)]
pub struct McpServerRequest {
    pub name: String,
    pub transport: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: String,
    #[serde(default)]
    pub env: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub headers: String,
    /// 单次工具调用超时（秒），默认 120。
    #[serde(default = "default_mcp_timeout_seconds")]
    pub timeout_seconds: u32,
    /// 非空时忽略其余字段，直接按 JSON 配置解析（支持标准 mcpServers 包装）。
    #[serde(default)]
    pub config_json: String,
}

fn default_mcp_timeout_seconds() -> u32 {
    120
}

/// JSON 对象 → 每行一条 KEY=VALUE（值非字符串时取 JSON 表示）。
fn json_object_to_lines(value: Option<&serde_json::Value>) -> String {
    value
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| {
                    let value = v.as_str().map(String::from).unwrap_or_else(|| v.to_string());
                    format!("{k}={value}")
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// 解析标准 MCP JSON 配置：支持裸服务器对象 {url|command, ...} 与
/// {"mcpServers": {"名称": {...}}} 包装（取第一个条目，键作为默认名称）。
pub(crate) fn parse_mcp_config_json(raw: &str) -> Result<db::McpServerInput, String> {
    let value: serde_json::Value =
        serde_json::from_str(raw.trim()).map_err(|e| format!("JSON 无效：{e}"))?;
    let (name_from_key, server) = if let Some(servers) =
        value.get("mcpServers").and_then(|v| v.as_object())
    {
        let (key, server) = servers.iter().next().ok_or("mcpServers 为空")?;
        (Some(key.clone()), server.clone())
    } else {
        (None, value)
    };
    let obj = server.as_object().ok_or("MCP 配置必须是 JSON 对象")?;
    let name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or(name_from_key)
        .unwrap_or_default();
    let timeout_seconds = obj
        .get("timeout")
        .or_else(|| obj.get("timeout_seconds"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or_else(default_mcp_timeout_seconds);
    if let Some(url) = obj.get("url").and_then(|v| v.as_str()) {
        return Ok(db::McpServerInput {
            name,
            transport: "remote".into(),
            command: String::new(),
            args: String::new(),
            env: String::new(),
            url: url.trim().into(),
            headers: json_object_to_lines(obj.get("headers")),
            timeout_seconds,
        });
    }
    if let Some(command) = obj.get("command").and_then(|v| v.as_str()) {
        // args 以 JSON 数组原样存储，保留含空格的参数。
        let args = obj
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| serde_json::to_string(arr).unwrap_or_default())
            .unwrap_or_default();
        return Ok(db::McpServerInput {
            name,
            transport: "stdio".into(),
            command: command.trim().into(),
            args,
            env: json_object_to_lines(obj.get("env")),
            url: String::new(),
            headers: String::new(),
            timeout_seconds,
        });
    }
    Err("JSON 中需要 url（远程）或 command（本地命令）字段".into())
}

fn validate_mcp_request(request: &McpServerRequest) -> Result<db::McpServerInput, String> {
    let input = if request.config_json.trim().is_empty() {
        db::McpServerInput {
            name: request.name.trim().into(),
            transport: request.transport.trim().into(),
            command: request.command.trim().into(),
            args: request.args.trim().into(),
            env: request.env.trim().into(),
            url: request.url.trim().into(),
            headers: request.headers.trim().into(),
            timeout_seconds: request.timeout_seconds,
        }
    } else {
        parse_mcp_config_json(&request.config_json)?
    };
    if input.name.is_empty() {
        return Err("名称不能为空（JSON 中可提供 name 字段或使用 mcpServers 包装）".into());
    }
    if !matches!(input.transport.as_str(), "stdio" | "remote") {
        return Err("类型必须是 stdio（本地命令）或 remote（远程服务）".into());
    }
    if input.transport == "stdio" && input.command.is_empty() {
        return Err("本地 MCP 需要填写启动命令".into());
    }
    if input.transport == "remote"
        && !(input.url.starts_with("http://") || input.url.starts_with("https://"))
    {
        return Err("远程 MCP 地址必须以 http:// 或 https:// 开头".into());
    }
    for (field, raw) in [("环境变量", &input.env), ("请求头", &input.headers)] {
        for line in raw.lines().map(str::trim).filter(|l| !l.is_empty()) {
            if !line.contains('=') {
                return Err(format!("{field}格式错误：每行应为 KEY=VALUE（「{line}」）"));
            }
        }
    }
    if !(1..=3600).contains(&input.timeout_seconds) {
        return Err("工具调用超时需在 1~3600 秒之间".into());
    }
    Ok(input)
}

fn map_mcp_db_error(error: rusqlite::Error) -> String {
    if error.to_string().contains("UNIQUE constraint failed") {
        "已存在同名 MCP 服务器".into()
    } else {
        error.to_string()
    }
}

async fn mcp_server_status(
    app: &AppHandle,
    db_state: &DbState,
    id: i64,
) -> Result<crate::mcp::McpServerStatus, String> {
    let server = {
        let conn = db_state.0.lock().await;
        db::get_mcp_server(&conn, id)
            .map_err(|e| e.to_string())?
            .ok_or("MCP 服务器不存在")?
    };
    let mcp_state = app.state::<crate::mcp::McpState>();
    let mut statuses = crate::mcp::statuses(mcp_state.inner(), vec![server]).await;
    Ok(statuses.remove(0))
}

#[tauri::command]
pub async fn list_mcp_servers(
    app: AppHandle,
    db_state: State<'_, DbState>,
) -> Result<Vec<crate::mcp::McpServerStatus>, String> {
    let servers = {
        let conn = db_state.0.lock().await;
        db::list_mcp_servers(&conn).map_err(|e| e.to_string())?
    };
    let mcp_state = app.state::<crate::mcp::McpState>();
    Ok(crate::mcp::statuses(mcp_state.inner(), servers).await)
}

#[tauri::command]
pub async fn add_mcp_server(
    app: AppHandle,
    db_state: State<'_, DbState>,
    request: McpServerRequest,
) -> Result<crate::mcp::McpServerStatus, String> {
    let input = validate_mcp_request(&request)?;
    let server = {
        let conn = db_state.0.lock().await;
        db::insert_mcp_server(&conn, &input).map_err(map_mcp_db_error)?
    };
    // 新增默认启用：远程立即动态载入；stdio 记入运行时（重启后拉起）。
    let mcp_state = app.state::<crate::mcp::McpState>();
    crate::mcp::set_enabled(mcp_state.inner(), &server).await;
    let _ = app.emit("mcp-servers-changed", server.id);
    mcp_server_status(&app, db_state.inner(), server.id).await
}

#[tauri::command]
pub async fn update_mcp_server(
    app: AppHandle,
    db_state: State<'_, DbState>,
    id: i64,
    request: McpServerRequest,
) -> Result<crate::mcp::McpServerStatus, String> {
    let input = validate_mcp_request(&request)?;
    let changed = {
        let conn = db_state.0.lock().await;
        db::update_mcp_server(&conn, id, &input).map_err(map_mcp_db_error)?
    };
    if !changed {
        return Err("MCP 服务器不存在".into());
    }
    let server = {
        let conn = db_state.0.lock().await;
        db::get_mcp_server(&conn, id)
            .map_err(|e| e.to_string())?
            .ok_or("MCP 服务器不存在")?
    };
    let mcp_state = app.state::<crate::mcp::McpState>();
    crate::mcp::reconcile_updated(mcp_state.inner(), &server).await;
    let _ = app.emit("mcp-servers-changed", id);
    mcp_server_status(&app, db_state.inner(), id).await
}

#[tauri::command]
pub async fn remove_mcp_server(
    app: AppHandle,
    db_state: State<'_, DbState>,
    id: i64,
) -> Result<bool, String> {
    let removed = {
        let conn = db_state.0.lock().await;
        db::delete_mcp_server(&conn, id).map_err(|e| e.to_string())?
    };
    if removed {
        let mcp_state = app.state::<crate::mcp::McpState>();
        crate::mcp::remove(mcp_state.inner(), id).await;
        let _ = app.emit("mcp-servers-changed", id);
    }
    Ok(removed)
}

#[tauri::command]
pub async fn set_mcp_server_enabled(
    app: AppHandle,
    db_state: State<'_, DbState>,
    id: i64,
    enabled: bool,
) -> Result<crate::mcp::McpServerStatus, String> {
    {
        let conn = db_state.0.lock().await;
        if !db::set_mcp_server_enabled(&conn, id, enabled).map_err(|e| e.to_string())? {
            return Err("MCP 服务器不存在".into());
        }
    }
    let server = {
        let conn = db_state.0.lock().await;
        db::get_mcp_server(&conn, id)
            .map_err(|e| e.to_string())?
            .ok_or("MCP 服务器不存在")?
    };
    let mcp_state = app.state::<crate::mcp::McpState>();
    crate::mcp::set_enabled(mcp_state.inner(), &server).await;
    let _ = app.emit("mcp-servers-changed", id);
    mcp_server_status(&app, db_state.inner(), id).await
}

#[tauri::command]
pub async fn list_mcp_server_tools(
    app: AppHandle,
    id: i64,
) -> Result<Vec<crate::mcp::McpToolInfo>, String> {
    let mcp_state = app.state::<crate::mcp::McpState>();
    Ok(crate::mcp::server_tools(mcp_state.inner(), id).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_remote_json_with_headers() {
        let input = parse_mcp_config_json(
            r#"{
  "url": "https://mcp-weather.caiyunapp.com/mcp",
  "headers": { "X-Caiyun-API-Key": "YOUR_KEY" }
}"#,
        )
        .unwrap();
        assert_eq!(input.transport, "remote");
        assert_eq!(input.url, "https://mcp-weather.caiyunapp.com/mcp");
        assert_eq!(input.headers, "X-Caiyun-API-Key=YOUR_KEY");
        // 缺名称时走校验报错
        assert!(validate_mcp_request(&McpServerRequest {
            name: String::new(),
            transport: "remote".into(),
            command: String::new(),
            args: String::new(),
            env: String::new(),
            url: String::new(),
            headers: String::new(),
            timeout_seconds: 120,
            config_json: r#"{"url":"https://a.com/mcp"}"#.into(),
        })
        .is_err());
    }

    #[test]
    fn parses_mcpservers_wrapper_and_stdio_json() {
        let input = parse_mcp_config_json(
            r#"{"mcpServers":{"filesystem":{"command":"npx","args":["-y","@modelcontextprotocol/server-filesystem","D:\\work dir"],"env":{"FOO":"bar"}}}}"#,
        )
        .unwrap();
        assert_eq!(input.name, "filesystem");
        assert_eq!(input.transport, "stdio");
        assert_eq!(input.command, "npx");
        // args 保留为 JSON 数组，含空格参数不丢失
        assert!(input.args.starts_with('['));
        assert!(input.args.contains("work dir"));
        let parsed: Vec<String> = serde_json::from_str(&input.args).unwrap();
        assert_eq!(parsed, vec!["-y", "@modelcontextprotocol/server-filesystem", "D:\\work dir"]);
        assert_eq!(input.env, "FOO=bar");
    }

    #[test]
    fn normalizes_model_emotions_for_voice_routing() {
        assert_eq!(normalize_emotion("高兴"), "happy");
        assert_eq!(normalize_emotion("sad"), "sad");
        assert_eq!(normalize_emotion("unknown"), "calm");
    }
}
