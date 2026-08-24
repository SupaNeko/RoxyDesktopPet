//! 只读媒体感知：通过 Windows SMTC（GlobalSystemMediaTransportControls）读取当前播放曲目。
//! QQ音乐 20.22+ 原生支持（需在客户端设置中开启 SMTC），网易云 v3.x 有基础支持。
//! 需要在设置中授权「当前播放」后才会被调用；只读取，不做任何控制。

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub source_app: String,
    pub playing: bool,
}

/// 返回当前媒体会话中正在播放（优先）或最近一条有标题的曲目；没有任何会话时返回 Ok(None)。
///
/// WinRT 会话对象不是 Send，因此整个查询放到专用线程 + current_thread runtime 里执行，
/// 通过 oneshot 通道把纯数据结果传回。
pub async fn now_playing() -> Result<Option<NowPlaying>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = sender.send(query_now_playing());
    });
    receiver
        .await
        .map_err(|_| "媒体查询线程异常结束".to_string())?
}

#[cfg(windows)]
fn query_now_playing() -> Result<Option<NowPlaying>, String> {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    // 线程首次使用 COM 前需要初始化；已被初始化时返回错误，忽略即可。
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("创建媒体查询运行时失败：{e}"))?;
    runtime.block_on(query_now_playing_inner())
}

#[cfg(windows)]
async fn query_now_playing_inner() -> Result<Option<NowPlaying>, String> {
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus,
    };

    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
        .map_err(|e| format!("SMTC 初始化失败：{e}"))?
        .await
        .map_err(|e| format!("SMTC 初始化失败：{e}"))?;
    let sessions = manager
        .GetSessions()
        .map_err(|e| format!("枚举媒体会话失败：{e}"))?;
    let mut fallback: Option<NowPlaying> = None;
    for session in sessions {
        let source = session
            .SourceAppUserModelId()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let playing = session
            .GetPlaybackInfo()
            .and_then(|info| info.PlaybackStatus())
            .map(|status| {
                status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing
            })
            .unwrap_or(false);
        let Ok(operation) = session.TryGetMediaPropertiesAsync() else {
            continue;
        };
        let Ok(props) = operation.await else {
            continue;
        };
        let entry = NowPlaying {
            title: props.Title().map(|s| s.to_string()).unwrap_or_default(),
            artist: props.Artist().map(|s| s.to_string()).unwrap_or_default(),
            album: props
                .AlbumTitle()
                .map(|s| s.to_string())
                .unwrap_or_default(),
            source_app: source,
            playing,
        };
        if entry.playing && !entry.title.is_empty() {
            return Ok(Some(entry));
        }
        if fallback.is_none() && !entry.title.is_empty() {
            fallback = Some(entry);
        }
    }
    Ok(fallback)
}

#[cfg(not(windows))]
fn query_now_playing() -> Result<Option<NowPlaying>, String> {
    Ok(None)
}
