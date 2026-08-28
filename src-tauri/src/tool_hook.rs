use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

use crate::db::{self, AppSettings, DbState, Message};
use crate::hook_server;

#[derive(Debug, Deserialize)]
pub struct HookEvent {
    pub tool: String,
    pub event: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub last_assistant_message: String,
}

#[derive(Default)]
pub struct ToolHookState {
    running: AtomicBool,
    handle: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    active_config: Mutex<Option<u32>>,
    last_notify: Mutex<HashMap<String, i64>>,
    debounce_gen: Mutex<HashMap<String, u64>>,
    daily: Mutex<(String, u32)>,
}

/// 根据当前设置启动/停止/重启监听器。应用启动与保存设置后调用。
pub async fn reconcile_server(app: &AppHandle) {
    let settings = {
        let db = app.state::<DbState>();
        let conn = db.0.lock().await;
        match db::get_settings(&conn, true) {
            Ok(s) => s,
            Err(e) => {
                log_error!("tool_hook reconcile: 读取设置失败: {e}");
                return;
            }
        }
    };
    log_info!(
        "tool_hook reconcile: enabled={}, port={}",
        settings.tool_hook_enabled,
        settings.tool_hook_port
    );
    let state = app.state::<ToolHookState>();
    let want = settings.tool_hook_enabled;
    let cfg = Some(settings.tool_hook_port);
    let running = state.running.load(Ordering::SeqCst);
    let config_changed = {
        let active = state.active_config.lock().await;
        active.as_ref() != cfg.as_ref()
    };

    if want && (!running || config_changed) {
        stop_server(&state).await;
        start_server(app, &settings).await;
    } else if !want && running {
        stop_server(&state).await;
    }

    // 自动自愈：外部工具里的 hook 脚本端口过期（needs_update）时，用当前设置重写。
    // 脚本是 RoxyDesktopPet 写入的（带 marker），重写安全；脚本路径不变，无需重启对应工具。
    for tool in crate::tool_hook_config::list_supported(&settings) {
        for item in tool.items {
            if item.status != "needs_update" {
                continue;
            }
            match crate::tool_hook_config::write(&tool.id, &item.id, &settings) {
                Ok(_) => log_info!("tool_hook: 已自动修复 {} 的 hook 配置（端口对齐 {}）", tool.id, settings.tool_hook_port),
                Err(error) => log_error!("tool_hook: 自动修复 {} 失败：{error}", tool.id),
            }
        }
    }
}

async fn start_server(app: &AppHandle, settings: &AppSettings) {
    let listener = match hook_server::bind(settings.tool_hook_port).await {
        Ok(listener) => listener,
        Err(error) => {
            log_error!("tool hook server bind failed: {error}");
            return;
        }
    };
    log_info!("tool hook server bound on 127.0.0.1:{}", settings.tool_hook_port);
    let app_clone = app.clone();
    let handler: hook_server::EventHandler = Arc::new(move |tool, body| {
        let app = app_clone.clone();
        tauri::async_runtime::spawn(async move {
            let _ = handle_event(&app, &tool, &body).await;
        });
    });
    let handle = hook_server::spawn(listener, handler);
    let state = app.state::<ToolHookState>();
    *state.handle.lock().await = Some(handle);
    *state.active_config.lock().await = Some(settings.tool_hook_port);
    state.running.store(true, Ordering::SeqCst);
}

async fn stop_server(state: &ToolHookState) {
    if let Some(handle) = state.handle.lock().await.take() {
        handle.abort();
    }
    *state.active_config.lock().await = None;
    state.running.store(false, Ordering::SeqCst);
}

async fn handle_event(app: &AppHandle, tool: &str, body: &str) -> Result<(), String> {
    log_info!("tool_hook handle_event: tool={tool}, body_len={}", body.len());
    let event: HookEvent = serde_json::from_str(body).map_err(|e| format!("事件 JSON 无效：{e}"))?;
    if event.tool != tool {
        return Err("工具名不匹配".into());
    }
    let db = app.state::<DbState>();
    let settings = {
        let conn = db.0.lock().await;
        db::get_settings(&conn, true).map_err(|e| e.to_string())?
    };
    if !settings.tool_hook_enabled {
        return Ok(());
    }
    if crate::tool_hook_config::find_tool(tool).is_none() {
        return Ok(());
    }

    let key = format!(
        "{}:{}",
        event.tool,
        if event.session_id.is_empty() {
            event.project.clone()
        } else {
            event.session_id.clone()
        }
    );
    let now = chrono::Utc::now().timestamp_millis();
    let interval_ms = settings.tool_hook_min_interval_minutes as i64 * 60_000;
    let state = app.state::<ToolHookState>();

    {
        let last = state.last_notify.lock().await;
        if let Some(prev) = last.get(&key) {
            if interval_ms > 0 && now - prev < interval_ms {
                return Ok(());
            }
        }
    }
    {
        let mut daily = state.daily.lock().await;
        let today = local_day(now);
        if daily.0 != today {
            daily.0 = today;
            daily.1 = 0;
        }
        if settings.tool_hook_daily_limit > 0 && daily.1 >= settings.tool_hook_daily_limit {
            return Ok(());
        }
    }

    let debounce = settings.tool_hook_debounce_seconds;
    if debounce > 0 {
        let generation = {
            let mut gens = state.debounce_gen.lock().await;
            let g = gens.entry(key.clone()).or_insert(0);
            *g += 1;
            *g
        };
        tokio::time::sleep(std::time::Duration::from_secs(debounce as u64)).await;
        let current = state
            .debounce_gen
            .lock()
            .await
            .get(&key)
            .copied()
            .unwrap_or(0);
        if current != generation {
            return Ok(());
        }
    }

    let (message, persisted) = if settings.tool_hook_mode == "ai" {
        match generate_ai_message(app, &settings, &event).await {
            Ok(message) => (message, true),
            Err(_) => (fixed_message(&settings, &event), false),
        }
    } else {
        (fixed_message(&settings, &event), false)
    };

    if !persisted {
        let conn = db.0.lock().await;
        db::insert_message(&conn, &message).map_err(|e| e.to_string())?;
    }
    {
        let mut daily = state.daily.lock().await;
        daily.1 += 1;
    }
    state.last_notify.lock().await.insert(key, now);

    let _ = app.emit("assistant-message", message.clone());
    if settings.tool_hook_voice_enabled {
        crate::voice_output::schedule(app.clone(), message);
    }
    Ok(())
}

