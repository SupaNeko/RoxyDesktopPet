//! MCP（Model Context Protocol）客户端管理。
//!
//! - stdio（本地命令）：跟随应用启动拉起子进程；运行期新增/修改标记 stale，
//!   重启后生效；启用开关只控制工具注入，不杀进程。
//! - remote（Streamable HTTP）：动态加载，启用即连接、停用即断开。
//! - 工具名统一加前缀 `mcp__{server}__{tool}` 注入 AI，防多服务器重名。

use crate::db::McpServer;
use rmcp::{
    RoleClient, ServiceExt,
    model::Tool,
    service::{Peer, RunningService},
    transport::{ConfigureCommandExt, StreamableHttpClientTransport, TokioChildProcess},
};
use serde::Serialize;
use std::{collections::HashMap, time::Duration};
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex;

const CONNECT_TIMEOUT_STDIO: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT_REMOTE: Duration = Duration::from_secs(15);
const CALL_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Default)]
pub struct McpState {
    inner: Mutex<HashMap<i64, ServerRuntime>>,
}

struct ServerRuntime {
    /// 保持连接存活的句柄；cancel/drop 会断开（stdio 会杀掉子进程）。
    service: Option<RunningService<RoleClient, ()>>,
    peer: Option<Peer<RoleClient>>,
    tools: Vec<Tool>,
    name: String,
    enabled: bool,
    /// 配置在运行期被修改，重启后才会按新配置重连。
    stale: bool,
    error: Option<String>,
}

