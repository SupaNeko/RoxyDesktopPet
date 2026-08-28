use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    menu::{Menu, MenuItem},
    AppHandle, Emitter, Manager,
};

#[derive(Default)]
pub struct PetInteractionState {
    passthrough: AtomicBool,
}
impl PetInteractionState {
    pub fn enabled(&self) -> bool {
        self.passthrough.load(Ordering::Relaxed)
    }
}

pub fn set(app: &AppHandle, enabled: bool) -> Result<(), String> {
    app.state::<PetInteractionState>()
        .passthrough
        .store(enabled, Ordering::Relaxed);
    crate::native_hit_test::set_passthrough(app, enabled);
    if enabled {
        // 穿透开启时桌宠窗口收不到右键，需要全局鼠标钩子来弹菜单
        crate::global_input::ensure_hooks();
    }
    let window = app
        .get_webview_window("pet")
        .ok_or_else(|| "找不到桌宠窗口".to_string())?;
    window
        .set_ignore_cursor_events(enabled)
        .map_err(|error| error.to_string())?;
    let _ = app.emit("mouse-passthrough-changed", enabled);
    Ok(())
}
pub fn toggle(app: &AppHandle) -> Result<bool, String> {
    let enabled = !app.state::<PetInteractionState>().enabled();
    set(app, enabled)?;
    Ok(enabled)
}
pub fn show_context_menu(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("pet")
        .ok_or_else(|| "找不到桌宠窗口".to_string())?;
    let passthrough = MenuItem::with_id(app, "pet_passthrough", "关闭鼠标穿透", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let settings = MenuItem::with_id(app, "pet_settings", "打开设置", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let todos = MenuItem::with_id(app, "pet_todos", "查看待办", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let history = MenuItem::with_id(app, "pet_history", "历史会话", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let menu = Menu::with_items(app, &[&passthrough, &settings, &todos, &history])
        .map_err(|e| e.to_string())?;
    window.popup_menu(&menu).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn set_mouse_passthrough(app: AppHandle, enabled: bool) -> Result<(), String> {
    set(&app, enabled)
}
#[tauri::command]
pub fn get_mouse_passthrough(app: AppHandle) -> bool {
    app.state::<PetInteractionState>().enabled()
}
