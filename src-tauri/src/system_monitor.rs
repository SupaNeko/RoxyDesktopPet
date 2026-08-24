//! 只读系统状态感知：任务栏应用列表、前台窗口、硬件占用。
//!
//! 隐私模型：所有能力都需要用户在「设置 → 系统感知」中逐项授权后才被调用；
//! 数据只在调用时实时读取，不落盘、不上传。

use crate::db::AppSettings;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct TaskbarApp {
    pub title: String,
    pub process_name: String,
    pub pid: u32,
    pub foreground: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct HardwareStats {
    pub cpu_usage_percent: f32,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub gpu_name: Option<String>,
    pub gpu_usage_percent: Option<f32>,
    pub gpu_memory_used_mb: Option<u64>,
    pub gpu_memory_total_mb: Option<u64>,
    pub gpu_temperature_celsius: Option<f32>,
}

/// Shell 任务栏显示规则的近似实现（纯函数，便于测试）：
/// 可见的、无 owner 的、非工具窗口且有标题的顶层窗口才会出现在任务栏。
fn should_include_window(
    visible: bool,
    has_owner: bool,
    tool_window: bool,
    title_len: usize,
    cloaked: bool,
) -> bool {
    visible && !has_owner && !tool_window && title_len > 0 && !cloaked
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetWindow, GetWindowLongW, GetWindowTextLengthW,
        GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, GWL_EXSTYLE, GW_OWNER,
        WS_EX_TOOLWINDOW,
    };

    pub struct RawWindow {
        pub hwnd: HWND,
        pub title: String,
        pub pid: u32,
    }

    fn read_window(hwnd: HWND, apply_filters: bool) -> Option<RawWindow> {
        unsafe {
            if apply_filters {
                let visible = IsWindowVisible(hwnd) != 0;
                let has_owner = !GetWindow(hwnd, GW_OWNER).is_null();
                let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
                let title_len = GetWindowTextLengthW(hwnd).max(0) as usize;
                let mut cloaked: u32 = 0;
                let _ = DwmGetWindowAttribute(
                    hwnd,
                    DWMWA_CLOAKED as u32,
                    &mut cloaked as *mut u32 as *mut _,
                    std::mem::size_of::<u32>() as u32,
                );
                if !super::should_include_window(
                    visible,
                    has_owner,
                    ex_style & WS_EX_TOOLWINDOW != 0,
                    title_len,
                    cloaked != 0,
                ) {
                    return None;
                }
            }
            let title_len = GetWindowTextLengthW(hwnd);
            if title_len <= 0 {
                return None;
            }
            let mut buffer = vec![0u16; (title_len + 1) as usize];
            let copied = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
            if copied <= 0 {
                return None;
            }
            let title = String::from_utf16_lossy(&buffer[..copied as usize]);
            if apply_filters && title == "Program Manager" {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            Some(RawWindow { hwnd, title, pid })
        }
    }

    unsafe extern "system" fn enum_window_proc(
        hwnd: HWND,
        lparam: windows_sys::Win32::Foundation::LPARAM,
    ) -> windows_sys::Win32::Foundation::BOOL {
        let windows = &mut *(lparam as *mut Vec<RawWindow>);
        if let Some(raw) = read_window(hwnd, true) {
            windows.push(raw);
        }
        1
    }

    pub fn taskbar_windows() -> (Vec<RawWindow>, HWND) {
        let mut windows: Vec<RawWindow> = Vec::new();
        unsafe {
            let _ = EnumWindows(
                Some(enum_window_proc),
                &mut windows as *mut Vec<RawWindow> as isize,
            );
            (windows, GetForegroundWindow())
        }
    }

    pub fn foreground_window() -> Option<RawWindow> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_null() {
            return None;
        }
        read_window(hwnd, false)
    }
}

fn process_names(pids: &[u32]) -> std::collections::HashMap<u32, String> {
    use sysinfo::{ProcessesToUpdate, System};
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);
    pids.iter()
        .filter_map(|pid| {
            system
                .process(sysinfo::Pid::from_u32(*pid))
                .map(|process| (*pid, process.name().to_string_lossy().into_owned()))
        })
        .collect()
}

#[cfg(windows)]
pub fn taskbar_apps() -> Vec<TaskbarApp> {
    let (windows, foreground) = imp::taskbar_windows();
    let pids: Vec<u32> = windows.iter().map(|w| w.pid).collect();
    let names = process_names(&pids);
    windows
        .into_iter()
        .map(|raw| TaskbarApp {
            title: raw.title,
            process_name: names.get(&raw.pid).cloned().unwrap_or_default(),
            pid: raw.pid,
            foreground: !foreground.is_null() && raw.hwnd == foreground,
        })
        .collect()
}

#[cfg(not(windows))]
pub fn taskbar_apps() -> Vec<TaskbarApp> {
    Vec::new()
}

#[cfg(windows)]
pub fn foreground_app() -> Option<(String, String)> {
    let raw = imp::foreground_window()?;
    let names = process_names(&[raw.pid]);
    Some((raw.title, names.get(&raw.pid).cloned().unwrap_or_default()))
}

#[cfg(not(windows))]
pub fn foreground_app() -> Option<(String, String)> {
    None
}

struct NvidiaGpuStats {
    name: String,
    usage_percent: f32,
    memory_used_mb: u64,
    memory_total_mb: u64,
    temperature_celsius: f32,
}

