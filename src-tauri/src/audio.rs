use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex as StdMutex,
    },
    thread,
};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use voice_activity_detector::VoiceActivityDetector;

const TARGET_RATE: u32 = 16_000;
const FRAME_SIZE: usize = 512;
const PRE_ROLL_FRAMES: usize = 10; // 320 ms
const START_FRAMES: usize = 6; // 192 ms
const END_FRAMES: usize = 29; // 928 ms
const MIN_SPEECH_FRAMES: usize = 13; // 416 ms
const MAX_FRAMES: usize = 938; // about 30 s
const START_THRESHOLD: f32 = 0.62;
const KEEP_THRESHOLD: f32 = 0.42;

pub struct VoiceState {
    pub active: Arc<AtomicBool>,
    pub stream: Mutex<Option<cpal::Stream>>,
    pub completed: Arc<StdMutex<VecDeque<Utterance>>>,
    pub push_samples: Arc<StdMutex<Vec<i16>>>,
}

impl Default for VoiceState {
    fn default() -> Self {
        Self {
            active: Arc::new(AtomicBool::new(false)),
            stream: Mutex::new(None),
            completed: Arc::new(StdMutex::new(VecDeque::new())),
            push_samples: Arc::new(StdMutex::new(Vec::new())),
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // consumed by the ASR provider in the next integration step
pub struct Utterance {
    pub id: String,
    pub samples: Vec<i16>,
    pub sample_rate: u32,
    pub forced_end: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct VoiceStatus {
    pub state: String,
    pub detail: Option<String>,
    pub duration_ms: Option<u64>,
}

pub fn input_devices() -> Result<Vec<String>, String> {
    let host = cpal::default_host();
    let mut names: Vec<String> = host
        .input_devices()
        .map_err(|e| format!("无法枚举麦克风：{e}"))?
        .filter_map(|device| device.name().ok())
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}

struct Segmenter {
    pre_roll: VecDeque<Vec<i16>>,
    current: Vec<i16>,
    speaking: bool,
    start_count: usize,
    silence_count: usize,
    speech_frames: usize,
}

impl Segmenter {
    fn new() -> Self {
        Self {
            pre_roll: VecDeque::with_capacity(PRE_ROLL_FRAMES),
            current: vec![],
            speaking: false,
            start_count: 0,
            silence_count: 0,
            speech_frames: 0,
        }
    }
    fn push(&mut self, frame: Vec<i16>, probability: f32) -> Option<(Vec<i16>, bool)> {
        if !self.speaking {
            if self.pre_roll.len() == PRE_ROLL_FRAMES {
                self.pre_roll.pop_front();
            }
            self.pre_roll.push_back(frame.clone());
            self.start_count = if probability >= START_THRESHOLD {
                self.start_count + 1
            } else {
                0
            };
            if self.start_count >= START_FRAMES {
                self.speaking = true;
                self.speech_frames = self.start_count;
                self.current = self.pre_roll.drain(..).flatten().collect();
                self.silence_count = 0;
            }
            return None;
        }
        self.current.extend_from_slice(&frame);
        if probability >= KEEP_THRESHOLD {
            self.speech_frames += 1;
            self.silence_count = 0;
        } else {
            self.silence_count += 1;
        }
        let forced = self.current.len() / FRAME_SIZE >= MAX_FRAMES;
        if self.silence_count >= END_FRAMES || forced {
            let valid = self.speech_frames >= MIN_SPEECH_FRAMES;
            let audio = std::mem::take(&mut self.current);
            self.speaking = false;
            self.start_count = 0;
            self.silence_count = 0;
            self.speech_frames = 0;
            self.pre_roll.clear();
            if valid {
                return Some((audio, forced));
            }
        }
        None
    }
}

fn emit(app: &AppHandle, state: &str, detail: Option<String>, duration_ms: Option<u64>) {
    let _ = app.emit(
        "voice-status",
        VoiceStatus {
            state: state.into(),
            detail,
            duration_ms,
        },
    );
}

pub async fn start(
    app: AppHandle,
    state: &VoiceState,
    selected_device_name: Option<&str>,
    asr_credentials: crate::asr::XfyunCredentials,
) -> Result<(), String> {
    if state.active.load(Ordering::SeqCst) {
        return Ok(());
    }
    let host = cpal::default_host();
    let device = if let Some(selected) = selected_device_name.filter(|name| !name.trim().is_empty())
    {
        host.input_devices()
            .map_err(|e| format!("无法枚举麦克风：{e}"))?
            .find(|device| device.name().ok().as_deref() == Some(selected))
            .ok_or_else(|| format!("已选择的麦克风不存在或已断开：{selected}"))?
    } else {
        return Err("尚未选择麦克风，请先在设置中选择输入设备".into());
    };
    let device_name = device.name().unwrap_or_else(|_| "默认麦克风".into());
    let config = device
        .default_input_config()
        .map_err(|e| format!("无法读取麦克风配置：{e}"))?;
    log_info!(
        "audio::start: device={device_name}, rate={}, channels={}",
        config.sample_rate().0,
        config.channels()
    );
    let source_rate = config.sample_rate().0;
    let device_name_log = device_name.clone();
    let channels = config.channels() as usize;
    if state.active.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<f32>>(64);
    state.active.store(true, Ordering::SeqCst);
    let active = state.active.clone();
    let completed = state.completed.clone();
    let worker_app = app.clone();
    let processing = Arc::new(AtomicBool::new(false));
    thread::Builder::new()
        .name("chatpet-vad".into())
        .spawn(move || {
            let mut vad = match VoiceActivityDetector::builder()
                .sample_rate(TARGET_RATE as i64)
                .chunk_size(FRAME_SIZE)
                .build()
            {
                Ok(v) => v,
                Err(e) => {
                    active.store(false, Ordering::SeqCst);
                    log_error!("Silero VAD 初始化失败：{e}");
                    emit(
                        &worker_app,
                        "error",
                        Some(format!("Silero VAD 初始化失败：{e}")),
                        None,
                    );
                    return;
                }
            };
            let mut segmenter = Segmenter::new();
            let mut pending = Vec::<i16>::new();
            let mut phase = 0u64;
            emit(&worker_app, "listening", Some(device_name), None);
            while active.load(Ordering::SeqCst) {
                let chunk = match rx.recv_timeout(std::time::Duration::from_millis(200)) {
                    Ok(c) => c,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(_) => break,
                };
                for sample in chunk {
                    phase += TARGET_RATE as u64;
                    if phase >= source_rate as u64 {
                        phase -= source_rate as u64;
                        pending.push((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
                    }
                }
                while pending.len() >= FRAME_SIZE {
                    let frame: Vec<i16> = pending.drain(..FRAME_SIZE).collect();
                    let probability = vad.predict(frame.clone());
                    if let Some((samples, forced)) = segmenter.push(frame, probability) {
                        let duration_ms = samples.len() as u64 * 1000 / TARGET_RATE as u64;
                        let utterance = Utterance {
                            id: uuid::Uuid::new_v4().to_string(),
                            samples,
                            sample_rate: TARGET_RATE,
                            forced_end: forced,
                        };
                        if let Ok(mut queue) = completed.lock() {
                            queue.push_back(utterance.clone());
                            while queue.len() > 3 {
                                queue.pop_front();
                            }
                        }
                        emit(
                            &worker_app,
                            "recognizing",
                            Some("正在识别语音…".into()),
                            Some(duration_ms),
                        );
                        if processing
                            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                            .is_err()
                        {
                            emit(
                                &worker_app,
                                "error",
                                Some("上一句仍在处理中，请稍后再说".into()),
                                None,
                            );
                            continue;
                        }
                        let task_app = worker_app.clone();
                        let task_active = active.clone();
                        let task_processing = processing.clone();
                        let task_credentials = asr_credentials.clone();
                        tauri::async_runtime::spawn(async move {
                            match crate::asr::transcribe(utterance.samples, task_credentials).await
                            {
                                Ok(text) => {
                                    let _ = task_app.emit("voice-transcript", &text);
                                    emit(
                                        &task_app,
                                        "thinking",
                                        Some(text.clone()),
                                        Some(duration_ms),
                                    );
                                    match crate::commands::process_voice_text(
                                        task_app.clone(),
                                        text,
                                    )
                                    .await
                                    {
                                        Ok(message) => {
                                            let _ = task_app.emit("assistant-message", message);
                                        }
                                        Err(error) => emit(&task_app, "error", Some(error), None),
                                    }
                                }
                                Err(error) => {
                                    emit(&task_app, "error", Some(error), Some(duration_ms))
                                }
                            }
                            task_processing.store(false, Ordering::SeqCst);
                            if task_active.load(Ordering::SeqCst) {
                                emit(&task_app, "listening", Some("等待下一句话".into()), None);
                            }
                        });
                    } else if segmenter.speaking {
                        emit(&worker_app, "speaking", None, None);
                    }
                }
            }
            emit(&worker_app, "disabled", None, None);
        })
        .map_err(|e| format!("无法启动 VAD 线程：{e}"))?;

    let stream_config: cpal::StreamConfig = config.clone().into();
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            let sender = tx.clone();
            let err_app = app.clone();
            let err_active = state.active.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    let _ = sender.try_send(to_mono_f32(data, channels));
                },
                move |e| {
                    err_active.store(false, Ordering::SeqCst);
                    log_error!("麦克风流错误：{e}");
                    emit(&err_app, "error", Some(format!("麦克风流错误：{e}")), None);
                },
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let sender = tx.clone();
            let err_app = app.clone();
            let err_active = state.active.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    let _ = sender.try_send(to_mono_i16(data, channels));
                },
                move |e| {
                    err_active.store(false, Ordering::SeqCst);
                    log_error!("麦克风流错误：{e}");
                    emit(&err_app, "error", Some(format!("麦克风流错误：{e}")), None);
                },
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let sender = tx.clone();
            let err_app = app.clone();
            let err_active = state.active.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| {
                    let _ = sender.try_send(to_mono_u16(data, channels));
                },
                move |e| {
                    err_active.store(false, Ordering::SeqCst);
                    log_error!("麦克风流错误：{e}");
                    emit(&err_app, "error", Some(format!("麦克风流错误：{e}")), None);
                },
                None,
            )
        }
        _ => {
            state.active.store(false, Ordering::SeqCst);
            return Err(format!(
                "当前麦克风格式 {:?} 尚不支持，请在 Windows 声音设置中选择浮点格式设备",
                config.sample_format()
            ));
        }
    }
    .map_err(|e| {
        state.active.store(false, Ordering::SeqCst);
        format!("无法打开麦克风：{e}")
    })?;
    stream.play().map_err(|e| {
        state.active.store(false, Ordering::SeqCst);
        format!("无法启动麦克风：{e}")
    })?;
    *state.stream.lock().await = Some(stream);
    log_info!("audio::start: microphone stream started ({device_name_log})");
    Ok(())
}

fn to_mono_f32(data: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return data.to_vec();
    }
    data.chunks(channels)
        .map(|f| f.iter().sum::<f32>() / channels as f32)
        .collect()
}
fn to_mono_i16(data: &[i16], channels: usize) -> Vec<f32> {
    data.chunks(channels.max(1))
        .map(|f| f.iter().map(|&s| s as f32 / i16::MAX as f32).sum::<f32>() / f.len() as f32)
        .collect()
}
fn to_mono_u16(data: &[u16], channels: usize) -> Vec<f32> {
    data.chunks(channels.max(1))
        .map(|f| {
            f.iter()
                .map(|&s| (s as f32 / u16::MAX as f32) * 2.0 - 1.0)
                .sum::<f32>()
                / f.len() as f32
        })
        .collect()
}

