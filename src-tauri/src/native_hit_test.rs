use std::sync::{Arc, RwLock};

use serde::Deserialize;
use tauri::{AppHandle, Manager};

#[derive(Clone, Default)]
pub struct PetHitTestState(Arc<RwLock<HitTestLayout>>);

#[derive(Clone, Default)]
struct HitTestLayout {
    passthrough: bool,
    cursor_ignored: bool,
    interactive: Vec<Rect>,
    pet: Option<PetMask>,
}

impl HitTestLayout {
    fn accepts_pointer(&self, x: f64, y: f64) -> bool {
        self.interactive.iter().any(|rect| rect.contains(x, y))
            || self.pet.as_ref().is_some_and(|pet| pet.contains(x, y))
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutRequest {
    scale_factor: f64,
    interactive: Vec<Rect>,
    pet: Option<PetMaskRequest>,
    content_height: f64,
}

#[derive(Clone, Deserialize)]
pub struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetMaskRequest {
    rect: Rect,
    image_width: u32,
    image_height: u32,
    rows: Vec<Vec<AlphaRun>>,
}

#[derive(Clone, Deserialize)]
pub struct AlphaRun {
    start: u32,
    end: u32,
}

#[derive(Clone)]
struct PetMask {
    rect: Rect,
    image_width: u32,
    image_height: u32,
    rows: Vec<Vec<AlphaRun>>,
}

impl Rect {
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }

    fn scaled(&self, scale: f64) -> Self {
        Self { x: self.x * scale, y: self.y * scale, width: self.width * scale, height: self.height * scale }
    }
}

impl PetMask {
    fn contains(&self, x: f64, y: f64) -> bool {
        if !self.rect.contains(x, y) || self.rect.width <= 0.0 || self.rect.height <= 0.0 { return false; }
        let px = ((x - self.rect.x) * self.image_width as f64 / self.rect.width) as usize;
        let py = ((y - self.rect.y) * self.image_height as f64 / self.rect.height) as usize;
        self.rows.get(py).is_some_and(|runs| runs.iter().any(|run| px >= run.start as usize && px < run.end as usize))
    }
}

pub fn update_layout(app: &AppHandle, request: LayoutRequest) -> Result<(), String> {
    let scale = request.scale_factor.max(0.1);
    let state = app.state::<PetHitTestState>();
    let previous = state.0.read().map_err(|_| "命中测试状态不可用")?.clone();
    let layout = HitTestLayout {
        passthrough: previous.passthrough,
        cursor_ignored: previous.cursor_ignored,
        interactive: request.interactive.iter().map(|rect| rect.scaled(scale)).collect(),
        pet: request.pet.map(|pet| PetMask { rect: pet.rect.scaled(scale), image_width: pet.image_width, image_height: pet.image_height, rows: pet.rows }),
    };
    *state.0.write().map_err(|_| "命中测试状态不可用")? = layout;

    let window = app.get_webview_window("pet").ok_or_else(|| "找不到桌宠窗口".to_string())?;
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let desired_height = (request.content_height * scale).ceil().clamp(230.0, 760.0) as u32;
    if (size.height as i64 - desired_height as i64).unsigned_abs() > 2 {
        window.set_size(tauri::PhysicalSize::new(size.width, desired_height)).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn update_pet_hit_test_layout(app: AppHandle, request: LayoutRequest) -> Result<(), String> {
    update_layout(&app, request)
}

pub fn set_passthrough(app: &AppHandle, enabled: bool) {
    if let Ok(mut layout) = app.state::<PetHitTestState>().0.write() {
        layout.passthrough = enabled;
        layout.cursor_ignored = enabled;
    }
}

/// Called only from real low-level mouse events. It performs no work while the
/// pointer remains in the same hit-test state, avoiding high-frequency IPC.
#[cfg(windows)]
pub fn update_mouse_position(app: &AppHandle, screen_x: i32, screen_y: i32) {
    use windows_sys::Win32::{Foundation::POINT, Graphics::Gdi::ScreenToClient};
    let Some(window) = app.get_webview_window("pet") else { return; };
    let Ok(hwnd) = window.hwnd() else { return; };
    let mut point = POINT { x: screen_x, y: screen_y };
    if unsafe { ScreenToClient(hwnd.0 as _, &mut point) } == 0 { return; }

    let desired_ignore = match app.state::<PetHitTestState>().0.read() {
        Ok(layout) => layout.passthrough || !layout.accepts_pointer(point.x as f64, point.y as f64),
        Err(_) => return,
    };
    let changed = match app.state::<PetHitTestState>().0.write() {
        Ok(mut layout) if layout.cursor_ignored != desired_ignore => { layout.cursor_ignored = desired_ignore; true }
        Ok(_) => false,
        Err(_) => return,
    };
    if changed {
        if let Err(error) = window.set_ignore_cursor_events(desired_ignore) {
            crate::logger::error(format!("切换桌宠鼠标穿透失败: {error}"));
        }
    }
}

#[cfg(not(windows))]
pub fn update_mouse_position(_: &AppHandle, _: i32, _: i32) {}

/// No window subclassing is needed: the global input hook drives hit testing.
pub fn install(_: &tauri::WebviewWindow, _: PetHitTestState) -> Result<(), String> { Ok(()) }