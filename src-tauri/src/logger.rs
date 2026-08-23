use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static LOG_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();

fn log_dir() -> PathBuf {
    crate::data_dir().join("logs")
}

fn timestamp() -> String {
    let now = chrono::Utc::now();
    format!(
        "{}.{:03}",
        now.format("%Y-%m-%d %H:%M:%S"),
        now.timestamp_subsec_millis()
    )
}

fn write_line(line: &str) {
    let full = format!("{} {}\n", timestamp(), line);
    if let Some(mutex) = LOG_FILE.get() {
        if let Ok(mut guard) = mutex.lock() {
            if let Some(file) = guard.as_mut() {
                let _ = file.write_all(full.as_bytes());
                let _ = file.flush();
            }
        }
    }
    #[cfg(debug_assertions)]
    eprint!("{full}");
}

pub fn info(message: String) {
    write_line(&format!("[INFO ] {message}"));
}

pub fn warn(message: String) {
    write_line(&format!("[WARN ] {message}"));
}

pub fn error(message: String) {
    write_line(&format!("[ERROR] {message}"));
}

#[allow(dead_code)]
pub fn debug(message: String) {
    write_line(&format!("[DEBUG] {message}"));
}

/// panic 时独立写一份 crash.log，避免主日志因缓冲/锁未刷新而丢失最后一条。
fn write_crash(report: &str) {
    let dir = log_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("crash.log");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{}\n{}\n", timestamp(), report);
        let _ = file.flush();
    }
}

pub fn init() {
    let dir = log_dir();
    let _ = std::fs::create_dir_all(&dir);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("chatpet.log"))
        .ok();
    let _ = LOG_FILE.set(Mutex::new(file));

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "未知位置".to_string());
        let message = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "未知 panic 载荷".to_string()
        };
        let report = format!("[PANIC] {location} {message}");
        write_line(&report);
        write_crash(&report);
        default_hook(info);
    }));

    info(format!(
        "日志系统已初始化，日志目录：{}",
        dir.display()
    ));
}

macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::logger::info(format!($($arg)*))
    };
}

macro_rules! log_warn {
    ($($arg:tt)*) => {
        $crate::logger::warn(format!($($arg)*))
    };
}

macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::logger::error(format!($($arg)*))
    };
}

#[allow(unused_macros)]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        $crate::logger::debug(format!($($arg)*))
    };
}
