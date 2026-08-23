use serde::Deserialize;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

const HEALTH_URL: &str = "http://127.0.0.1:9880/docs";
const EXPECTED_MODEL_VERSION: &str = "v2ProPlus";

#[derive(Debug, Deserialize)]
struct VoiceBundleManifest {
    bundle_version: String,
    model_version: String,
    python: String,
    api_entry: String,
    config: String,
    references: String,
}

pub struct GptSoVitsState {
    inner: Mutex<Runtime>,
}

struct Runtime {
    child: Option<Child>,
    status: String,
    detail: String,
}

#[derive(Debug, Clone)]
pub struct VoiceRuntimeStatus {
    pub status: String,
    pub detail: String,
    pub gpu: Option<String>,
}

impl Default for GptSoVitsState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Runtime {
                child: None,
                status: "not_configured".into(),
                detail: "请将洛琪希语音扩展包解压到程序目录下的 voice 文件夹".into(),
            }),
        }
    }
}

pub fn extension_root() -> PathBuf {
    if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("voice")
    } else {
        std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("voice")
    }
}

fn resolve_relative(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("语音扩展清单包含不安全路径：{relative}"));
    }
    Ok(root.join(path))
}

fn load_manifest() -> Result<(VoiceBundleManifest, PathBuf), String> {
    let root = extension_root();
    let path = root.join("manifest.json");
    let content = std::fs::read_to_string(&path)
        .map_err(|_| format!("未找到语音扩展：{}", path.display()))?;
    let manifest: VoiceBundleManifest =
        serde_json::from_str(&content).map_err(|error| format!("语音扩展清单无效：{error}"))?;
    if manifest.model_version != EXPECTED_MODEL_VERSION {
        return Err(format!(
            "语音扩展模型版本不匹配：需要 {EXPECTED_MODEL_VERSION}，实际为 {}",
            manifest.model_version
        ));
    }
    if manifest.bundle_version.trim().is_empty() {
        return Err("语音扩展清单缺少 bundle_version".into());
    }
    for (name, relative) in [
        ("Python", &manifest.python),
        ("API", &manifest.api_entry),
        ("配置", &manifest.config),
        ("参考音频", &manifest.references),
    ] {
        let path = resolve_relative(&root, relative)?;
        if !path.exists() {
            return Err(format!("语音扩展缺少{name}：{}", path.display()));
        }
    }
    Ok((manifest, root))
}

pub fn references_root() -> Result<PathBuf, String> {
    let (manifest, root) = load_manifest()?;
    resolve_relative(&root, &manifest.references)
}

fn nvidia_gpu() -> Result<String, String> {
    let mut command = std::process::Command::new("nvidia-smi");
    command.args([
        "--query-gpu=name,driver_version,memory.total",
        "--format=csv,noheader,nounits",
    ]);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let output = command
        .output()
        .map_err(|_| "未检测到 NVIDIA 驱动或 nvidia-smi".to_string())?;
    if !output.status.success() {
        return Err("NVIDIA 驱动检测失败".into());
    }
    let line = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if line.is_empty() {
        Err("未检测到可用的 NVIDIA GPU".into())
    } else {
        Ok(line)
    }
}

async fn health_available() -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(800))
        .build()
    else {
        return false;
    };
    client
        .get(HEALTH_URL)
        .send()
        .await
        .is_ok_and(|response| response.status().is_success())
}

