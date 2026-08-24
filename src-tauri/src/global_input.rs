use serde::Serialize;
use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{channel, Receiver, Sender},
        Mutex as StdMutex,
        OnceLock,
    },
    thread,
};
use tauri::{AppHandle, Emitter, Manager};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, SetWindowsHookExW, KBDLLHOOKSTRUCT, MSLLHOOKSTRUCT, MSG,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WM_XBUTTONDOWN, WM_XBUTTONUP, WM_MOUSEMOVE,
};

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

struct InputEvent {
    vk: u16,
    down: bool,
    mouse_position: Option<(i32, i32)>,
}

/// 钩子回调与消费者线程之间的事件通道。钩子回调只做「投递」这一件最小的事，
/// 真正的状态机在消费者线程里跑，绝不让钩子回调阻塞（系统会移除超时钩子）。
static HOOK_SENDER: OnceLock<Sender<InputEvent>> = OnceLock::new();
static HOOKS_INSTALLED: AtomicBool = AtomicBool::new(false);

fn send_event(vk: u16, down: bool) {
    // 过滤左右不分的通用修饰键，与绑定/捕获逻辑保持一致
    if matches!(vk, 0x10 | 0x11 | 0x12) {
        return;
    }
    if let Some(sender) = HOOK_SENDER.get() {
        let _ = sender.send(InputEvent { vk, down, mouse_position: None });
    }
}

fn send_mouse_move(x: i32, y: i32) {
    if let Some(sender) = HOOK_SENDER.get() {
        let _ = sender.send(InputEvent { vk: 0, down: false, mouse_position: Some((x, y)) });
    }
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

/// 按需安装全局低级键盘/鼠标钩子。进程内只会真正安装一次；
/// 只有在用户启用「按键说话」或「鼠标穿透」时才调用，避免无谓的全局钩子常驻。
pub fn ensure_hooks() {
    if HOOKS_INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    log_info!("安装全局低级键盘/鼠标钩子");
    start_hook_thread();
}

/// 钩子回调在独立线程的消息循环上运行。回调只负责把事件投递到 channel，
/// 不执行任何可能阻塞的业务逻辑，也不直接触碰 Tauri 状态。
fn start_hook_thread() {
    thread::Builder::new()
        .name("roxy-input-hooks".into())
        .spawn(|| unsafe {
            let keyboard = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_hook),
                std::ptr::null_mut(),
                0,
            );
            let mouse = SetWindowsHookExW(
                WH_MOUSE_LL,
                Some(mouse_hook),
                std::ptr::null_mut(),
                0,
            );
            if keyboard.is_null() || mouse.is_null() {
                log_error!(
                    "全局输入钩子安装失败（keyboard={}, mouse={}），按键说话可能不可用",
                    keyboard.is_null(),
                    mouse.is_null()
                );
            } else {
                log_info!("全局低级键盘/鼠标钩子安装成功");
            }
            let mut msg: MSG = std::mem::zeroed();
            loop {
                if GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) <= 0 {
                    break;
                }
            }
            log_warn!("全局输入钩子消息循环退出");
        })
        .expect("failed to start global input hooks");
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lparam as *const KBDLLHOOKSTRUCT);
        let msg = wparam as u32;
        match msg {
            WM_KEYDOWN | WM_SYSKEYDOWN => send_event(info.vkCode as u16, true),
            WM_KEYUP | WM_SYSKEYUP => send_event(info.vkCode as u16, false),
            _ => {}
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let msg = wparam as u32;
        let info = &*(lparam as *const MSLLHOOKSTRUCT);
        if msg == WM_MOUSEMOVE {
            send_mouse_move(info.pt.x, info.pt.y);
            return CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam);
        }
        let event = match msg {
            WM_LBUTTONDOWN => Some((0x01u16, true)),
            WM_LBUTTONUP => Some((0x01u16, false)),
            WM_RBUTTONDOWN => Some((0x02u16, true)),
            WM_RBUTTONUP => Some((0x02u16, false)),
            WM_MBUTTONDOWN => Some((0x04u16, true)),
            WM_MBUTTONUP => Some((0x04u16, false)),
            WM_XBUTTONDOWN | WM_XBUTTONUP => {
                let info = &*(lparam as *const MSLLHOOKSTRUCT);
                let xbutton = (info.mouseData >> 16) & 0xFFFF;
                Some((if xbutton == 1 { 0x05u16 } else { 0x06u16 }, msg == WM_XBUTTONDOWN))
            }
            _ => None,
        };
        if let Some((vk, down)) = event {
            send_event(vk, down);
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

/// 启动消费者线程：阻塞等待钩子投递的事件，事件驱动地跑状态机。
/// 与旧的 8ms 轮询不同，这里没有事件时线程完全休眠，不再空转。
pub fn start_listener(app: AppHandle) {
    log_info!("global_input listener starting");
    let (tx, rx) = channel::<InputEvent>();
    let _ = HOOK_SENDER.set(tx);
    thread::Builder::new()
        .name("roxy-global-input".into())
        .spawn(move || consume_events(app, rx))
        .expect("failed to start global input listener");
}

fn consume_events(app: AppHandle, rx: Receiver<InputEvent>) {
    let mut pressed: HashSet<u16> = HashSet::new();
    while let Ok(event) = rx.recv() {
        if let Some((x, y)) = event.mouse_position {
            crate::native_hit_test::update_mouse_position(&app, x, y);
            continue;
        }
        if event.down {
            pressed.insert(event.vk);
        } else {
            pressed.remove(&event.vk);
        }

        // 鼠标穿透开启时，桌宠窗口收不到自身右键，需要全局右键弹原生菜单。
        // 弹窗/菜单必须走主线程，不能在 raw 线程直接调用。
        if event.down
            && event.vk == 0x02
            && app
                .state::<crate::pet_interaction::PetInteractionState>()
                .enabled()
        {
            let menu_app = app.clone();
            let _ = app.run_on_main_thread(move || {
                let _ = crate::pet_interaction::show_context_menu(&menu_app);
            });
        }

        let state = app.state::<GlobalInputState>();
        let mut preview: Option<CapturedBinding> = None;
        let mut completed: Option<CapturedBinding> = None;
        let mut start_recording = false;
        let mut stop_recording = false;
        if let Ok(mut inner) = state.0.lock() {
            if inner.capturing {
                if event.down {
                    inner.captured.insert(event.vk);
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
                if event.down
                    && !inner.triggered
                    && !inner.binding.is_empty()
                    && inner.binding.is_subset(&pressed)
                {
                    inner.triggered = true;
                    start_recording = true;
                }
                if !event.down && inner.triggered && inner.binding.contains(&event.vk) {
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
                if let Err(error) = crate::commands::begin_push_to_talk(task_app.clone()).await {
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
                if let Err(error) = crate::commands::end_push_to_talk(task_app.clone()).await {
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
    log_warn!("全局输入事件循环退出");
}
