use base64::Engine;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::io::BufReader as StdBufReader;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use crate::db::{AppSettings, DbState, Message};
use crate::ModelConfig;

pub struct VoiceOutputState {
    pipeline: Mutex<()>,
    runtime: Mutex<VitsRuntime>,
}

impl VoiceOutputState {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            pipeline: Mutex::new(()),
            runtime: Mutex::new(VitsRuntime::new(data_dir)),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VitsModelInfo {
    pub name: String,
    pub path: String,
    pub language: Option<String>,
    pub speakers: Vec<String>,
    pub has_config: bool,
}

#[derive(Serialize)]
struct VitsRequest {
    action: String,
    text: Option<String>,
    model_path: Option<String>,
    speaker_id: Option<String>,
    emotion_params: Option<String>,
    speed: Option<f64>,
    target_language: Option<String>,
    output_path: Option<String>,
}

#[derive(Deserialize)]
struct VitsResponse {
    success: bool,
    message: Option<String>,
    output_path: Option<String>,
}

#[derive(Deserialize)]
struct VitsReady {
    ready: bool,
}

struct VitsRuntime {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stdout: Option<BufReader<ChildStdout>>,
    exe: PathBuf,
}

impl VitsRuntime {
    fn new(data_dir: &Path) -> Self {
        Self {
            child: None,
            stdin: None,
            stdout: None,
            exe: data_dir.join("vits_runtime").join("vits_runtime.exe"),
        }
    }
    async fn start(&mut self) -> Result<(), String> {
        if self.child.is_some() {
            return Ok(());
        }
        if !self.exe.exists() {
            return Err(format!("未找到 VITS Runtime：{}", self.exe.display()));
        }
        let mut command = Command::new(&self.exe);
        command
            .current_dir(self.exe.parent().unwrap())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .env("PYTHONIOENCODING", "utf-8");
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let mut child = command
            .spawn()
            .map_err(|e| format!("启动 VITS 失败：{e}"))?;
        self.stdin = child.stdin.take();
        self.stdout = child.stdout.take().map(BufReader::new);
        self.child = Some(child);
        let mut line = String::new();
        let read = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            self.stdout
                .as_mut()
                .ok_or("VITS stdout 不可用")?
                .read_line(&mut line),
        )
        .await
        .map_err(|_| "等待 VITS 就绪超时".to_string())?
        .map_err(|e| e.to_string())?;
        if read == 0
            || !serde_json::from_str::<VitsReady>(line.trim())
                .map_err(|e| format!("VITS 就绪响应无效：{e}"))?
                .ready
        {
            self.stop();
            return Err("VITS Runtime 未就绪".into());
        }
        Ok(())
    }
    async fn generate(&mut self, request: &VitsRequest) -> Result<PathBuf, String> {
        self.start().await?;
        let json = serde_json::to_string(request).map_err(|e| e.to_string())?;
        let stdin = self.stdin.as_mut().ok_or("VITS stdin 不可用")?;
        stdin
            .write_all(json.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        stdin.write_all(b"\n").await.map_err(|e| e.to_string())?;
        stdin.flush().await.map_err(|e| e.to_string())?;
        let mut line = String::new();
        let read = tokio::time::timeout(
            std::time::Duration::from_secs(600),
            self.stdout
                .as_mut()
                .ok_or("VITS stdout 不可用")?
                .read_line(&mut line),
        )
        .await
        .map_err(|_| "VITS 合成超时".to_string())?
        .map_err(|e| e.to_string())?;
        if read == 0 {
            self.stop();
            return Err("VITS Runtime 意外退出".into());
        }
        let response: VitsResponse =
            serde_json::from_str(line.trim()).map_err(|e| format!("VITS 响应无效：{e}"))?;
        if !response.success {
            return Err(response.message.unwrap_or_else(|| "VITS 合成失败".into()));
        }
        response
            .output_path
            .map(PathBuf::from)
            .or_else(|| request.output_path.as_ref().map(PathBuf::from))
            .ok_or("VITS 未返回音频路径".into())
    }
    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
        }
        self.stdin = None;
        self.stdout = None;
    }
}