async fn python_cuda_check(python: &Path, root: &Path) -> Result<String, String> {
    let mut command = Command::new(python);
    command
        .current_dir(root)
        .args([
            "-c",
            "import torch; assert torch.cuda.is_available(), 'torch.cuda.is_available() is false'; print(f'{torch.__version__}|{torch.version.cuda}|{torch.cuda.get_device_name(0)}')",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let output = tokio::time::timeout(std::time::Duration::from_secs(30), command.output())
        .await
        .map_err(|_| "PyTorch CUDA 自检超时".to_string())?
        .map_err(|error| format!("无法执行语音扩展 Python：{error}"))?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(format!("PyTorch CUDA 自检失败：{}", error.trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub async fn status(state: &GptSoVitsState) -> VoiceRuntimeStatus {
    let gpu = match nvidia_gpu() {
        Ok(gpu) => gpu,
        Err(detail) => {
            return VoiceRuntimeStatus {
                status: "unsupported_gpu".into(),
                detail,
                gpu: None,
            }
        }
    };
    if let Err(detail) = load_manifest() {
        return VoiceRuntimeStatus {
            status: "not_configured".into(),
            detail,
            gpu: Some(gpu),
        };
    }
    if health_available().await {
        return VoiceRuntimeStatus {
            status: "available".into(),
            detail: "GPT-SoVITS 本地推理服务已就绪".into(),
            gpu: Some(gpu),
        };
    }
    let inner = state.inner.lock().await;
    VoiceRuntimeStatus {
        status: inner.status.clone(),
        detail: inner.detail.clone(),
        gpu: Some(gpu),
    }
}

pub async fn ensure(state: &GptSoVitsState) -> Result<(), String> {
    match ensure_inner(state).await {
        Ok(()) => Ok(()),
        Err(error) => {
            let mut inner = state.inner.lock().await;
            inner.status = if error.contains("未检测到 NVIDIA") || error.contains("NVIDIA 驱动")
            {
                "unsupported_gpu".into()
            } else if error.contains("未找到语音扩展") || error.contains("语音扩展缺少")
            {
                "not_configured".into()
            } else {
                "error".into()
            };
            inner.detail = error.clone();
            Err(error)
        }
    }
}

async fn ensure_inner(state: &GptSoVitsState) -> Result<(), String> {
    let gpu = nvidia_gpu()?;
    let (manifest, root) = load_manifest()?;
    if health_available().await {
        let mut inner = state.inner.lock().await;
        inner.status = "available".into();
        inner.detail = format!("GPT-SoVITS 已就绪，GPU：{gpu}");
        return Ok(());
    }

    let python = resolve_relative(&root, &manifest.python)?;
    let api = resolve_relative(&root, &manifest.api_entry)?;
    let config = resolve_relative(&root, &manifest.config)?;
    let cuda = python_cuda_check(&python, &root).await?;

    {
        let mut inner = state.inner.lock().await;
        if let Some(child) = inner.child.as_mut() {
            if child
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_none()
            {
                return Ok(());
            }
            inner.child = None;
        }
        std::fs::create_dir_all(crate::data_dir().join("logs")).map_err(|e| e.to_string())?;
        let stdout = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(crate::data_dir().join("logs").join("gpt-sovits.log"))
            .map_err(|e| e.to_string())?;
        let stderr = stdout.try_clone().map_err(|e| e.to_string())?;
        let mut command = Command::new(&python);
        // api_v2.py 及其内部模块（TTS.py / sv.py 等）大量依赖 os.getcwd() 定位
        // GPT_SoVITS 包和模型路径，官方约定 cwd 必须是 GPT-SoVITS 仓库根目录。
        let sovits_dir = api
            .parent()
            .ok_or("语音扩展 API 路径无效")?
            .to_path_buf();
        command
            .current_dir(&sovits_dir)
            .arg(&api)
            .args(["-a", "127.0.0.1", "-p", "9880", "-c"])
            .arg(&config)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .kill_on_drop(true)
            .env("PYTHONIOENCODING", "utf-8");
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        inner.child = Some(
            command
                .spawn()
                .map_err(|error| format!("启动 GPT-SoVITS 失败：{error}"))?,
        );
        inner.status = "starting".into();
        inner.detail = format!("正在加载 v2ProPlus 模型，{cuda}");
    }

    for _ in 0..240 {
        if health_available().await {
            let mut inner = state.inner.lock().await;
            inner.status = "available".into();
            inner.detail = format!("GPT-SoVITS 已就绪，GPU：{gpu}");
            return Ok(());
        }
        {
            let mut inner = state.inner.lock().await;
            if let Some(child) = inner.child.as_mut() {
                if let Some(exit) = child.try_wait().map_err(|error| error.to_string())? {
                    inner.child = None;
                    inner.status = "error".into();
                    inner.detail =
                        format!("GPT-SoVITS 启动失败（{exit}），请查看 data\\logs\\gpt-sovits.log");
                    return Err(inner.detail.clone());
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    let mut inner = state.inner.lock().await;
    if let Some(child) = inner.child.as_mut() {
        let _ = child.start_kill();
    }
    inner.child = None;
    inner.status = "error".into();
    inner.detail = "GPT-SoVITS 启动超时，请查看 data\\logs\\gpt-sovits.log".into();
    Err(inner.detail.clone())
}

pub async fn shutdown(state: &GptSoVitsState) {
    let mut inner = state.inner.lock().await;
    if let Some(child) = inner.child.as_mut() {
        let _ = child.start_kill();
    }
    inner.child = None;
    inner.status = "not_started".into();
}