fn fixed_message(settings: &AppSettings, event: &HookEvent) -> Message {
    let project = if event.project.is_empty() {
        project_from_cwd(&event.cwd)
    } else {
        event.project.clone()
    };
    let time = if event.timestamp > 0 {
        chrono::DateTime::from_timestamp_millis(event.timestamp)
            .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
            .unwrap_or_default()
    } else {
        chrono::Local::now().format("%H:%M").to_string()
    };
    let render = |text: &str| {
        text.replace("{tool}", &event.tool)
            .replace("{project}", &project)
            .replace("{event}", &event.event)
            .replace("{time}", &time)
    };
    // 按工具取文本配置；留空时回退到该工具的默认语句。
    let defaults = crate::tool_hook_config::find_tool(&event.tool);
    let configured = settings.tool_hook_tool_texts.get(&event.tool);
    let fixed_text = configured
        .map(|t| t.fixed_text.trim())
        .filter(|t| !t.is_empty())
        .or_else(|| defaults.map(|d| d.default_fixed_text))
        .unwrap_or_default();
    let voice_text = configured
        .map(|t| t.fixed_voice_text.trim())
        .filter(|t| !t.is_empty())
        .or_else(|| defaults.map(|d| d.default_fixed_voice_text))
        .unwrap_or_default();
    let content = render(fixed_text);
    let japanese_text = if voice_text.is_empty() {
        None
    } else {
        Some(render(voice_text))
    };
    Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: "assistant".into(),
        content,
        japanese_text,
        emotion: Some("calm".into()),
        trigger_type: "tool_hook".into(),
        created_at: chrono::Utc::now().timestamp_millis(),
    }
}

async fn generate_ai_message(
    app: &AppHandle,
    settings: &AppSettings,
    event: &HookEvent,
) -> Result<Message, String> {
    let project = if event.project.is_empty() {
        project_from_cwd(&event.cwd)
    } else {
        event.project.clone()
    };
    let mut prompt = format!(
        "这次是外部编程工具事件触发的主动提醒。来源信息：\n- 工具：{}\n- 项目：{}\n- 工作目录：{}\n- 事件类型：{}\n",
        event.tool, project, event.cwd, event.event
    );
    if settings.tool_hook_include_last_message && !event.last_assistant_message.is_empty() {
        prompt.push_str(&format!(
            "- 最后一条助手消息摘要：{}\n",
            truncate(&event.last_assistant_message, 300)
        ));
    }
    prompt.push_str("请以角色身份、用 1～2 句自然的话提醒用户该工具的任务已完成。不要虚构载荷中不存在的信息。");
    crate::commands::generate_scheduled_message(app, "tool_hook", &prompt).await
}

fn project_from_cwd(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_default()
}

fn truncate(text: &str, max: usize) -> String {
    let count = text.chars().count();
    let mut out: String = text.chars().take(max).collect();
    if count > max {
        out.push('…');
    }
    out
}

fn local_day(now: i64) -> String {
    chrono::DateTime::from_timestamp_millis(now)
        .unwrap_or_default()
        .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
        .format("%Y-%m-%d")
        .to_string()
}

/// 供测试命令直接走一遍管线，模拟一次真实 hook 事件。
pub async fn test_event(app: &AppHandle, tool: &str) -> Result<(), String> {
    let db = app.state::<DbState>();
    let settings = {
        let conn = db.0.lock().await;
        db::get_settings(&conn, true).map_err(|e| e.to_string())?
    };
    if !settings.tool_hook_enabled {
        return Err("编程联动提醒未启用，请先打开总开关".into());
    }
    let body = serde_json::json!({
        "tool": tool,
        "event": "session.idle",
        "session_id": format!("test-{}", chrono::Utc::now().timestamp_millis()),
        "project": "测试项目",
        "cwd": "",
        "timestamp": chrono::Utc::now().timestamp_millis(),
    });
    handle_event(app, tool, &body.to_string()).await
}