impl Drop for VitsRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn scan_models(data_dir: &Path) -> Result<Vec<VitsModelInfo>, String> {
    let root = data_dir.join("vits_models");
    if !root.exists() {
        return Ok(vec![]);
    }
    let mut result = Vec::new();
    for entry in std::fs::read_dir(root)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let files: Vec<_> = std::fs::read_dir(&path)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .collect();
        if !files
            .iter()
            .any(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pth")))
        {
            continue;
        }
        let config = if path.join("config.json").exists() {
            Some(path.join("config.json"))
        } else {
            files
                .iter()
                .find(|p| {
                    p.extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
                })
                .cloned()
        };
        let mut language = None;
        let mut speakers = Vec::new();
        if let Some(config) = &config {
            if let Ok(value) = std::fs::read_to_string(config)
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .ok_or(())
            {
                language = value
                    .pointer("/data/language")
                    .or_else(|| value.pointer("/model/language"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                if let Some(items) = value.get("speakers").and_then(|v| v.as_array()) {
                    speakers = items
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect();
                } else if let Some(items) = value.get("speakers").and_then(|v| v.as_object()) {
                    let mut pairs: Vec<_> = items
                        .iter()
                        .filter_map(|(k, v)| v.as_i64().map(|id| (id, k.clone())))
                        .collect();
                    pairs.sort_by_key(|p| p.0);
                    speakers = pairs.into_iter().map(|p| p.1).collect();
                }
            }
        }
        result.push(VitsModelInfo {
            name: entry.file_name().to_string_lossy().into(),
            path: path.to_string_lossy().into(),
            language,
            speakers,
            has_config: config.is_some(),
        });
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}

pub fn schedule(app: AppHandle, message: Message) {
    let trigger = message.trigger_type.clone();
    let emotion = message.emotion.clone();
    // 打断：新语音到来时停掉尚未播完的旧语音，并作废旧播放任务。
    let epoch = interrupt_playback();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = generate_and_play(&app, &message, epoch).await {
            log_error!("voice output failed (trigger={trigger}, emotion={:?}): {error}", emotion);
            let _ = app.emit("voice-output-error", error);
        }
    });
}

pub async fn test(app: &AppHandle) -> Result<(), String> {
    let message = Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: "assistant".into(),
        content: "你好，我是你的桌面伙伴。".into(),
        japanese_text: Some("こんにちは。ロキシーです。".into()),
        emotion: Some("calm".into()),
        trigger_type: "voice_test".into(),
        created_at: chrono::Utc::now().timestamp_millis(),
        session: "main".into(),
        quiz: None,
    };
    let epoch = interrupt_playback();
    generate_and_play(app, &message, epoch).await
}

// ---- 播放打断：epoch 递增作废旧任务，CURRENT_SINK 注册当前播放器供新任务停止 ----
static PLAY_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static CURRENT_SINK: std::sync::Mutex<Option<(u64, std::sync::Arc<rodio::Sink>)>> = std::sync::Mutex::new(None);

/// 递增播放纪元并停止当前正在播放的语音，返回新纪元。
fn interrupt_playback() -> u64 {
    let epoch = PLAY_EPOCH.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    if let Ok(mut guard) = CURRENT_SINK.lock() {
        if let Some((_, sink)) = guard.take() {
            sink.stop();
        }
    }
    epoch
}

/// 当前任务是否已被更新的语音打断。
fn playback_stale(epoch: u64) -> bool {
    PLAY_EPOCH.load(std::sync::atomic::Ordering::SeqCst) != epoch
}

