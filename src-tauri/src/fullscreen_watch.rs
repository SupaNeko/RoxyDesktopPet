//! 全屏检测：当设置关闭「在全屏应用上显示桌宠」时，pet 窗口所在显示器出现
//! 全屏前台应用就临时隐藏桌宠，全屏退出后恢复。只恢复由本模块隐藏的窗口，
//! 不会抢回用户手动隐藏的桌宠。多显示器：只关心 pet 窗口所在的那块屏。

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager};

static HIDDEN_BY_FULLSCREEN: AtomicBool = AtomicBool::new(false);

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            if let Err(error) = tick(&app).await {
                log_warn!("fullscreen watch tick failed: {error}");
            }
        }
    });
}

async fn tick(app: &AppHandle) -> Result<(), String> {
    let show_on_fullscreen = {
        let db = app.try_state::<crate::db::DbState>();
        match db {
            Some(state) => {
                let conn = state.0.lock().await;
                crate::db::get_settings(&conn, true)
                    .map(|s| s.pet_show_on_fullscreen)
                    .unwrap_or(true)
            }
            None => true,
        }
    };
    let Some(pet) = app.get_webview_window("pet") else {
        return Ok(());
    };
    let hidden_by_us = HIDDEN_BY_FULLSCREEN.load(Ordering::SeqCst);
    if show_on_fullscreen {
        if hidden_by_us {
            HIDDEN_BY_FULLSCREEN.store(false, Ordering::SeqCst);
            let _ = pet.show();
            log_info!("fullscreen watch: 设置已开启，恢复显示桌宠");
        }
        return Ok(());
    }
    let pet_hwnd = pet.hwnd().map_err(|e| e.to_string())?;
    let fullscreen_here = foreground_fullscreen_on_monitor(pet_hwnd.0);
    if fullscreen_here && !hidden_by_us {
        HIDDEN_BY_FULLSCREEN.store(true, Ordering::SeqCst);
        let _ = pet.hide();
        log_info!("fullscreen watch: 检测到全屏应用，隐藏桌宠");
    } else if !fullscreen_here && hidden_by_us {
        HIDDEN_BY_FULLSCREEN.store(false, Ordering::SeqCst);
        let _ = pet.show();
        log_info!("fullscreen watch: 全屏结束，恢复显示桌宠");
    }
    Ok(())
}

/// 前台窗口是否是与 `reference_hwnd` 同一块显示器上的全屏窗口。
#[cfg(windows)]
fn foreground_fullscreen_on_monitor(reference_hwnd: windows_sys::Win32::Foundation::HWND) -> bool {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId,
    };

    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.is_null() {
            return false;
        }
        // 排除本进程自己的窗口。
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(foreground, &mut pid);
        if pid == std::process::id() {
            return false;
        }
        // 排除桌面与任务栏等 shell 窗口。
        let mut class = [0u16; 64];
        let len = GetClassNameW(foreground, class.as_mut_ptr(), class.len() as i32);
        if len > 0 {
            let name = String::from_utf16_lossy(&class[..len as usize]);
            if matches!(
                name.as_str(),
                "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
            ) {
                return false;
            }
        }
        let fg_monitor = MonitorFromWindow(foreground, MONITOR_DEFAULTTONEAREST);
        let pet_monitor = MonitorFromWindow(reference_hwnd, MONITOR_DEFAULTTONEAREST);
        if fg_monitor.is_null() || fg_monitor != pet_monitor {
            return false;
        }
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(fg_monitor, &mut info) == 0 {
            return false;
        }
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(foreground, &mut rect) == 0 {
            return false;
        }
        let monitor = info.rcMonitor;
        rect.left <= monitor.left
            && rect.top <= monitor.top
            && rect.right >= monitor.right
            && rect.bottom >= monitor.bottom
    }
}

#[cfg(not(windows))]
fn foreground_fullscreen_on_monitor(_reference_hwnd: *mut core::ffi::c_void) -> bool {
    false
}
