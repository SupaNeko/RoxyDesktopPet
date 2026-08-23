use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    process::{Child, Command},
    sync::{Mutex, RwLock},
};

pub struct QdrantRuntime {
    child: Mutex<Option<Child>>,
    status: RwLock<String>,
}

impl Default for QdrantRuntime {
    fn default() -> Self {
        Self {
            child: Mutex::new(None),
            status: RwLock::new("not_started".into()),
        }
    }
}

async fn healthy(url: &str) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    else {
        return false;
    };
    client
        .get(format!("{}/collections", url.trim_end_matches('/')))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

fn runtime_candidates() -> Vec<PathBuf> {
    let mut paths = vec![crate::data_dir().join("qdrant_runtime").join("qdrant.exe")];
    if cfg!(debug_assertions) {
        paths.push(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources")
                .join("qdrant")
                .join("qdrant.exe"),
        );
    } else if let Ok(executable) = std::env::current_exe() {
        if let Some(root) = executable.parent() {
            paths.push(root.join("resources").join("qdrant").join("qdrant.exe"));
            paths.push(root.join("qdrant").join("qdrant.exe"));
        }
    }
    if let Ok(output) = std::process::Command::new("where.exe")
        .arg("qdrant.exe")
        .output()
    {
        if output.status.success() {
            paths.extend(
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .map(|line| PathBuf::from(line.trim())),
            );
        }
    }
    paths
}

pub async fn ensure(runtime: &QdrantRuntime, url: &str) -> Result<String, String> {
    if healthy(url).await {
        *runtime.status.write().await = "external".into();
        return Ok("external".into());
    }
    if !(url.contains("127.0.0.1") || url.contains("localhost")) {
        *runtime.status.write().await = "unavailable".into();
        return Err("外部 Qdrant 地址不可用".into());
    }
    let executable = runtime_candidates()
        .into_iter()
        .find(|path| path.is_file())
        .ok_or("未找到 qdrant.exe")?;
    let runtime_dir = crate::data_dir().join("qdrant_runtime");
    let storage_dir = crate::data_dir().join("qdrant").join("storage");
    let snapshots_dir = crate::data_dir().join("qdrant").join("snapshots");
    std::fs::create_dir_all(&runtime_dir)
        .map_err(|e| format!("无法创建 Qdrant Runtime 目录：{e}"))?;
    std::fs::create_dir_all(&storage_dir).map_err(|e| format!("无法创建 Qdrant 数据目录：{e}"))?;
    std::fs::create_dir_all(&snapshots_dir)
        .map_err(|e| format!("无法创建 Qdrant 快照目录：{e}"))?;
    let config_path = runtime_dir.join("chatpet-qdrant.yaml");
    let storage = storage_dir.to_string_lossy().replace('\\', "/");
    let snapshots = snapshots_dir.to_string_lossy().replace('\\', "/");
    std::fs::write(&config_path,format!("storage:\n  storage_path: \"{storage}\"\n  snapshots_path: \"{snapshots}\"\nservice:\n  host: 127.0.0.1\n  http_port: 6333\n  grpc_port: 6334\ntelemetry_disabled: true\n")).map_err(|e|format!("无法写入 Qdrant 配置：{e}"))?;
    let mut command = Command::new(executable);
    command
        // Qdrant 的临时快照目录默认相对当前工作目录。开发模式下如果继承
        // `src-tauri`，运行时写入会被 Tauri watcher 误认为源码变化并无限重启。
        .current_dir(&runtime_dir)
        .arg("--config-path")
        .arg(config_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        command.creation_flags(0x08000000);
    }
    let child = command
        .spawn()
        .map_err(|e| format!("无法启动 qdrant.exe：{e}"))?;
    *runtime.child.lock().await = Some(child);
    *runtime.status.write().await = "starting".into();
    for _ in 0..30 {
        if healthy(url).await {
            *runtime.status.write().await = "owned".into();
            return Ok("owned".into());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    shutdown(runtime).await;
    *runtime.status.write().await = "error".into();
    Err("Qdrant 启动后健康检查超时".into())
}

pub async fn shutdown(runtime: &QdrantRuntime) {
    if let Some(mut child) = runtime.child.lock().await.take() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    *runtime.status.write().await = "stopped".into();
}

pub async fn status(runtime: &QdrantRuntime) -> String {
    runtime.status.read().await.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "requires bundled qdrant.exe"]
    async fn bundled_runtime_starts_and_restarts() {
        let runtime = QdrantRuntime::default();
        let url = "http://127.0.0.1:6333";
        assert!(ensure(&runtime, url).await.is_ok());
        assert!(healthy(url).await);
        shutdown(&runtime).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(ensure(&runtime, url).await.is_ok());
        assert!(healthy(url).await);
        shutdown(&runtime).await;
    }
}