/// 注册当前 sink（供后续打断）；纪元已过期则不注册并返回 false（调用方应停止播放）。
fn register_sink(epoch: u64, sink: &std::sync::Arc<rodio::Sink>) -> bool {
    if playback_stale(epoch) {
        return false;
    }
    if let Ok(mut guard) = CURRENT_SINK.lock() {
        *guard = Some((epoch, sink.clone()));
    }
    true
}

/// 播放自然结束时注销（只在自己仍是当前播放器时）。
fn unregister_sink(epoch: u64) {
    if let Ok(mut guard) = CURRENT_SINK.lock() {
        if guard.as_ref().is_some_and(|(current, _)| *current == epoch) {
            *guard = None;
        }
    }
}

async fn generate_and_play(app: &AppHandle, message: &Message, epoch: u64) -> Result<(), String> {
    let state = app.state::<VoiceOutputState>();
    let _guard = state.pipeline.lock().await;
    let db = app.state::<DbState>();
    let mut settings = {
        let conn = db.0.lock().await;
        crate::db::get_settings(&conn, true).map_err(|e| e.to_string())?
    };
    log_info!(
        "generate_and_play start: mode={}, trigger={}, id={}",
        settings.voice_output_mode,
        message.trigger_type,
        message.id
    );
    if settings.tts_api_key.is_empty() {
        settings.tts_api_key = app
            .state::<crate::MemoryEnvConfig>()
            .embedding_api_key
            .clone();
    }
    if settings.voice_output_mode == "disabled" {
        return Ok(());
    }
    let cache = crate::data_dir().join("voice_cache");
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    if settings.voice_output_mode == "gpt_sovits" {
        // 流式合成：边下边播，函数内部完成播放与缓存落盘
        return gpt_sovits_tts(app, message, &cache.join(format!("{}.wav", message.id)), epoch).await;
    }
    let path = if settings.voice_output_mode == "api" {
        api_tts(&settings, &message.content, &cache.join(&message.id)).await?
    } else if settings.voice_output_mode == "vits" {
        if !crate::data_dir()
            .join("vits_runtime")
            .join("vits_runtime.exe")
            .exists()
        {
            return Ok(());
        }
        if settings.vits_model_path.is_empty() {
            return Err("尚未选择 VITS 模型".into());
        }
        let text = if settings.vits_translate_enabled {
            translate(app, &settings, &message.content).await?
        } else {
            message.content.clone()
        };
        let output = cache.join(format!("{}.wav", message.id));
        let request = VitsRequest {
            action: "generate".into(),
            text: Some(text),
            model_path: Some(settings.vits_model_path.clone()),
            speaker_id: settings.vits_speaker_id.clone(),
            emotion_params: (!settings.vits_emotion_params.is_empty())
                .then_some(settings.vits_emotion_params.clone()),
            speed: Some(settings.vits_speed),
            target_language: Some(settings.vits_target_language.clone()),
            output_path: Some(output.to_string_lossy().into()),
        };
        state.runtime.lock().await.generate(&request).await?
    } else {
        return Ok(());
    };
    // 合成完成后若已被新语音打断，直接丢弃不播。
    if playback_stale(epoch) {
        return Ok(());
    }
    play(path, epoch).await
}

