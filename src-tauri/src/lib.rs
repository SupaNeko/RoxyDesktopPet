mod asr;
mod audio;
mod commands;
mod db;
mod global_input;
mod gpt_sovits;
mod hook_server;
mod memory;
mod observer;
mod pet_interaction;
mod qdrant_runtime;
mod scheduler;
mod tool_hook;
mod tool_hook_config;
mod voice_output;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
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

fn pet_window_position_path() -> PathBuf {
    data_dir().join("pet-window-position.json")
}

fn load_pet_window_position(
    window: &tauri::WebviewWindow,
) -> Option<tauri::PhysicalPosition<i32>> {
    let saved = std::fs::read_to_string(pet_window_position_path()).ok()?;
    let position: PetWindowPosition = serde_json::from_str(&saved).ok()?;
    if !position.anchored {
        return Some(tauri::PhysicalPosition::new(position.x, position.y));
    }
    let size = window.outer_size().ok()?;
    Some(tauri::PhysicalPosition::new(
        position.x - size.width as i32 / 2,
        position.y - size.height as i32,
    ))
}

fn save_pet_window_position(
    window: &tauri::WebviewWindow,
    position: tauri::PhysicalPosition<i32>,
) {
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
    let db = db::open(&data_dir().join("chatpet.db")).expect("failed to open ChatPet database");
    let voice_enabled = db::get_settings(&db, false)
        .ok()
        .is_some_and(|settings| settings.voice_output_mode == "gpt_sovits");
    let saved_qdrant_url = db::get_settings(&db, false)
        .ok()
        .map(|settings| settings.qdrant_url)
        .filter(|url| !url.is_empty());
    let memory_env = memory_env_config();
    let qdrant_url = saved_qdrant_url.unwrap_or_else(|| memory_env.qdrant_url.clone());

    tauri::Builder::default()
        // Command state must exist before setup because configured webviews can load
        // and invoke commands while the setup callback is still running.
        .manage(db::DbState(Mutex::new(db)))
        .manage(model_config())
        .manage(asr_env_config())
        .manage(memory_env)
        .manage(qdrant_runtime::QdrantRuntime::default())
        .manage(gpt_sovits::GptSoVitsState::default())
        .manage(audio::VoiceState::default())
        .manage(global_input::GlobalInputState::default())
        .manage(pet_interaction::PetInteractionState::default())
        .manage(ConversationState(Mutex::new(())))
        .manage(voice_output::VoiceOutputState::new(&data_dir()))
        .manage(tool_hook::ToolHookState::default())
        .setup(move |app| {
            let runtime_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let runtime = runtime_app.state::<qdrant_runtime::QdrantRuntime>();
                let _ = qdrant_runtime::ensure(runtime.inner(), &qdrant_url).await;
            });
            if voice_enabled {
                let voice_app = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let state = voice_app.state::<gpt_sovits::GptSoVitsState>();
                    if let Err(error) = gpt_sovits::ensure(state.inner()).await {
                        eprintln!("GPT-SoVITS startup failed: {error}");
                    }
                });
            }
            global_input::start_listener(app.handle().clone());
            scheduler::start(app.handle().clone());

            let hook_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tool_hook::reconcile_server(&hook_app).await;
            });

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
                if let Some(position) = load_pet_window_position(&pet_window) {
                    let _ = pet_window.set_position(position);
                }
                let position_window = pet_window.clone();
                pet_window.on_window_event(move |event| {
                    if let WindowEvent::Moved(position) = event {
                        save_pet_window_position(&position_window, *position);
                    }
                });
                let _ = pet_window.show();
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
                            let runtime = handle.state::<qdrant_runtime::QdrantRuntime>();
                            qdrant_runtime::shutdown(runtime.inner()).await;
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
            pet_interaction::set_mouse_passthrough,
            pet_interaction::get_mouse_passthrough
        ])
        .run(tauri::generate_context!())
        .expect("error while running ChatPet");
}
