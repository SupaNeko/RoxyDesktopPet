#[macro_use]
mod logger;

mod agent;
mod asr;
mod audio;
mod commands;
mod db;
mod fullscreen_watch;
mod global_input;
mod gpt_sovits;
mod hook_server;
mod media_control;
mod mcp;
mod memory;
mod memory_store;
mod native_hit_test;
mod observer;
mod pet_interaction;
mod scheduler;
mod system_monitor;
mod tool_hook;
mod tool_hook_config;
mod voice_output;

use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{Mutex as StdMutex, OnceLock},
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};
use tokio::sync::Mutex;

pub struct ConversationState(pub Mutex<()>);

pub struct ModelConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

pub struct AsrEnvConfig {
    pub app_id: String,
    pub api_key: String,
    pub api_secret: String,
}

pub struct MemoryEnvConfig {
    pub qdrant_url: String,
    pub embedding_base_url: String,
    pub embedding_model: String,
    pub embedding_api_key: String,
    pub embedding_dimension: u32,
}

fn read_dotenv(path: &Path) -> HashMap<String, String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|content| {
            content
                .lines()
                .filter_map(|line| {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        return None;
                    }
                    let (key, value) = line.split_once('=')?;
                    Some((
                        key.trim().to_string(),
                        value.trim().trim_matches(['"', '\'']).to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn model_config() -> ModelConfig {
    let env_path = if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(".env")
    } else {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".env")
    };
    let file = read_dotenv(&env_path);
    let value = |key: &str, default: &str| {
        file.get(key)
            .cloned()
            .unwrap_or_else(|| default.to_string())
    };
    ModelConfig {
        api_key: value("DEEPSEEK_API_KEY", ""),
        base_url: value("DEEPSEEK_BASE_URL", "https://api.deepseek.com")
            .trim_end_matches('/')
            .to_string(),
        model: value("DEEPSEEK_MODEL", "deepseek-v4-flash"),
    }
}

fn asr_env_config() -> AsrEnvConfig {
    let env_path = if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(".env")
    } else {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".env")
    };
    let file = read_dotenv(&env_path);
    let value = |key: &str| file.get(key).cloned().unwrap_or_default();
    AsrEnvConfig {
        app_id: value("XFYUN_ASR_APP_ID"),
        api_key: value("XFYUN_ASR_API_KEY"),
        api_secret: value("XFYUN_ASR_API_SECRET"),
    }
}

fn memory_env_config() -> MemoryEnvConfig {
    let env_path = if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(".env")
    } else {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".env")
    };
    let file = read_dotenv(&env_path);
    let value = |key: &str, default: &str| file.get(key).cloned().unwrap_or_else(|| default.into());
    MemoryEnvConfig {
        qdrant_url: value("QDRANT_URL", "http://127.0.0.1:6333"),
        embedding_base_url: value("EMBEDDING_BASE_URL", ""),
        embedding_model: value("EMBEDDING_MODEL", ""),
        embedding_api_key: value("EMBEDDING_API_KEY", ""),
        embedding_dimension: value("EMBEDDING_DIMENSION", "0").parse().unwrap_or(0),
    }
}

pub fn data_dir() -> PathBuf {
    if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("data")
    } else {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("data")
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
struct PetWindowPosition {
    x: i32,
    y: i32,
    #[serde(default)]
    anchored: bool,
}

static PROGRAMMATIC_PET_POSITIONS: OnceLock<StdMutex<VecDeque<tauri::PhysicalPosition<i32>>>> =
    OnceLock::new();

fn programmatic_pet_positions() -> &'static StdMutex<VecDeque<tauri::PhysicalPosition<i32>>> {
    PROGRAMMATIC_PET_POSITIONS.get_or_init(|| StdMutex::new(VecDeque::new()))
}

pub(crate) fn expect_programmatic_pet_move(position: tauri::PhysicalPosition<i32>) {
    let Ok(mut pending) = programmatic_pet_positions().lock() else {
        return;
    };
    if pending.len() >= 8 {
        pending.pop_front();
    }
    pending.push_back(position);
}

fn consume_programmatic_pet_move(position: tauri::PhysicalPosition<i32>) -> bool {
    let Ok(mut pending) = programmatic_pet_positions().lock() else {
        return false;
    };
    let Some(index) = pending.iter().position(|expected| {
        (position.x - expected.x).abs() <= 2 && (position.y - expected.y).abs() <= 2
    }) else {
        // A real drag makes any older expected positions obsolete.
        pending.clear();
        return false;
    };
    pending.remove(index);
    true
}

fn pet_window_position_path() -> PathBuf {
    data_dir().join("pet-window-position.json")
}

