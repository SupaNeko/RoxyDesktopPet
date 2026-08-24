use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock, RwLock},
};

use serde::Deserialize;
use tauri::{AppHandle, Manager};

#[derive(Clone, Default)]
pub struct PetHitTestState(pub Arc<RwLock<HitTestLayout>>);

#[derive(Clone, Default)]
struct HitTestLayout {
    passthrough: bool,
    interactive: Vec<Rect>,
    pet: Option<PetMask>,
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
        Self {
            x: self.x * scale,
            y: self.y * scale,
            width: self.width * scale,
            height: self.height * scale,
        }
    }
}

impl PetMask {
    fn contains(&self, x: f64, y: f64) -> bool {
        if !self.rect.contains(x, y) || self.rect.width <= 0.0 || self.rect.height <= 0.0 {
            return false;
        }
        let px = ((x - self.rect.x) * self.image_width as f64 / self.rect.width) as usize;
        let py = ((y - self.rect.y) * self.image_height as f64 / self.rect.height) as usize;
        self.rows
            .get(py)
            .is_some_and(|runs| runs.iter().any(|run| px >= run.start as usize && px < run.end as usize))
    }
}

pub fn update_layout(app: &AppHandle, request: LayoutRequest) -> Result<(), String> {
    let scale = request.scale_factor.max(0.1);
    let layout = HitTestLayout {
        passthrough: app.state::<PetHitTestState>().0.read().map_err(|_| "命中测试状态不可用")?.passthrough,
        interactive: request.interactive.iter().map(|rect| rect.scaled(scale)).collect(),
        pet: request.pet.map(|pet| PetMask {
            rect: pet.rect.scaled(scale),
            image_width: pet.image_width,
            image_height: pet.image_height,
            rows: pet.rows,
        }),
    };
    *app.state::<PetHitTestState>().0.write().map_err(|_| "命中测试状态不可用")? = layout;

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
    }
}

#[cfg(windows)]
struct WindowProcState { layout: Arc<RwLock<HitTestLayout>> }
#[cfg(windows)] static WINDOW_PROCS: OnceLock<Mutex<HashMap<isize, WindowProcState>>> = OnceLock::new();
#[cfg(windows)] const PET_SUBCLASS_ID: usize = 0x524F_5859;
#[cfg(windows)] fn window_procs() -> &'static Mutex<HashMap<isize, WindowProcState>> { WINDOW_PROCS.get_or_init(|| Mutex::new(HashMap::new())) }

#[cfg(windows)]
unsafe extern "system" fn pet_subclass_proc(hwnd: windows_sys::Win32::Foundation::HWND, message: u32, wparam: windows_sys::Win32::Foundation::WPARAM, lparam: windows_sys::Win32::Foundation::LPARAM, _: usize, _: usize) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::{Foundation::POINT, Graphics::Gdi::ScreenToClient, UI::{Shell::{DefSubclassProc, RemoveWindowSubclass}, WindowsAndMessaging::{HTCLIENT, HTTRANSPARENT, WM_NCDESTROY, WM_NCHITTEST}}};
    let layout = window_procs().lock().ok().and_then(|states| states.get(&(hwnd as isize)).map(|state| state.layout.clone()));
    if message == WM_NCHITTEST {
        if let Some(layout) = layout {
            let mut point = POINT { x: lparam as i16 as i32, y: (lparam >> 16) as i16 as i32 };
            if ScreenToClient(hwnd, &mut point) != 0 {
                if let Ok(layout) = layout.read() {
                    let x = point.x as f64; let y = point.y as f64;
                    let interactive = layout.interactive.iter().any(|rect| rect.contains(x, y));
                    let pet = layout.pet.as_ref().is_some_and(|pet| pet.contains(x, y));
                    if layout.passthrough || (!interactive && !pet) { return HTTRANSPARENT as isize; }
                    return HTCLIENT as isize;
                }
            }
        }
    }
    if message == WM_NCDESTROY { let _ = RemoveWindowSubclass(hwnd, Some(pet_subclass_proc), PET_SUBCLASS_ID); if let Ok(mut states) = window_procs().lock() { states.remove(&(hwnd as isize)); } }
    DefSubclassProc(hwnd, message, wparam, lparam)
}

#[cfg(windows)]
pub fn install(window: &tauri::WebviewWindow, state: PetHitTestState) -> Result<(), String> {
    use windows_sys::Win32::{Foundation::HWND, UI::Shell::SetWindowSubclass};
    let hwnd = window.hwnd().map_err(|error| error.to_string())?.0 as isize;
    window.run_on_main_thread(move || unsafe { let raw_hwnd = hwnd as HWND; if SetWindowSubclass(raw_hwnd, Some(pet_subclass_proc), PET_SUBCLASS_ID, 0) != 0 { if let Ok(mut states) = window_procs().lock() { states.insert(hwnd, WindowProcState { layout: state.0 }); } } }).map_err(|error| error.to_string())?;
    Ok(())
}
#[cfg(not(windows))]
pub fn install(_: &tauri::WebviewWindow, _: PetHitTestState) -> Result<(), String> { Ok(()) }