/// GPT-SoVITS 流式合成：边合成边播放，结束后落盘缓存。
/// api_v2.py 在 streaming_mode 2/3 + media_type "wav" 下，先返回一个占位长度的
/// WAV 头，随后按合成进度返回裸 int16 PCM chunk；合成结束后用
/// normalize_streaming_wav_header 修正长度字段，保证缓存文件仍是合法 WAV。
async fn gpt_sovits_tts(
    app: &AppHandle,
    message: &Message,
    output: &Path,
    epoch: u64,
) -> Result<(), String> {
    let text = message
        .japanese_text
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .ok_or("GPT-SoVITS 消息缺少日文文本")?;
    let runtime = app.state::<crate::gpt_sovits::GptSoVitsState>();
    crate::gpt_sovits::ensure(runtime.inner()).await?;
    log_info!("gpt_sovits_tts: runtime ready, synthesizing {} chars", text.chars().count());
    let folder = match message.emotion.as_deref().unwrap_or("calm") {
        "shy" => "害羞",
        "affectionate" => "撒娇",
        "sad" => "委屈",
        "happy" => "欣慰",
        "angry" => "责备",
        "battle" => "战斗",
        "self_deprecating" => "自嘲",
        _ => "慵懒",
    };
    let root = crate::gpt_sovits::references_root()?;
    let candidates = std::fs::read_dir(root.join(folder))
        .map_err(|e| format!("读取情绪参考音频失败：{e}"))?
        .filter_map(Result::ok)
        .map(|v| v.path())
        .filter(|p| {
            p.extension()
                .and_then(|v| v.to_str())
                .is_some_and(|v| v.eq_ignore_ascii_case("wav"))
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err(format!("情绪 {folder} 没有参考音频"));
    }
    let reference = &candidates[rand::random_range(0..candidates.len())];
    let prompt = reference
        .file_stem()
        .and_then(|v| v.to_str())
        .ok_or("参考音频文件名不是有效日语文本")?;
    // 本机回环地址必须绕过系统代理：Windows 代理例外列表的通配符（如 127.*）
    // reqwest 解析不完整，Clash 等代理会拦截 127.0.0.1:9880 并返回 502。
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("GPT-SoVITS 客户端初始化失败：{e}"))?;
    let response = client.post("http://127.0.0.1:9880/tts").json(&serde_json::json!({
        "text":text,"text_lang":"ja","ref_audio_path":reference.to_string_lossy(),"prompt_text":prompt,"prompt_lang":"ja",
        "text_split_method":"cut5","batch_size":1,"media_type":"wav","streaming_mode":2,"top_k":15,"top_p":1.0,"temperature":1.0,"speed_factor":1.0
    })).send().await.map_err(|e|format!("GPT-SoVITS 请求失败：{e}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("GPT-SoVITS 返回 {status}: {body}"));
    }

    // 播放线程消费字节流；本任务同时收集完整字节用于缓存。
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let player = tokio::task::spawn_blocking(move || play_pcm_stream(rx, epoch));
    let mut bytes = Vec::new();
    let mut aborted = false;
    let mut stream_error: Option<String> = None;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        // 被新语音打断：停止下载与播放，不落缓存。
        if playback_stale(epoch) {
            aborted = true;
            break;
        }
        match chunk {
            Ok(chunk) => {
                if tx.send(chunk.to_vec()).is_err() {
                    stream_error = Some("音频播放线程意外退出".into());
                    break;
                }
                bytes.extend_from_slice(&chunk);
            }
            Err(e) => {
                stream_error = Some(format!("GPT-SoVITS 流式传输中断：{e}"));
                break;
            }
        }
    }
    drop(tx);
    // 播放线程在通道关闭后播完剩余缓冲再退出
    player.await.map_err(|e| e.to_string())??;
    if aborted || playback_stale(epoch) {
        log_info!("gpt_sovits_tts: 播放被新语音打断，丢弃本次合成");
        return Ok(());
    }
    if let Some(error) = stream_error {
        return Err(error);
    }
    log_info!("gpt_sovits_tts: stream finished, {} bytes", bytes.len());
    if !bytes.starts_with(b"RIFF") {
        return Err("GPT-SoVITS 返回的不是 WAV".into());
    }
    normalize_streaming_wav_header(&mut bytes)?;
    std::fs::write(output, &bytes).map_err(|e| e.to_string())?;
    Ok(())
}

