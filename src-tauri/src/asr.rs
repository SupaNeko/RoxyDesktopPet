use base64::{engine::general_purpose::STANDARD, Engine};
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

#[derive(Clone)]
pub struct XfyunCredentials {
    pub app_id: String,
    pub api_key: String,
    pub api_secret: String,
}

impl XfyunCredentials {
    pub fn is_complete(&self) -> bool {
        !self.app_id.trim().is_empty()
            && !self.api_key.trim().is_empty()
            && !self.api_secret.trim().is_empty()
    }
}

fn auth_url(credentials: &XfyunCredentials) -> Result<Url, String> {
    let host = "iat-api.xfyun.cn";
    let path = "/v2/iat";
    let date = chrono::Utc::now()
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string();
    let origin = format!("host: {host}\ndate: {date}\nGET {path} HTTP/1.1");
    let mut mac = Hmac::<Sha256>::new_from_slice(credentials.api_secret.as_bytes())
        .map_err(|_| "讯飞 APISecret 无效".to_string())?;
    mac.update(origin.as_bytes());
    let signature = STANDARD.encode(mac.finalize().into_bytes());
    let authorization = STANDARD.encode(format!(
        "api_key=\"{}\", algorithm=\"hmac-sha256\", headers=\"host date request-line\", signature=\"{}\"",
        credentials.api_key, signature
    ));
    let mut url =
        Url::parse("wss://iat-api.xfyun.cn/v2/iat").map_err(|e| format!("讯飞地址无效：{e}"))?;
    url.query_pairs_mut()
        .append_pair("authorization", &authorization)
        .append_pair("date", &date)
        .append_pair("host", host);
    Ok(url)
}

pub async fn transcribe(
    samples: Vec<i16>,
    credentials: XfyunCredentials,
) -> Result<String, String> {
    if !credentials.is_complete() {
        return Err("尚未配置完整的讯飞 ASR 凭据".into());
    }
    let url = auth_url(&credentials)?;
    let (socket, _) = connect_async(url.as_str())
        .await
        .map_err(|e| format!("连接讯飞 ASR 失败：{e}"))?;
    // WebSocket 会话建立即计为一次 ASR 调用。
    crate::usage::record(crate::usage::CAT_ASR, 0, 0).await;
    let (mut write, mut read) = socket.split();
    let pcm: Vec<u8> = samples.into_iter().flat_map(i16::to_le_bytes).collect();
    let chunks: Vec<&[u8]> = pcm.chunks(1280).collect();
    for (index, chunk) in chunks.iter().enumerate() {
        let status = if index == 0 { 0 } else { 1 };
        let mut payload = serde_json::json!({
            "data": {"status": status, "format": "audio/L16;rate=16000", "encoding": "raw", "audio": STANDARD.encode(chunk)}
        });
        if index == 0 {
            payload["common"] = serde_json::json!({"app_id": credentials.app_id});
            payload["business"] = serde_json::json!({
                "language": "zh_cn", "domain": "iat", "accent": "mandarin", "ptt": 1, "rlang": "zh-cn"
            });
        }
        write
            .send(Message::Text(payload.to_string().into()))
            .await
            .map_err(|e| format!("上传语音失败：{e}"))?;
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    write.send(Message::Text(serde_json::json!({
        "data": {"status": 2, "format": "audio/L16;rate=16000", "encoding": "raw", "audio": ""}
    }).to_string().into())).await.map_err(|e| format!("结束语音上传失败：{e}"))?;

    let mut text = String::new();
    while let Some(message) = read.next().await {
        let message = message.map_err(|e| format!("接收讯飞结果失败：{e}"))?;
        let Message::Text(raw) = message else {
            continue;
        };
        let value: Value =
            serde_json::from_str(&raw).map_err(|e| format!("讯飞结果格式错误：{e}"))?;
        let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
        if code != 0 {
            let message = value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("未知错误");
            return Err(format!("讯飞 ASR 错误 {code}：{message}"));
        }
        if let Some(words) = value.pointer("/data/result/ws").and_then(Value::as_array) {
            for word in words {
                if let Some(value) = word.pointer("/cw/0/w").and_then(Value::as_str) {
                    text.push_str(value);
                }
            }
        }
        if value.pointer("/data/status").and_then(Value::as_i64) == Some(2) {
            break;
        }
    }
    let text = text.trim().to_string();
    if text.is_empty() {
        Err("讯飞没有识别出文字".into())
    } else {
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auth_url_contains_required_fields_without_exposing_secret() {
        let credentials = XfyunCredentials {
            app_id: "app".into(),
            api_key: "key".into(),
            api_secret: "secret".into(),
        };
        let url = auth_url(&credentials).unwrap().to_string();
        assert!(url.contains("authorization="));
        assert!(url.contains("date="));
        assert!(!url.contains("secret"));
    }

    #[tokio::test]
    #[ignore = "requires real XFYUN credentials and network"]
    async fn live_credentials_can_reach_xfyun() {
        let credentials = XfyunCredentials {
            app_id: std::env::var("XFYUN_ASR_APP_ID").unwrap(),
            api_key: std::env::var("XFYUN_ASR_API_KEY").unwrap(),
            api_secret: std::env::var("XFYUN_ASR_API_SECRET").unwrap(),
        };
        let result = transcribe(vec![0; 16_000], credentials).await;
        assert!(result.is_ok() || result.unwrap_err().contains("没有识别出文字"));
    }
}