impl ServerRuntime {
    fn new(name: String, enabled: bool) -> Self {
        Self {
            service: None,
            peer: None,
            tools: Vec::new(),
            name,
            enabled,
            stale: false,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct McpServerStatus {
    #[serde(flatten)]
    pub server: McpServer,
    /// "disabled" | "connected" | "pending_restart" | "error"
    pub status: String,
    pub tool_count: usize,
    pub requires_restart: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct McpToolEntry {
    pub prefixed_name: String,
    pub server_name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

impl McpToolEntry {
    /// OpenAI chat.completions 的 tools 元素。
    pub fn to_openai_tool(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": self.prefixed_name,
                "description": self.description,
                "parameters": self.input_schema,
            }
        })
    }
}

/// 工具名清洗为 OpenAI 允许的 [a-zA-Z0-9_-]，并限制长度。
fn sanitize(value: &str, max: usize) -> String {
    let mut out: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    out.truncate(max);
    if out.is_empty() {
        out.push('x');
    }
    out
}

fn prefixed_tool_name(server_name: &str, tool_name: &str) -> String {
    let mut name = format!(
        "mcp__{}__{}",
        sanitize(&server_name.to_lowercase(), 20),
        sanitize(tool_name, 38)
    );
    name.truncate(64);
    name
}

/// 参数解析：优先按 JSON 数组（JSON 导入配置保留含空格参数），否则按空白切分。
fn parse_args(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.starts_with('[') {
        if let Ok(values) = serde_json::from_str::<Vec<serde_json::Value>>(trimmed) {
            return values
                .into_iter()
                .map(|v| match v {
                    serde_json::Value::String(s) => s,
                    other => other.to_string(),
                })
                .collect();
        }
    }
    trimmed.split_whitespace().map(|s| s.to_string()).collect()
}

fn parse_env(raw: &str) -> Vec<(String, String)> {
    raw.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

async fn connect(
    server: &McpServer,
) -> Result<(RunningService<RoleClient, ()>, Vec<Tool>), String> {
    match server.transport.as_str() {
        "stdio" => {
            if server.command.trim().is_empty() {
                return Err("本地 MCP 缺少启动命令".into());
            }
            let mut command = tokio::process::Command::new(server.command.trim());
            command
                .args(parse_args(&server.args))
                .envs(parse_env(&server.env));
            // CREATE_NO_WINDOW：避免每个 MCP 子进程弹出控制台窗口。
            #[cfg(windows)]
            command.creation_flags(0x08000000);
            let connect = async {
                let transport = TokioChildProcess::new(command.configure(|_| {}))
                    .map_err(|e| format!("启动 MCP 子进程失败：{e}"))?;
                let service = ()
                    .serve(transport)
                    .await
                    .map_err(|e| format!("MCP 初始化握手失败：{e}"))?;
                let tools = service
                    .list_all_tools()
                    .await
                    .map_err(|e| format!("获取 MCP 工具列表失败：{e}"))?;
                Ok::<(RunningService<RoleClient, ()>, Vec<Tool>), String>((service, tools))
            };
            tokio::time::timeout(CONNECT_TIMEOUT_STDIO, connect)
                .await
                .map_err(|_| "连接本地 MCP 超时".to_string())?
        }
        "remote" => {
            if !(server.url.starts_with("http://") || server.url.starts_with("https://")) {
                return Err("远程 MCP 地址必须以 http:// 或 https:// 开头".into());
            }
            let mut custom_headers = std::collections::HashMap::new();
            for (name, value) in parse_env(&server.headers) {
                let header_name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
                    .map_err(|e| format!("请求头名称无效「{name}」：{e}"))?;
                let header_value = reqwest::header::HeaderValue::from_str(&value)
                    .map_err(|e| format!("请求头「{name}」的值无效：{e}"))?;
                custom_headers.insert(header_name, header_value);
            }
            let connect = async {
                // #[non_exhaustive]：先 Default 再改字段。
                let mut config = rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::default();
                config.uri = server.url.as_str().into();
                config.custom_headers = custom_headers;
                let transport = StreamableHttpClientTransport::from_config(config);
                let service = ()
                    .serve(transport)
                    .await
                    .map_err(|e| format!("连接远程 MCP 失败：{e}"))?;
                let tools = service
                    .list_all_tools()
                    .await
                    .map_err(|e| format!("获取 MCP 工具列表失败：{e}"))?;
                Ok::<(RunningService<RoleClient, ()>, Vec<Tool>), String>((service, tools))
            };
            tokio::time::timeout(CONNECT_TIMEOUT_REMOTE, connect)
                .await
                .map_err(|_| "连接远程 MCP 超时".to_string())?
        }
        other => Err(format!("未知的 MCP 类型：{other}")),
    }
}

async fn cancel_entry(entry: &mut ServerRuntime) {
    entry.peer = None;
    entry.tools.clear();
    if let Some(service) = entry.service.take() {
        if let Err(error) = service.cancel().await {
            log_warn!("MCP 连接关闭异常：{error}");
        }
    }
}

async fn store_connected(
    state: &McpState,
    server: &McpServer,
    result: Result<(RunningService<RoleClient, ()>, Vec<Tool>), String>,
) {
    let mut inner = state.inner.lock().await;
    let entry = inner
        .entry(server.id)
        .or_insert_with(|| ServerRuntime::new(server.name.clone(), server.enabled));
    entry.name = server.name.clone();
    entry.enabled = server.enabled;
    cancel_entry(entry).await;
    match result {
        Ok((service, tools)) => {
            log_info!(
                "MCP 服务器「{}」已连接，工具数 {}",
                server.name,
                tools.len()
            );
            entry.peer = Some(service.peer().clone());
            entry.service = Some(service);
            entry.tools = tools;
            entry.error = None;
            entry.stale = false;
        }
        Err(error) => {
            log_error!("MCP 服务器「{}」连接失败：{error}", server.name);
            entry.error = Some(error);
        }
    }
}

/// 应用启动时拉起所有已启用的服务器（stdio 与 remote 都在此连接）。
pub fn bootstrap(app: AppHandle, servers: Vec<McpServer>) {
    for server in servers.into_iter().filter(|s| s.enabled) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let state = app.state::<McpState>();
            let result = connect(&server).await;
            store_connected(state.inner(), &server, result).await;
        });
    }
}

/// 启用/停用切换（db 已更新，server 为最新行）。
pub async fn set_enabled(state: &McpState, server: &McpServer) {
    if server.enabled && server.transport == "remote" {
        // 远程动态加载：立即按当前配置重连。
        let result = connect(server).await;
        store_connected(state, server, result).await;
        return;
    }
    let mut inner = state.inner.lock().await;
    let entry = inner
        .entry(server.id)
        .or_insert_with(|| ServerRuntime::new(server.name.clone(), server.enabled));
    entry.name = server.name.clone();
    entry.enabled = server.enabled;
    if !server.enabled {
        // stdio 进程保留（跟随启动生命周期），远程断开。
        if server.transport == "remote" {
            cancel_entry(entry).await;
        }
    }
}

/// 配置被修改：远程若启用则立即按新配置重连；stdio 标记 stale，重启后生效。
pub async fn reconcile_updated(state: &McpState, server: &McpServer) {
    if server.transport == "remote" {
        set_enabled(state, server).await;
        return;
    }
    let mut inner = state.inner.lock().await;
    let entry = inner
        .entry(server.id)
        .or_insert_with(|| ServerRuntime::new(server.name.clone(), server.enabled));
    entry.name = server.name.clone();
    entry.enabled = server.enabled;
    entry.stale = true;
}

/// 删除服务器：断开连接（stdio 子进程随之退出）并移除运行时状态。
pub async fn remove(state: &McpState, id: i64) {
    let mut inner = state.inner.lock().await;
    if let Some(mut entry) = inner.remove(&id) {
        cancel_entry(&mut entry).await;
    }
}