/// 消费 GPT-SoVITS 流式 WAV 字节流并实时播放（在阻塞线程中运行）。
/// 先解析 WAV 头拿采样率/声道，之后的裸 PCM 累积到预缓冲量才开始播，
/// 降低合成速度波动导致的断流卡顿。
fn play_pcm_stream(rx: std::sync::mpsc::Receiver<Vec<u8>>, epoch: u64) -> Result<(), String> {
    const PREBUFFER_MS: usize = 700;
    let mut header_buf: Vec<u8> = Vec::new();
    let mut format: Option<(u32, u16)> = None; // (采样率, 声道数)
    let mut prebuffer_bytes = 0usize;
    let mut pending_pcm: Vec<u8> = Vec::new();
    let mut output: Option<(rodio::OutputStream, std::sync::Arc<rodio::Sink>)> = None;

    for chunk in rx.iter() {
        // 被新语音打断：立即停止播放并退出。
        if playback_stale(epoch) {
            if let Some((_, sink)) = &output {
                sink.stop();
            }
            return Ok(());
        }
        if format.is_none() {
            header_buf.extend_from_slice(&chunk);
            match parse_wav_stream_header(&header_buf) {
                Some((pcm_offset, sample_rate, channels)) => {
                    format = Some((sample_rate, channels));
                    prebuffer_bytes =
                        sample_rate as usize * channels as usize * 2 * PREBUFFER_MS / 1000;
                    pending_pcm.extend_from_slice(&header_buf[pcm_offset..]);
                    header_buf.clear();
                }
                None => {
                    if header_buf.len() > 4096 {
                        return Err("GPT-SoVITS 流式响应缺少有效 WAV 头（或不是 16-bit PCM）".into());
                    }
                    continue;
                }
            }
        } else {
            pending_pcm.extend_from_slice(&chunk);
        }
        let (sample_rate, channels) = format.expect("格式已解析");
        if output.is_none() && pending_pcm.len() >= prebuffer_bytes.max(1) {
            let stream_output = create_stream_output()?;
            if !register_sink(epoch, &stream_output.1) {
                return Ok(()); // 注册前已被打断
            }
            output = Some(stream_output);
        }
        if let Some((_, sink)) = &output {
            flush_pcm(sink, &mut pending_pcm, sample_rate, channels);
        }
    }

    // 通道关闭（合成结束）：音频太短不足预缓冲量也要播
    let (sample_rate, channels) = format.ok_or("GPT-SoVITS 未返回音频数据")?;
    if output.is_none() {
        let stream_output = create_stream_output()?;
        if !register_sink(epoch, &stream_output.1) {
            return Ok(());
        }
        output = Some(stream_output);
    }
    if let Some((_, sink)) = &output {
        flush_pcm(sink, &mut pending_pcm, sample_rate, channels);
        // 等待播完，期间响应打断。
        while !sink.empty() {
            if playback_stale(epoch) {
                sink.stop();
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    unregister_sink(epoch);
    Ok(())
}

fn create_stream_output() -> Result<(rodio::OutputStream, std::sync::Arc<rodio::Sink>), String> {
    let (stream, handle) =
        rodio::OutputStream::try_default().map_err(|e| format!("打开音频输出失败：{e}"))?;
    let sink = rodio::Sink::try_new(&handle).map_err(|e| e.to_string())?;
    Ok((stream, std::sync::Arc::new(sink)))
}

/// 把 pending 中完整的 int16 样本推入 sink；保留末尾可能残缺的单个字节。
fn flush_pcm(sink: &rodio::Sink, pending: &mut Vec<u8>, sample_rate: u32, channels: u16) {
    let usable = pending.len() & !1;
    if usable == 0 {
        return;
    }
    let samples: Vec<i16> = pending
        .drain(..usable)
        .collect::<Vec<_>>()
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    sink.append(rodio::buffer::SamplesBuffer::new(
        channels,
        sample_rate,
        samples,
    ));
}

/// 解析流式 WAV 头部，返回 (PCM 起始偏移, 采样率, 声道数)。
/// 头部字节尚不完整时返回 None，调用方继续累积。
fn parse_wav_stream_header(bytes: &[u8]) -> Option<(usize, u32, u16)> {
    if bytes.len() < 12 || !bytes.starts_with(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return None;
    }
    let mut offset = 12usize;
    let mut channels = None;
    let mut sample_rate = None;
    let mut bits_per_sample = None;
    while offset + 8 <= bytes.len() {
        let chunk_id = &bytes[offset..offset + 4];
        let chunk_size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().ok()?) as usize;
        if chunk_id == b"fmt " {
            if offset + 8 + 16 > bytes.len() {
                return None;
            }
            let fmt = &bytes[offset + 8..];
            channels = Some(u16::from_le_bytes([fmt[2], fmt[3]]));
            sample_rate = Some(u32::from_le_bytes([fmt[4], fmt[5], fmt[6], fmt[7]]));
            bits_per_sample = Some(u16::from_le_bytes([fmt[14], fmt[15]]));
        } else if chunk_id == b"data" {
            if bits_per_sample != Some(16) {
                return None; // 非 16-bit PCM 不支持，上层按无效头部报错
            }
            return Some((offset + 8, sample_rate?, channels?));
        }
        offset = offset.checked_add(8 + chunk_size + (chunk_size & 1))?;
    }
    None
}

async fn api_tts(settings: &AppSettings, text: &str, output: &Path) -> Result<PathBuf, String> {
    if settings.tts_api_base_url.is_empty()
        || settings.tts_api_model.is_empty()
        || settings.tts_api_key.is_empty()
    {
        return Err("通用 TTS API 配置不完整".into());
    }
    let client = reqwest::Client::new();
    let (mut bytes, content_type) = if settings.tts_api_protocol == "openai" {
        let response = client.post(format!("{}/audio/speech", settings.tts_api_base_url)).bearer_auth(&settings.tts_api_key).json(&serde_json::json!({"model":settings.tts_api_model,"input":text,"voice":settings.tts_api_voice,"response_format":"wav"})).send().await.map_err(|e| format!("TTS 请求失败：{e}"))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!(
                "TTS 返回 {status}: {}",
                body.chars().take(240).collect::<String>()
            ));
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        (
            response.bytes().await.map_err(|e| e.to_string())?.to_vec(),
            content_type,
        )
    } else {
        let response = client.post(format!("{}/services/aigc/multimodal-generation/generation", settings.tts_api_base_url)).bearer_auth(&settings.tts_api_key).json(&serde_json::json!({"model":settings.tts_api_model,"input":{"text":text,"voice":settings.tts_api_voice,"language_type":settings.tts_api_language}})).send().await.map_err(|e| format!("千问 TTS 请求失败：{e}"))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!(
                "千问 TTS 返回 {status}: {}",
                body.chars().take(240).collect::<String>()
            ));
        }
        let value: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("千问 TTS 响应解析失败：{e}"))?;
        if let Some(data) = value
            .pointer("/output/audio/data")
            .and_then(|v| v.as_str())
            .filter(|data| !data.trim().is_empty())
        {
            (
                base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .map_err(|e| format!("音频 Base64 无效：{e}"))?,
                "audio/wav".to_string(),
            )
        } else if let Some(url) = value.pointer("/output/audio/url").and_then(|v| v.as_str()) {
            let response = client
                .get(url)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?;
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_string();
            (
                response.bytes().await.map_err(|e| e.to_string())?.to_vec(),
                content_type,
            )
        } else {
            return Err(format!(
                "千问 TTS 未返回音频：{}",
                value
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("未知响应")
            ));
        }
    };
    if bytes.is_empty() {
        return Err("TTS 返回了空音频，未写入缓存".into());
    }
    normalize_streaming_wav_header(&mut bytes)?;
    let extension = detect_audio_extension(&bytes, &content_type).ok_or_else(|| {
        let header = bytes
            .iter()
            .take(12)
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(" ");
        format!("TTS 返回的内容不是受支持的音频（Content-Type: {content_type}，文件头: {header}）")
    })?;
    let output = output.with_extension(extension);
    if let Err(error) = std::fs::write(&output, &bytes) {
        let _ = std::fs::remove_file(&output);
        return Err(error.to_string());
    }
    Ok(output)
}

