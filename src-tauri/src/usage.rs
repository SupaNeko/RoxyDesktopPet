//! API 消耗统计埋点：各调用点在请求成功后记录调用次数与 token 用量。
//!
//! 通过全局 AppHandle 访问数据库，避免把句柄逐层穿进各 LLM 调用函数；
//! 记录失败只写日志，绝不影响主对话链路。未初始化（如单元测试）时静默跳过。

use std::sync::OnceLock;

use tauri::{AppHandle, Manager};

use crate::db::{self, DbState};

/// 语音转文字（讯飞 ASR）
pub const CAT_ASR: &str = "asr";
/// 主对话（含对话后工具审计）
pub const CAT_CHAT: &str = "chat";
/// 旁路任务代理
pub const CAT_AGENT: &str = "agent";
/// 观察者（长期记忆 + 用户偏好总结）
pub const CAT_OBSERVER: &str = "observer";

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

pub fn init(app: &AppHandle) {
    let _ = APP_HANDLE.set(app.clone());
}

/// 记录一次 API 调用；无 token 概念的调用（如 ASR）传 0。
pub(crate) async fn record(category: &'static str, prompt_tokens: i64, completion_tokens: i64) {
    let Some(app) = APP_HANDLE.get() else {
        return;
    };
    let state = app.state::<DbState>();
    let conn = state.0.lock().await;
    if let Err(error) = db::insert_usage_event(&conn, category, prompt_tokens, completion_tokens) {
        log_warn!("记录 API 消耗失败（{category}）：{error}");
    }
}

/// 从 Chat Completions 响应 JSON 提取 token 用量；缺 usage 字段时返回 (0, 0)，
/// 调用方仍应记录这次调用（次数照计，token 为 0）。
pub(crate) fn extract_usage(value: &serde_json::Value) -> (i64, i64) {
    let usage = value.pointer("/usage");
    let prompt = usage
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let completion = usage
        .and_then(|u| u.get("completion_tokens"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    (prompt, completion)
}

/// 记录一次 Chat Completions 调用（裸 serde_json::Value 响应的调用点使用）。
pub(crate) async fn record_chat_response(category: &'static str, value: &serde_json::Value) {
    let (prompt, completion) = extract_usage(value);
    record(category, prompt, completion).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_usage_parses_standard_response() {
        let value = serde_json::json!({
            "choices": [],
            "usage": {"prompt_tokens": 123, "completion_tokens": 45, "total_tokens": 168}
        });
        assert_eq!(extract_usage(&value), (123, 45));
    }

    #[test]
    fn extract_usage_defaults_to_zero_when_missing() {
        // 完全缺少 usage 字段
        assert_eq!(extract_usage(&serde_json::json!({"choices": []})), (0, 0));
        // usage 存在但缺个别字段
        assert_eq!(
            extract_usage(&serde_json::json!({"usage": {"prompt_tokens": 7}})),
            (7, 0)
        );
        // 字段类型异常
        assert_eq!(
            extract_usage(&serde_json::json!({"usage": {"prompt_tokens": "abc"}})),
            (0, 0)
        );
    }
}
