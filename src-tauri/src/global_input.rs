use serde::Serialize;
use std::{collections::HashSet, sync::Mutex as StdMutex, thread, time::Duration};
use tauri::{AppHandle, Emitter, Manager};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

#[derive(Default)]
struct InputInner {
    binding: HashSet<u16>,
    captured: HashSet<u16>,
    capturing: bool,
    triggered: bool,
}
#[derive(Default)]
pub struct GlobalInputState(StdMutex<InputInner>);
#[derive(Clone, Serialize)]
pub struct CapturedBinding {
    pub tokens: Vec<String>,
    pub label: String,
}

fn vk_label(vk: u16) -> String {
    match vk {
        0x01 => "鼠标左键".into(),
        0x02 => "鼠标右键".into(),
        0x04 => "鼠标中键".into(),
        0x05 => "鼠标侧键 1".into(),
        0x06 => "鼠标侧键 2".into(),
        0x08 => "Backspace".into(),
        0x09 => "Tab".into(),
        0x0D => "Enter".into(),
        0x1B => "Esc".into(),
        0x20 => "Space".into(),
        0x21 => "PageUp".into(),
        0x22 => "PageDown".into(),
        0x23 => "End".into(),
        0x24 => "Home".into(),
        0x25 => "←".into(),
        0x26 => "↑".into(),
        0x27 => "→".into(),
        0x28 => "↓".into(),
        0x2D => "Insert".into(),
        0x2E => "Delete".into(),
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk as u32).unwrap_or('?').to_string(),
        0x60..=0x69 => format!("Num{}", vk - 0x60),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0xA0 | 0xA1 => "Shift".into(),
        0xA2 | 0xA3 => "Ctrl".into(),
        0xA4 | 0xA5 => "Alt".into(),
        0x5B | 0x5C => "Win".into(),
        _ => format!("VK-{vk}"),
    }
}
fn binding_payload(binding: &HashSet<u16>) -> CapturedBinding {
    let mut codes: Vec<_> = binding.iter().copied().collect();
    codes.sort_unstable();
    CapturedBinding {
        tokens: codes.iter().map(|vk| format!("VK:{vk}")).collect(),
        label: codes
            .iter()
            .map(|vk| vk_label(*vk))
            .collect::<Vec<_>>()
            .join(" + "),
    }
}
fn legacy_token(token: &str) -> Option<u16> {
    if let Some(value) = token.strip_prefix("VK:") {
        return value.parse().ok();
    }
    match token {
        "K:F8" => Some(0x77),
        "M:Left" => Some(0x01),
        "M:Right" => Some(0x02),
        "M:Middle" => Some(0x04),
        "M:Unknown(1)" => Some(0x05),
        "M:Unknown(2)" => Some(0x06),
        _ => None,
    }
}
pub fn set_binding(
    state: &GlobalInputState,
    tokens: Vec<String>,
) -> Result<CapturedBinding, String> {
    let binding: HashSet<_> = tokens
        .iter()
        .filter_map(|token| legacy_token(token))
        .collect();
    let payload = binding_payload(&binding);
    let mut inner = state.0.lock().map_err(|_| "全局输入状态不可用")?;
    inner.binding = binding;
    inner.capturing = false;
    inner.triggered = false;
    Ok(payload)
}
pub fn begin_capture(state: &GlobalInputState) -> Result<(), String> {
    let mut inner = state.0.lock().map_err(|_| "全局输入状态不可用")?;
    inner.capturing = true;
    inner.captured.clear();
    inner.triggered = false;
    Ok(())
}
pub fn cancel_capture(state: &GlobalInputState) {
    if let Ok(mut inner) = state.0.lock() {
        inner.capturing = false;
        inner.captured.clear();
    }
}
fn current_pressed() -> HashSet<u16> {
    (1u16..=254)
        .filter(|vk| !matches!(vk, 0x10 | 0x11 | 0x12))
        .filter(|vk| unsafe { GetAsyncKeyState(*vk as i32) } < 0)
        .collect()
}
pub fn start_listener(app: AppHandle) {
    thread::Builder::new()
        .name("roxy-global-input".into())
        .spawn(move || {
            let mut previous = current_pressed();
            loop {
                thread::sleep(Duration::from_millis(8));
                let pressed = current_pressed();
                let downs: Vec<_> = pressed.difference(&previous).copied().collect();
                let ups: Vec<_> = previous.difference(&pressed).copied().collect();
                previous = pressed.clone();
                if downs.contains(&0x02)
                    && app
                        .state::<crate::pet_interaction::PetInteractionState>()
                        .enabled()
                {
                    let _ = crate::pet_interaction::show_context_menu(&app);
                }
                let state = app.state::<GlobalInputState>();
                let mut start_recording = false;
                let mut stop_recording = false;
                let mut preview = None;
                let mut completed = None;
                if let Ok(mut inner) = state.0.lock() {
                    if inner.capturing {
                        for vk in downs {
                            inner.captured.insert(vk);
                        }
                        if !inner.captured.is_empty() && !pressed.is_empty() {
                            preview = Some(binding_payload(&inner.captured));
                        }
                        if !inner.captured.is_empty() && pressed.is_empty() {
                            inner.binding = std::mem::take(&mut inner.captured);
                            inner.capturing = false;
                            completed = Some(binding_payload(&inner.binding));
                        }
                    } else {
                        if !inner.triggered
                            && !inner.binding.is_empty()
                            && inner.binding.is_subset(&pressed)
                        {
                            inner.triggered = true;
                            start_recording = true;
                        }
                        if inner.triggered && ups.iter().any(|vk| inner.binding.contains(vk)) {
                            inner.triggered = false;
                            stop_recording = true;
                        }
                    }
                }
                if let Some(payload) = preview {
                    let _ = app.emit("shortcut-capture-preview", payload);
                }
                if let Some(payload) = completed {
                    let _ = app.emit("shortcut-captured", payload);
                }
                if start_recording {
                    let _ = app.emit(
                        "voice-status",
                        crate::audio::VoiceStatus {
                            state: "speaking".into(),
                            detail: Some("检测到按键，正在打开麦克风…".into()),
                            duration_ms: None,
                        },
                    );
                    let task_app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(error) =
                            crate::commands::begin_push_to_talk(task_app.clone()).await
                        {
                            let _ = task_app.emit(
                                "voice-status",
                                crate::audio::VoiceStatus {
                                    state: "error".into(),
                                    detail: Some(error),
                                    duration_ms: None,
                                },
                            );
                        }
                    });
                }
                if stop_recording {
                    let task_app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(error) =
                            crate::commands::end_push_to_talk(task_app.clone()).await
                        {
                            let _ = task_app.emit(
                                "voice-status",
                                crate::audio::VoiceStatus {
                                    state: "error".into(),
                                    detail: Some(error),
                                    duration_ms: None,
                                },
                            );
                        }
                    });
                }
            }
        })
        .expect("failed to start global input listener");
}