fn detect_audio_extension(bytes: &[u8], content_type: &str) -> Option<&'static str> {
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
        Some("wav")
    } else if bytes.starts_with(b"ID3")
        || matches!(bytes.get(0..2), Some([0xFF, second]) if second & 0xE0 == 0xE0)
    {
        Some("mp3")
    } else if bytes.starts_with(b"OggS") {
        Some("ogg")
    } else if bytes.starts_with(b"fLaC") {
        Some("flac")
    } else if content_type.contains("wav") || content_type.contains("wave") {
        Some("wav")
    } else if content_type.contains("mpeg") || content_type.contains("mp3") {
        Some("mp3")
    } else if content_type.contains("ogg") {
        Some("ogg")
    } else if content_type.contains("flac") {
        Some("flac")
    } else {
        None
    }
}

/// 部分云 TTS 为便于流式传输，会把 RIFF/data 长度写成 0x7fffffff。
/// 保存为普通文件后必须改成真实长度，否则严格的 WAV 解码器会在文件末尾
/// 报 `end of stream`，即使 PCM 数据本身完整。
fn normalize_streaming_wav_header(bytes: &mut [u8]) -> Result<(), String> {
    if bytes.len() < 12 || !bytes.starts_with(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Ok(());
    }
    let riff_size = u32::try_from(bytes.len().saturating_sub(8))
        .map_err(|_| "WAV 文件过大，无法写入 RIFF 长度".to_string())?;
    bytes[4..8].copy_from_slice(&riff_size.to_le_bytes());

    let mut offset = 12usize;
    while offset + 8 <= bytes.len() {
        let chunk_id = &bytes[offset..offset + 4];
        if chunk_id == b"data" {
            let data_size = u32::try_from(bytes.len().saturating_sub(offset + 8))
                .map_err(|_| "WAV 数据过大，无法写入 data 长度".to_string())?;
            bytes[offset + 4..offset + 8].copy_from_slice(&data_size.to_le_bytes());
            return Ok(());
        }
        let chunk_size = u32::from_le_bytes(
            bytes[offset + 4..offset + 8]
                .try_into()
                .map_err(|_| "WAV chunk 长度无效".to_string())?,
        ) as usize;
        offset = offset
            .checked_add(8 + chunk_size + (chunk_size & 1))
            .ok_or("WAV chunk 偏移溢出")?;
    }
    Err("WAV 文件缺少 data chunk".into())
}

