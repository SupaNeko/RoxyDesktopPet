use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 64 * 1024;

pub type EventHandler = Arc<dyn Fn(String, String) + Send + Sync>;

/// 绑定 127.0.0.1:<port>，失败返回可读错误。
pub async fn bind(port: u32) -> Result<TcpListener, String> {
    let addr = format!("127.0.0.1:{port}");
    TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("无法监听 {addr}：{e}"))
}

/// 在 tokio runtime 上跑 accept 循环。handler 收到 (tool, body)。
pub fn spawn(
    listener: TcpListener,
    token: String,
    token_enabled: bool,
    handler: EventHandler,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _peer)) => {
                    let token = token.clone();
                    let handler = handler.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = handle_connection(stream, &token, token_enabled, &handler).await;
                    });
                }
                Err(_) => {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            }
        }
    })
}

async fn handle_connection(
    mut stream: TcpStream,
    token: &str,
    token_enabled: bool,
    handler: &EventHandler,
) -> Result<(), ()> {
    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 1024];
    let header_end;
    loop {
        let n = stream.read(&mut tmp).await.map_err(|_| ())?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
            header_end = pos + 4;
            break;
        }
        if buf.len() > MAX_HEADER_BYTES {
            return respond(&mut stream, 431, "request header too large").await;
        }
    }

    let header_text = String::from_utf8_lossy(&buf[..header_end]);
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split(' ');
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");

    let mut content_length = 0usize;
    let mut req_token = String::new();
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            match key.trim().to_ascii_lowercase().as_str() {
                "content-length" => content_length = value.trim().parse().unwrap_or(0),
                "x-chatpet-token" => req_token = value.trim().to_string(),
                _ => {}
            }
        }
    }

    if content_length > MAX_BODY_BYTES {
        return respond(&mut stream, 413, "body too large").await;
    }

    let mut body = buf[header_end..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut tmp).await.map_err(|_| ())?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(content_length);

    if path == "/health" {
        return respond(&mut stream, 200, "{\"ok\":true}").await;
    }
    if method != "POST" {
        return respond(&mut stream, 405, "method not allowed").await;
    }
    let Some(tool) = path.strip_prefix("/hook/") else {
        return respond(&mut stream, 404, "not found").await;
    };
    if tool.is_empty() || tool.contains('/') {
        return respond(&mut stream, 404, "not found").await;
    }
    if token_enabled && !constant_time_eq(req_token.as_bytes(), token.as_bytes()) {
        return respond(&mut stream, 401, "unauthorized").await;
    }

    let body_str = String::from_utf8_lossy(&body).to_string();
    handler(tool.to_string(), body_str);
    respond(&mut stream, 200, "{\"ok\":true}").await
}

async fn respond(stream: &mut TcpStream, status: u16, body: &str) -> Result<(), ()> {
    let reason = match status {
        200 => "OK",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        431 => "Request Header Fields Too Large",
        _ => "Error",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await.map_err(|_| ())?;
    let _ = stream.shutdown().await;
    Ok(())
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::{constant_time_eq, find_subsequence};

    #[test]
    fn finds_header_terminator() {
        let buf = b"POST /hook/opencode HTTP/1.1\r\nHost: x\r\n\r\nbody";
        assert_eq!(find_subsequence(buf, b"\r\n\r\n"), Some(37));
        assert_eq!(find_subsequence(b"no terminator", b"\r\n\r\n"), None);
    }

    #[test]
    fn token_compare_is_constant_time_and_exact() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}