/// 通过 nvidia-smi 读取显卡占用与温度（无需管理员权限）；非 NVIDIA 机器返回 None。
fn nvidia_gpu_stats() -> Option<NvidiaGpuStats> {
    let mut command = std::process::Command::new("nvidia-smi");
    command.args([
        "--query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu",
        "--format=csv,noheader,nounits",
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()?
        .trim()
        .to_string();
    let parts: Vec<&str> = line.split(',').map(str::trim).collect();
    if parts.len() < 5 {
        return None;
    }
    Some(NvidiaGpuStats {
        name: parts[0].to_string(),
        usage_percent: parts[1].parse().ok()?,
        memory_used_mb: parts[2].parse().ok()?,
        memory_total_mb: parts[3].parse().ok()?,
        temperature_celsius: parts[4].parse().ok()?,
    })
}

pub fn hardware_stats() -> HardwareStats {
    use sysinfo::System;
    let mut system = System::new();
    // CPU 占用率需要两次采样取差值。
    system.refresh_cpu_all();
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    system.refresh_cpu_all();
    system.refresh_memory();
    let gpu = nvidia_gpu_stats();
    HardwareStats {
        cpu_usage_percent: system.global_cpu_usage(),
        memory_used_mb: system.used_memory() / 1024 / 1024,
        memory_total_mb: system.total_memory() / 1024 / 1024,
        gpu_name: gpu.as_ref().map(|g| g.name.clone()),
        gpu_usage_percent: gpu.as_ref().map(|g| g.usage_percent),
        gpu_memory_used_mb: gpu.as_ref().map(|g| g.memory_used_mb),
        gpu_memory_total_mb: gpu.as_ref().map(|g| g.memory_total_mb),
        gpu_temperature_celsius: gpu.as_ref().map(|g| g.temperature_celsius),
    }
}

/// 供对话系统提示词注入的实时状态摘要。所有项目都未授权时返回空串。
pub async fn context_summary(settings: &AppSettings) -> String {
    let mut parts: Vec<String> = Vec::new();
    if settings.system_status_enabled {
        let stats = tokio::task::spawn_blocking(hardware_stats)
            .await
            .unwrap_or_else(|_| HardwareStats {
                cpu_usage_percent: 0.0,
                memory_used_mb: 0,
                memory_total_mb: 0,
                gpu_name: None,
                gpu_usage_percent: None,
                gpu_memory_used_mb: None,
                gpu_memory_total_mb: None,
                gpu_temperature_celsius: None,
            });
        let memory_percent =
            stats.memory_used_mb as f64 * 100.0 / stats.memory_total_mb.max(1) as f64;
        let mut line = format!(
            "硬件：CPU 占用 {:.0}%，内存 {:.0}%（已用 {} MB / 共 {} MB）",
            stats.cpu_usage_percent, memory_percent, stats.memory_used_mb, stats.memory_total_mb
        );
        if let Some(name) = &stats.gpu_name {
            line.push_str(&format!(
                "；显卡 {} 占用 {:.0}%",
                name,
                stats.gpu_usage_percent.unwrap_or(0.0)
            ));
            if let Some(temperature) = stats.gpu_temperature_celsius {
                line.push_str(&format!("，温度 {:.0}℃", temperature));
            }
        }
        parts.push(line);
    }
    if settings.taskbar_apps_enabled {
        let apps = tokio::task::spawn_blocking(taskbar_apps)
            .await
            .unwrap_or_default();
        let titles: Vec<String> = apps
            .iter()
            .take(15)
            .map(|app| format!("{}({})", app.title, app.process_name))
            .collect();
        if !titles.is_empty() {
            parts.push(format!("任务栏正在运行的应用：{}", titles.join("、")));
        }
        let foreground = tokio::task::spawn_blocking(foreground_app)
            .await
            .unwrap_or(None);
        if let Some((title, process)) = foreground {
            parts.push(format!("用户当前正在使用：{}（{}）", title, process));
        }
    }
    if settings.now_playing_enabled {
        match crate::media_control::now_playing().await {
            Ok(Some(now)) => parts.push(format!(
                "正在播放：{} - {}（来自 {}，{}）",
                now.title,
                now.artist,
                now.source_app,
                if now.playing {
                    "播放中"
                } else {
                    "已暂停"
                }
            )),
            Ok(None) => parts.push("当前没有检测到正在播放的媒体".into()),
            Err(error) => log_warn!("读取当前播放失败：{error}"),
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(
            "\n用户电脑的实时状态（已获用户逐项授权，仅用于回答相关问题，不要主动向用户罗列）：\n{}",
            parts.join("\n")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taskbar_filter_follows_shell_rules() {
        assert!(super::should_include_window(true, false, false, 3, false));
        assert!(!super::should_include_window(false, false, false, 3, false));
        assert!(!super::should_include_window(true, true, false, 3, false));
        assert!(!super::should_include_window(true, false, true, 3, false));
        assert!(!super::should_include_window(true, false, false, 0, false));
        assert!(!super::should_include_window(true, false, false, 3, true));
    }

    /// 真实桌面会话冒烟测试：cargo test --lib -- --ignored --nocapture
    #[test]
    #[ignore = "需要真实 Windows 桌面会话，手动运行"]
    fn live_system_snapshot_smoke() {
        let apps = taskbar_apps();
        println!("taskbar apps ({}):", apps.len());
        for app in &apps {
            println!(
                "  {} | {} | pid={} | fg={}",
                app.title, app.process_name, app.pid, app.foreground
            );
        }
        println!("foreground: {:?}", foreground_app());
        println!("hardware: {:?}", hardware_stats());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        println!(
            "now playing: {:?}",
            runtime.block_on(crate::media_control::now_playing())
        );
    }
}