async fn translate(app: &AppHandle, settings: &AppSettings, text: &str) -> Result<String, String> {
    let model = app.state::<ModelConfig>();
    if model.api_key.is_empty() {
        return Err("翻译需要 DeepSeek API Key".into());
    }
    let memories = {
        let db = app.state::<DbState>();
        let conn = db.0.lock().await;
        crate::memory_store::list_active(&conn, 30)
            .map_err(|e| e.to_string())?
            .into_iter()
            .take(30)
            .map(|m| format!("- {}", m.text))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let prompt = format!("你是角色语音翻译器。角色人设：{}\n相关长期记忆：\n{}\n将下面文本翻译为 {}，保持角色口吻和原标点结构。如果已经是目标语言则原样返回。只输出 JSON：{{\"translated_text\":\"...\"}}\n文本：{}", settings.persona, memories, settings.vits_target_language, text);
    let response = reqwest::Client::new().post(format!("{}/chat/completions",model.base_url)).bearer_auth(&model.api_key).json(&serde_json::json!({"model":model.model,"messages":[{"role":"user","content":prompt}],"temperature":0.3,"response_format":{"type":"json_object"},"thinking":{"type":"disabled"}})).send().await.map_err(|e|format!("翻译请求失败：{e}"))?;
    if !response.status().is_success() {
        return Err(format!("翻译模型返回 {}", response.status()));
    }
    let value: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let content = value
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .ok_or("翻译没有返回文本")?;
    let start = content.find('{').ok_or("翻译响应缺少 JSON")?;
    let end = content.rfind('}').ok_or("翻译响应缺少 JSON")?;
    let parsed: serde_json::Value =
        serde_json::from_str(&content[start..=end]).map_err(|e| e.to_string())?;
    parsed
        .get("translated_text")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|v| !v.trim().is_empty())
        .ok_or("翻译结果为空".into())
}