pub async fn start_push_to_talk(
    app: AppHandle,
    state: &VoiceState,
    selected_device_name: Option<&str>,
) -> Result<(), String> {
    let host = cpal::default_host();
    let selected = selected_device_name
        .filter(|name| !name.trim().is_empty())
        .ok_or("尚未选择麦克风，请先在设置中选择输入设备")?;
    let device = host
        .input_devices()
        .map_err(|e| format!("无法枚举麦克风：{e}"))?
        .find(|device| device.name().ok().as_deref() == Some(selected))
        .ok_or_else(|| format!("已选择的麦克风不存在或已断开：{selected}"))?;
    let config = device
        .default_input_config()
        .map_err(|e| format!("无法读取麦克风配置：{e}"))?;
    let source_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    if state.active.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    if let Ok(mut samples) = state.push_samples.lock() {
        samples.clear();
    }
    let stream_config: cpal::StreamConfig = config.clone().into();
    let build = |samples: Arc<StdMutex<Vec<i16>>>, mut phase: u64| {
        move |mono: Vec<f32>| {
            if let Ok(mut output) = samples.lock() {
                for sample in mono {
                    phase += TARGET_RATE as u64;
                    if phase >= source_rate as u64 {
                        phase -= source_rate as u64;
                        output.push((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
                    }
                }
            }
        }
    };
    let err_app = app.clone();
    let err_active = state.active.clone();
    let error = move |e| {
        err_active.store(false, Ordering::SeqCst);
        emit(&err_app, "error", Some(format!("麦克风流错误：{e}")), None);
    };
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            let mut append = build(state.push_samples.clone(), 0);
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| append(to_mono_f32(data, channels)),
                error,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let mut append = build(state.push_samples.clone(), 0);
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| append(to_mono_i16(data, channels)),
                error,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let mut append = build(state.push_samples.clone(), 0);
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| append(to_mono_u16(data, channels)),
                error,
                None,
            )
        }
        _ => {
            state.active.store(false, Ordering::SeqCst);
            return Err(format!(
                "当前麦克风格式 {:?} 尚不支持",
                config.sample_format()
            ));
        }
    }
    .map_err(|e| format!("无法打开麦克风：{e}"))?;
    stream.play().map_err(|e| format!("无法启动麦克风：{e}"))?;
    *state.stream.lock().await = Some(stream);
    emit(&app, "speaking", Some("按键录音中".into()), None);
    Ok(())
}

