mod asr;
mod audio;
mod commands;
mod db;
mod memory;
mod observer;
mod qdrant_runtime;
mod scheduler;
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
        std::env::var(key)
            .ok()
            .or_else(|| file.get(key).cloned())
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
    let value = |key: &str| {
        std::env::var(key)
            .ok()
            .or_else(|| file.get(key).cloned())
            .unwrap_or_default()
    };
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
    let value = |key: &str, default: &str| {
        std::env::var(key)
            .ok()
            .or_else(|| file.get(key).cloned())
            .unwrap_or_else(|| default.into())
    };
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

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let db = db::open(&data_dir().join("chatpet.db"))?;
            let saved_qdrant_url = db::get_settings(&db, false)
                .ok()
                .map(|s| s.qdrant_url)
                .filter(|url| !url.is_empty());
            app.manage(db::DbState(Mutex::new(db)));
            app.manage(model_config());
            app.manage(asr_env_config());
            let memory_env = memory_env_config();
            let qdrant_url = saved_qdrant_url.unwrap_or_else(|| memory_env.qdrant_url.clone());
            app.manage(memory_env);
            app.manage(qdrant_runtime::QdrantRuntime::default());
            let runtime_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let runtime = runtime_app.state::<qdrant_runtime::QdrantRuntime>();
                let _ = qdrant_runtime::ensure(runtime.inner(), &qdrant_url).await;
            });
            app.manage(audio::VoiceState::default());
            app.manage(ConversationState(Mutex::new(())));
            app.manage(voice_output::VoiceOutputState::new(&data_dir()));
            scheduler::start(app.handle().clone());

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

            let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
            let toggle = MenuItem::with_id(app, "toggle", "显示/隐藏桌宠", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings, &toggle, &quit])?;
            TrayIconBuilder::new()
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
                    "quit" => {
                        let handle = app.clone();
                        tauri::async_runtime::spawn(async move {
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
            commands::list_microphone_devices,
            commands::list_memories,
            commands::list_todos,
            commands::open_app_window,
            commands::scan_vits_models,
            commands::test_voice_output
        ])
        .run(tauri::generate_context!())
        .expect("error while running ChatPet");
}