async fn play(path: PathBuf, epoch: u64) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let (_stream, handle) =
            rodio::OutputStream::try_default().map_err(|e| format!("打开音频输出失败：{e}"))?;
        let sink = std::sync::Arc::new(rodio::Sink::try_new(&handle).map_err(|e| e.to_string())?);
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        sink.append(
            rodio::Decoder::new(StdBufReader::new(file))
                .map_err(|e| format!("音频解码失败：{e}"))?,
        );
        if !register_sink(epoch, &sink) {
            return Ok(()); // 开始前已被打断
        }
        // 等待播完，期间响应打断。
        while !sink.empty() {
            if playback_stale(epoch) {
                sink.stop();
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        unregister_sink(epoch);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::{detect_audio_extension, normalize_streaming_wav_header, parse_wav_stream_header};

    #[test]
    fn detects_common_audio_headers() {
        assert_eq!(detect_audio_extension(b"RIFF1234WAVEfmt ", ""), Some("wav"));
        assert_eq!(detect_audio_extension(b"ID3anything", ""), Some("mp3"));
        assert_eq!(detect_audio_extension(b"OggSanything", ""), Some("ogg"));
        assert_eq!(detect_audio_extension(b"fLaCanything", ""), Some("flac"));
    }

    #[test]
    fn rejects_empty_or_non_audio_response() {
        assert_eq!(detect_audio_extension(b"", ""), None);
        assert_eq!(
            detect_audio_extension(b"{\"error\":true}", "application/json"),
            None
        );
    }

    #[test]
    fn repairs_streaming_wav_placeholder_lengths() {
        let mut wav = Vec::from(&b"RIFF\xFF\xFF\xFF\x7FWAVEfmt \x10\0\0\0\x01\0\x01\0\xC0\x5D\0\0\x80\xBB\0\0\x02\0\x10\0data\xFF\xFF\xFF\x7F\x01\0\x02\0"[..]);
        normalize_streaming_wav_header(&mut wav).unwrap();
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()), 40);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 4);
    }

    // GPT-SoVITS api_v2.py 流式模式：Python wave 模块生成的标准 44 字节头
    // （32000Hz / 单声道 / 16-bit，data 长度为 0），后接裸 PCM chunk。
    #[test]
    fn parses_streaming_wav_header() {
        let header = b"RIFF\x24\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x00\x7D\0\0\0\xFA\0\0\x02\0\x10\0data\0\0\0\0";
        assert_eq!(parse_wav_stream_header(header), Some((44, 32000, 1)));
        // 头部不完整时返回 None
        assert_eq!(parse_wav_stream_header(&header[..20]), None);
        // 非 RIFF 返回 None
        assert_eq!(parse_wav_stream_header(b"not a wav file at all.."), None);
    }

    #[test]
    fn normalizes_gpt_sovits_streamed_wav() {
        let mut wav = Vec::from(
            &b"RIFF\x24\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x00\x7D\0\0\0\xFA\0\0\x02\0\x10\0data\0\0\0\0"[..],
        );
        wav.extend_from_slice(&[1, 0, 2, 0]);
        normalize_streaming_wav_header(&mut wav).unwrap();
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()), 40);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 4);
    }
}