pub async fn finish_push_to_talk(
    app: AppHandle,
    state: &VoiceState,
    credentials: crate::asr::XfyunCredentials,
) -> Result<(), String> {
    if !state.active.swap(false, Ordering::SeqCst) {
        return Ok(());
    }
    *state.stream.lock().await = None;
    let samples = state
        .push_samples
        .lock()
        .map(|mut samples| std::mem::take(&mut *samples))
        .map_err(|_| "无法读取按键录音")?;
    let duration_ms = samples.len() as u64 * 1000 / TARGET_RATE as u64;
    if duration_ms < 200 {
        emit(
            &app,
            "disabled",
            Some("录音时间太短".into()),
            Some(duration_ms),
        );
        return Ok(());
    }
    emit(
        &app,
        "recognizing",
        Some("正在识别完整录音…".into()),
        Some(duration_ms),
    );
    let text = crate::asr::transcribe(samples, credentials).await?;
    let _ = app.emit("voice-transcript", &text);
    emit(&app, "thinking", Some(text.clone()), Some(duration_ms));
    let message = crate::commands::process_voice_text(app.clone(), text).await?;
    let _ = app.emit("assistant-message", message);
    emit(&app, "disabled", None, None);
    Ok(())
}
pub async fn stop(state: &VoiceState) {
    log_info!("audio::stop called");
    state.active.store(false, Ordering::SeqCst);
    *state.stream.lock().await = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silero_model_initializes() {
        VoiceActivityDetector::builder()
            .sample_rate(TARGET_RATE as i64)
            .chunk_size(FRAME_SIZE)
            .build()
            .expect("Silero VAD should initialize");
    }
    #[test]
    fn short_noise_is_discarded() {
        let mut s = Segmenter::new();
        for _ in 0..START_FRAMES {
            s.push(vec![1; FRAME_SIZE], 0.9);
        }
        for _ in 0..END_FRAMES {
            assert!(s.push(vec![0; FRAME_SIZE], 0.0).is_none());
        }
    }
    #[test]
    fn valid_utterance_completes() {
        let mut s = Segmenter::new();
        for _ in 0..(MIN_SPEECH_FRAMES + 2) {
            s.push(vec![1; FRAME_SIZE], 0.9);
        }
        let mut out = None;
        for _ in 0..END_FRAMES {
            out = s.push(vec![0; FRAME_SIZE], 0.0).or(out);
        }
        assert!(out.is_some());
    }
}