/// 退出应用时断开全部连接（stdio 子进程随 transport drop 被杀掉）。
pub async fn shutdown(state: &McpState) {
    let mut inner = state.inner.lock().await;
    for (_, mut entry) in inner.drain() {
        cancel_entry(&mut entry).await;
    }
    log_info!("所有 MCP 连接已关闭");
}

/// 当前可注入 AI 的工具（仅 enabled 且已连接的服务器）。
pub async fn available_tools(state: &McpState) -> Vec<McpToolEntry> {
    let inner = state.inner.lock().await;
    let mut out = Vec::new();
    for entry in inner.values() {
        if !entry.enabled || entry.service.is_none() {
            continue;
        }
        for tool in &entry.tools {
            let description = tool
                .description
                .as_deref()
                .unwrap_or_default()
                .to_string();
            out.push(McpToolEntry {
                prefixed_name: prefixed_tool_name(&entry.name, tool.name.as_ref()),
                server_name: entry.name.clone(),
                description: if description.is_empty() {
                    format!("来自 MCP 服务「{}」的工具", entry.name)
                } else {
                    format!("[{}] {description}", entry.name)
                },
                input_schema: serde_json::Value::Object(tool.input_schema.as_ref().clone()),
            });
        }
    }
    out
}

/// 按前缀名调用工具。
pub async fn call_tool(
    state: &McpState,
    prefixed_name: &str,
    args: serde_json::Value,
) -> Result<String, String> {
    let (peer, original_name) = {
        let inner = state.inner.lock().await;
        let mut found = None;
        for entry in inner.values() {
            if !entry.enabled || entry.service.is_none() {
                continue;
            }
            for tool in &entry.tools {
                if prefixed_tool_name(&entry.name, tool.name.as_ref()) == prefixed_name {
                    found = entry.peer.clone().map(|peer| (peer, tool.name.to_string()));
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        found.ok_or_else(|| format!("MCP 工具不可用：{prefixed_name}"))?
    };
    let mut params = rmcp::model::CallToolRequestParams::new(original_name);
    if let Some(arguments) = args.as_object().cloned() {
        params = params.with_arguments(arguments);
    }
    let result = tokio::time::timeout(CALL_TIMEOUT, peer.call_tool(params))
        .await
        .map_err(|_| format!("MCP 工具调用超时：{prefixed_name}"))?
        .map_err(|e| format!("MCP 工具调用失败：{e}"))?;
    let mut text = result
        .content
        .iter()
        .filter_map(|block| block.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        text = serde_json::to_string(&result.structured_content)
            .unwrap_or_else(|_| "（无文本结果）".into());
    }
    if result.is_error == Some(true) {
        return Err(format!("MCP 工具返回错误：{text}"));
    }
    // 避免超长结果撑爆上下文。
    const MAX_RESULT_CHARS: usize = 4000;
    if text.chars().count() > MAX_RESULT_CHARS {
        text = text.chars().take(MAX_RESULT_CHARS).collect::<String>() + "……（结果过长已截断）";
    }
    Ok(text)
}

/// 组合 db 行与运行时状态，供设置页展示。
pub async fn statuses(state: &McpState, servers: Vec<McpServer>) -> Vec<McpServerStatus> {
    let inner = state.inner.lock().await;
    servers
        .into_iter()
        .map(|server| {
            let entry = inner.get(&server.id);
            let (status, tool_count, error) = if !server.enabled {
                ("disabled", 0, None)
            } else {
                match entry {
                    Some(entry) if entry.service.is_some() => {
                        ("connected", entry.tools.len(), entry.error.clone())
                    }
                    Some(entry) if entry.error.is_some() => ("error", 0, entry.error.clone()),
                    _ => ("pending_restart", 0, None),
                }
            };
            let requires_restart = server.enabled
                && server.transport == "stdio"
                && status != "error"
                && entry.map_or(true, |e| e.stale || e.service.is_none());
            McpServerStatus {
                server,
                status: status.into(),
                tool_count,
                requires_restart,
                error,
            }
        })
        .collect()
}

/// 单个服务器的工具清单（未连接时返回空）。
pub async fn server_tools(state: &McpState, id: i64) -> Vec<McpToolInfo> {
    let inner = state.inner.lock().await;
    inner
        .get(&id)
        .map(|entry| {
            entry
                .tools
                .iter()
                .map(|tool| McpToolInfo {
                    name: tool.name.to_string(),
                    description: tool
                        .description
                        .as_deref()
                        .unwrap_or_default()
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}