fn load_pet_window_position(window: &tauri::WebviewWindow) -> Option<tauri::PhysicalPosition<i32>> {
    let saved = std::fs::read_to_string(pet_window_position_path()).ok()?;
    let position: PetWindowPosition = serde_json::from_str(&saved).ok()?;
    if position.anchored {
        let size = window.outer_size().ok()?;
        return Some(tauri::PhysicalPosition::new(
            position.x - size.width as i32 / 2,
            position.y - size.height as i32,
        ));
    }

    // Migrate the short-lived top-left format. All new positions use a
    // bottom-center anchor so changing the bubble height cannot move the pet.
    let migrated = tauri::PhysicalPosition::new(position.x, position.y);
    save_pet_window_position(window, migrated);
    Some(migrated)
}

fn save_pet_window_position(window: &tauri::WebviewWindow, position: tauri::PhysicalPosition<i32>) {
    let Ok(size) = window.outer_size() else {
        return;
    };
    let path = pet_window_position_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let saved = PetWindowPosition {
        x: position.x + size.width as i32 / 2,
        y: position.y + size.height as i32,
        anchored: true,
    };
    if let Ok(json) = serde_json::to_string(&saved) {
        let _ = std::fs::write(path, json);
    }
}
pub fn run() {
    crate::logger::init();
    let db = db::open(&data_dir().join("chatpet.db")).expect("failed to open ChatPet database");
    log_info!("数据库已打开：{}", data_dir().join("chatpet.db").display());
    let voice_enabled = db::get_settings(&db, false)
        .ok()
        .is_some_and(|settings| settings.voice_output_mode == "gpt_sovits");
    let memory_env = memory_env_config();
    let mcp_servers = db::list_mcp_servers(&db).unwrap_or_default();

    tauri::Builder::default()
        // Command state must exist before setup because configured webviews can load
        // and invoke commands while the setup callback is still running.
        .manage(db::DbState(Mutex::new(db)))
        .manage(model_config())
        .manage(asr_env_config())
        .manage(memory_env)
        .manage(gpt_sovits::GptSoVitsState::default())
        .manage(audio::VoiceState::default())
        .manage(global_input::GlobalInputState::default())
        .manage(pet_interaction::PetInteractionState::default())
        .manage(native_hit_test::PetHitTestState::default())
        .manage(ConversationState(Mutex::new(())))
        .manage(voice_output::VoiceOutputState::new(&data_dir()))
        .manage(tool_hook::ToolHookState::default())
        .manage(mcp::McpState::default())
        .setup(move |app| {
            if voice_enabled {
                let voice_app = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let state = voice_app.state::<gpt_sovits::GptSoVitsState>();
                    if let Err(error) = gpt_sovits::ensure(state.inner()).await {
                        log_error!("GPT-SoVITS startup failed: {error}");
                    }
                });
            }
            global_input::start_listener(app.handle().clone());
            // Pixel click-through is driven by real mouse events, not a polling loop.
            global_input::ensure_hooks();
            scheduler::start(app.handle().clone());
            fullscreen_watch::start(app.handle().clone());

            // 恢复上次的语音输入方式（持续监听 / 按住说话快捷键）。
            {
                let voice_app = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let settings = {
                        let db_state = voice_app.state::<db::DbState>();
                        let conn = db_state.0.lock().await;
                        match db::get_settings(&conn, true) {
                            Ok(s) => s,
                            Err(e) => {
                                log_error!("恢复语音输入设置失败: {e}");
                                return;
                            }
                        }
                    };
                    match settings.voice_input_mode.as_str() {
                        "continuous" => {
                            let asr_env = voice_app.state::<AsrEnvConfig>();
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
                                log_warn!("持续语音输入未恢复：讯飞 ASR 凭据不完整");
                                return;
                            }
                            let voice = voice_app.state::<audio::VoiceState>();
                            if let Err(error) = audio::start(
                                voice_app.clone(),
                                voice.inner(),
                                settings.microphone_device_name.as_deref(),
                                credentials,
                            )
                            .await
                            {
                                log_error!("启动时恢复持续语音监听失败: {error}");
                            } else {
                                log_info!("已恢复持续语音监听");
                            }
                        }
                        "push_to_talk" => {
                            if let Ok(binding) = serde_json::from_str::<
                                crate::global_input::CapturedBinding,
                            >(&settings.push_to_talk_shortcut)
                            {
                                let input = voice_app.state::<global_input::GlobalInputState>();
                                if let Err(error) =
                                    global_input::set_binding(input.inner(), binding.tokens)
                                {
                                    log_error!("恢复按住说话快捷键失败: {error}");
                                } else {
                                    log_info!("已恢复按住说话快捷键");
                                }
                            }
                        }
                        _ => {}
                    }
                });
            }

            let hook_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tool_hook::reconcile_server(&hook_app).await;
            });

            // 启动时拉起已启用的 MCP 服务器（stdio 子进程 + 远程连接）。
            mcp::bootstrap(app.handle().clone(), mcp_servers.clone());

            if let Some(settings_window) = app.get_webview_window("settings") {
                let window_to_hide = settings_window.clone();
                settings_window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_to_hide.hide();
                    }
                });
            }
            if let Some(todos_window) = app.get_webview_window("todos") {
                let window_to_hide = todos_window.clone();
                todos_window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_to_hide.hide();
                    }
                });
            }

            if let Some(menu_window) = app.get_webview_window("pet-menu") {
                let window_to_hide = menu_window.clone();
                menu_window.on_window_event(move |event| {
                    if let WindowEvent::Focused(false) = event {
                        let _ = window_to_hide.hide();
                    }
                });
            }

            if let Some(pet_window) = app.get_webview_window("pet") {
                if let Err(error) = native_hit_test::install(
                    &pet_window,
                    app.state::<native_hit_test::PetHitTestState>()
                        .inner()
                        .clone(),
                ) {
                    log_error!("安装宠物原生命中测试失败: {error}");
                }
                if let Some(position) = load_pet_window_position(&pet_window) {
                    expect_programmatic_pet_move(position);
                    let _ = pet_window.set_position(position);
                }
                let position_window = pet_window.clone();
                pet_window.on_window_event(move |event| {
                    if let WindowEvent::Moved(position) = event {
                        if !consume_programmatic_pet_move(*position) {
                            save_pet_window_position(&position_window, *position);
                        }
                    }
                });
                let _ = pet_window.show();
                if let Err(error) = native_hit_test::install(
                    &pet_window,
                    app.state::<native_hit_test::PetHitTestState>()
                        .inner()
                        .clone(),
                ) {
                    log_error!("显示后安装宠物原生命中测试失败: {error}");
                }
                pet_window.on_menu_event(|window, event| match event.id.as_ref() {
                    "pet_passthrough" => {
                        let _ = pet_interaction::set(window.app_handle(), false);
                    }
                    "pet_settings" => {
                        if let Some(w) = window.app_handle().get_webview_window("settings") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "pet_todos" => {
                        if let Some(w) = window.app_handle().get_webview_window("todos") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    _ => {}
                });
            }
            let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
            let toggle = MenuItem::with_id(app, "toggle", "显示/隐藏桌宠", true, None::<&str>)?;
            let passthrough =
                MenuItem::with_id(app, "passthrough", "切换鼠标穿透", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings, &toggle, &passthrough, &quit])?;
            let tray_icon = app.default_window_icon().cloned();
            let mut tray_builder = TrayIconBuilder::new();
            if let Some(icon) = tray_icon {
                tray_builder = tray_builder.icon(icon);
            }
            tray_builder
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => {
                        if let Some(w) = app.get_webview_window("settings") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "toggle" => {
                        if let Some(w) = app.get_webview_window("pet") {
                            if w.is_visible().unwrap_or(false) {
                                let _ = w.hide();
                            } else {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                    }
                    "passthrough" => {
                        let _ = pet_interaction::toggle(app);
                    }
                    "quit" => {
                        let handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let voice = handle.state::<gpt_sovits::GptSoVitsState>();
                            gpt_sovits::shutdown(voice.inner()).await;
                            let mcp_state = handle.state::<mcp::McpState>();
                            mcp::shutdown(mcp_state.inner()).await;
                            handle.exit(0);
                        });
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("pet") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::list_messages,
            commands::get_runtime_status,
            commands::send_text_message,
            commands::start_voice_listening,
            commands::stop_voice_listening,
            commands::begin_push_to_talk,
            commands::end_push_to_talk,
            commands::set_push_to_talk_shortcut,
            commands::begin_shortcut_capture,
            commands::cancel_shortcut_capture,
            commands::list_microphone_devices,
            commands::list_memories,
            commands::list_todos,
            commands::delete_todo,
            commands::open_app_window,
            commands::show_pet_menu,
            commands::toggle_proactive_enabled,
            commands::scan_vits_models,
            commands::test_voice_output,
            commands::start_gpt_sovits,
            commands::list_tool_hook_support,
            commands::write_tool_hook_config,
            commands::remove_tool_hook_config,
            commands::test_tool_hook,
            commands::get_taskbar_apps,
            commands::get_hardware_stats,
            commands::get_now_playing,
            commands::list_mcp_servers,
            commands::add_mcp_server,
            commands::update_mcp_server,
            commands::remove_mcp_server,
            commands::set_mcp_server_enabled,
            commands::list_mcp_server_tools,
            pet_interaction::set_mouse_passthrough,
            pet_interaction::get_mouse_passthrough,
            native_hit_test::update_pet_hit_test_layout
        ])
        .run(tauri::generate_context!())
        .expect("error while running ChatPet");
}
