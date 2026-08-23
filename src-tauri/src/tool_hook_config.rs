use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::db::AppSettings;

const OPENCODE_PLUGIN_FILE: &str = "chatpet-task-done.js";
const OPENCODE_MARKER: &str = "// chatpet-hook";
const CODEX_SCRIPT_FILE: &str = "chatpet-stop.ps1";
const CODEX_SCRIPT_MARKER: &str = "# chatpet-hook";
const CODEX_HOOKS_JSON: &str = "hooks.json";
const KIMI_SCRIPT_FILE: &str = "chatpet-stop.ps1";
const KIMI_SCRIPT_MARKER: &str = "# chatpet-hook";
const KIMI_HOOK_BEGIN: &str = "# >>> chatpet hook >>>";
const KIMI_HOOK_END: &str = "# <<< chatpet hook <<<";

#[derive(Debug, Clone, Serialize)]
pub struct ToolHookStatus {
    pub status: String,
    pub detail: String,
}

impl ToolHookStatus {
    fn not_configured() -> Self {
        Self { status: "not_configured".into(), detail: "未配置".into() }
    }
    fn configured() -> Self {
        Self { status: "configured".into(), detail: "已配置".into() }
    }
    fn needs_update() -> Self {
        Self {
            status: "needs_update".into(),
            detail: "配置已写入，但端口或校验信息已变更，需重新配置".into(),
        }
    }
    fn error(detail: impl Into<String>) -> Self {
        Self { status: "error".into(), detail: detail.into() }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolHookItemInfo {
    pub id: String,
    pub label: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolHookToolInfo {
    pub id: String,
    pub name: String,
    pub items: Vec<ToolHookItemInfo>,
}

pub fn list_supported(settings: &AppSettings) -> Vec<ToolHookToolInfo> {
    vec![
        ToolHookToolInfo {
            id: "opencode".into(),
            name: "OpenCode".into(),
            items: vec![item_info("task_done", "任务完成提示", opencode_detect(settings))],
        },
        ToolHookToolInfo {
            id: "codex".into(),
            name: "Codex".into(),
            items: vec![item_info("task_done", "任务完成提示", codex_detect(settings))],
        },
        ToolHookToolInfo {
            id: "kimi".into(),
            name: "Kimi".into(),
            items: vec![item_info("task_done", "任务完成提示", kimi_detect(settings))],
        },
    ]
}

fn item_info(id: &str, label: &str, status: ToolHookStatus) -> ToolHookItemInfo {
    ToolHookItemInfo { id: id.into(), label: label.into(), status: status.status, detail: status.detail }
}

pub fn write(tool: &str, item: &str, settings: &AppSettings) -> Result<ToolHookStatus, String> {
    if item != "task_done" {
        return Err("不支持的配置项".into());
    }
    match tool {
        "opencode" => opencode_write(settings),
        "codex" => codex_write(settings),
        "kimi" => kimi_write(settings),
        _ => Err("不支持的软件".into()),
    }
}

pub fn remove(tool: &str, item: &str, _settings: &AppSettings) -> Result<ToolHookStatus, String> {
    if item != "task_done" {
        return Err("不支持的配置项".into());
    }
    match tool {
        "opencode" => opencode_remove(),
        "codex" => codex_remove(),
        "kimi" => kimi_remove(),
        _ => Err("不支持的软件".into()),
    }
}

// ---------- 通用路径 / 备份工具 ----------

fn home_dir() -> PathBuf {
    std::env::var("USERPROFILE")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(PathBuf::from))
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn timestamp() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn write_file_atomic(path: &Path, content: &str) -> Result<(), String> {
    let tmp = PathBuf::from(format!("{}.tmp", path.display()));
    std::fs::write(&tmp, content).map_err(|e| format!("写入临时文件失败：{e}"))?;
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| format!("替换旧文件失败：{e}"))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| format!("落盘失败：{e}"))
}

fn backup_existing(path: &Path) -> Result<Option<PathBuf>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let backup = PathBuf::from(format!("{}.chatpet.bak-{}", path.display(), timestamp()));
    std::fs::copy(path, &backup).map_err(|e| format!("备份失败：{e}"))?;
    Ok(Some(backup))
}

fn latest_backup(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    let name = path.file_name()?.to_string_lossy().to_string();
    let prefix = format!("{name}.chatpet.bak-");
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(parent)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix))
        })
        .collect();
    candidates.sort();
    candidates.pop()
}

// ---------- opencode ----------

fn opencode_plugin_file() -> PathBuf {
    let xdg_dir = home_dir().join(".config").join("opencode").join("plugins");
    if let Ok(appdata) = std::env::var("APPDATA") {
        let alt_dir = PathBuf::from(&appdata).join("opencode").join("plugins");
        if alt_dir.exists() && !xdg_dir.exists() {
            return alt_dir.join(OPENCODE_PLUGIN_FILE);
        }
    }
    xdg_dir.join(OPENCODE_PLUGIN_FILE)
}

fn opencode_plugin_content(port: u32, token: &str) -> String {
    format!(
        r#"// chatpet-hook — written by ChatPet
export const ChatPetHook = async ({{ project, directory }}) => {{
  return {{
    event: async ({{ event }}) => {{
      const {{ type, properties }} = event;
      let kind = null;
      let sessionID = "";
      if (type === "session.status" && properties?.status?.type === "idle") {{
        kind = "session.idle";
        sessionID = properties.sessionID ?? "";
      }} else if (type === "session.error") {{
        kind = "session.error";
        sessionID = properties?.sessionID ?? "";
      }} else {{
        return;
      }}
      try {{
        await fetch("http://127.0.0.1:{port}/hook/opencode", {{
          method: "POST",
          headers: {{ "content-type": "application/json", "x-chatpet-token": "{token}" }},
          body: JSON.stringify({{
            tool: "opencode",
            event: kind,
            session_id: sessionID,
            project: project?.id ?? "",
            cwd: directory ?? "",
            timestamp: Date.now(),
          }}),
        }});
      }} catch (e) {{}}
    }},
  }};
}};
"#
    )
}

fn opencode_detect(settings: &AppSettings) -> ToolHookStatus {
    let file = opencode_plugin_file();
    if !file.exists() {
        return ToolHookStatus::not_configured();
    }
    let Ok(content) = std::fs::read_to_string(&file) else {
        return ToolHookStatus::error("插件文件存在但无法读取");
    };
    if !content.contains(OPENCODE_MARKER) {
        return ToolHookStatus::error("插件文件存在但不是桌宠写入（文件已被其它程序占用）");
    }
    if !content.contains(&format!("http://127.0.0.1:{}", settings.tool_hook_port))
        || !content.contains(&format!("\"x-chatpet-token\": \"{}\"", settings.tool_hook_token))
    {
        return ToolHookStatus::needs_update();
    }
    ToolHookStatus::configured()
}

fn opencode_write(settings: &AppSettings) -> Result<ToolHookStatus, String> {
    let file = opencode_plugin_file();
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建插件目录失败：{e}"))?;
    }
    if file.exists() {
        let content = std::fs::read_to_string(&file).unwrap_or_default();
        if !content.contains(OPENCODE_MARKER) {
            backup_existing(&file)?;
        }
    }
    let content = opencode_plugin_content(settings.tool_hook_port, &settings.tool_hook_token);
    write_file_atomic(&file, &content)?;
    let written = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
    if !written.contains(OPENCODE_MARKER) {
        return Err("写入校验失败".into());
    }
    Ok(ToolHookStatus::configured())
}

fn opencode_remove() -> Result<ToolHookStatus, String> {
    let file = opencode_plugin_file();
    if !file.exists() {
        return Ok(ToolHookStatus::not_configured());
    }
    std::fs::remove_file(&file).map_err(|e| format!("删除插件文件失败：{e}"))?;
    if let Some(backup) = latest_backup(&file) {
        std::fs::rename(&backup, &file).map_err(|e| format!("恢复原文件失败：{e}"))?;
    }
    Ok(ToolHookStatus::not_configured())
}

// ---------- codex ----------

fn codex_dir() -> PathBuf {
    home_dir().join(".codex")
}

fn codex_script_file() -> PathBuf {
    codex_dir().join("hooks").join(CODEX_SCRIPT_FILE)
}

fn codex_hooks_json() -> PathBuf {
    codex_dir().join(CODEX_HOOKS_JSON)
}

fn codex_script_content(port: u32, token: &str) -> String {
    format!(
        "# chatpet-hook — written by ChatPet\ntry {{\n  $raw = [Console]::In.ReadToEnd()\n  $ev = $raw | ConvertFrom-Json\n  $project = \"\"\n  if ($ev.cwd) {{ $project = Split-Path $ev.cwd -Leaf }}\n  $body = @{{\n    tool = \"codex\"\n    event = \"turn_done\"\n    session_id = $ev.session_id\n    project = $project\n    cwd = $ev.cwd\n    timestamp = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()\n  }} | ConvertTo-Json -Compress\n  Invoke-WebRequest -Uri \"http://127.0.0.1:{port}/hook/codex\" -Method POST -ContentType \"application/json\" -Headers @{{ \"x-chatpet-token\" = \"{token}\" }} -Body $body -UseBasicParsing | Out-Null\n}} catch {{\n  exit 0\n}}\nexit 0\n"
    )
}

fn codex_command(script: &Path) -> String {
    format!(
        "powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"{}\"",
        script.display()
    )
}

fn group_contains_script(group: &serde_json::Value) -> bool {
    group
        .get("hooks")
        .and_then(|v| v.as_array())
        .is_some_and(|handlers| {
            handlers.iter().any(|h| {
                [h.get("command"), h.get("commandWindows")]
                    .iter()
                    .flatten()
                    .filter_map(|v| v.as_str())
                    .any(|cmd| cmd.contains(CODEX_SCRIPT_FILE))
            })
        })
}

fn read_hooks_json(path: &Path) -> Result<serde_json::Value, String> {
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("读取 hooks.json 失败：{e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("hooks.json 不是合法 JSON：{e}"))
}

fn codex_upsert_hooks(script: &Path, timeout: u32) -> Result<(), String> {
    let hooks_path = codex_hooks_json();
    let mut root = read_hooks_json(&hooks_path)?;
    let root_obj = root
        .as_object_mut()
        .ok_or("hooks.json 顶层必须是对象")?;
    let hooks_obj = root_obj
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or("hooks.json 的 hooks 必须是对象")?;
    let stop_arr = hooks_obj
        .entry("Stop")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .ok_or("hooks.json 的 Stop 必须是数组")?;
    stop_arr.retain(|group| !group_contains_script(group));
    let command = codex_command(script);
    stop_arr.push(serde_json::json!({
        "hooks": [
            { "type": "command", "command": command, "commandWindows": command, "timeout": timeout }
        ]
    }));
    let text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    write_file_atomic(&hooks_path, &format!("{text}\n"))
}

fn codex_remove_hooks() -> Result<(), String> {
    let hooks_path = codex_hooks_json();
    if !hooks_path.exists() {
        return Ok(());
    }
    let mut root = read_hooks_json(&hooks_path)?;
    let mut emptied = false;
    if let Some(hooks_obj) = root.get_mut("hooks").and_then(|v| v.as_object_mut()) {
        if let Some(stop_arr) = hooks_obj.get_mut("Stop").and_then(|v| v.as_array_mut()) {
            stop_arr.retain(|group| !group_contains_script(group));
            if stop_arr.is_empty() {
                hooks_obj.remove("Stop");
            }
        }
        if hooks_obj.is_empty() {
            root.as_object_mut().map(|o| o.remove("hooks"));
            emptied = true;
        }
    }
    let sentinel = PathBuf::from(format!("{}.chatpet-original-missing", hooks_path.display()));
    if emptied && root.as_object().is_some_and(|o| o.is_empty()) && sentinel.exists() {
        std::fs::remove_file(&hooks_path).map_err(|e| format!("删除 hooks.json 失败：{e}"))?;
        let _ = std::fs::remove_file(&sentinel);
        return Ok(());
    }
    let text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    write_file_atomic(&hooks_path, &format!("{text}\n"))
}

fn codex_detect(settings: &AppSettings) -> ToolHookStatus {
    let script = codex_script_file();
    let hooks_path = codex_hooks_json();
    let entry_present = match read_hooks_json(&hooks_path) {
        Ok(root) => root
            .get("hooks")
            .and_then(|v| v.get("Stop"))
            .and_then(|v| v.as_array())
            .is_some_and(|groups| groups.iter().any(group_contains_script)),
        Err(_) => false,
    };
    if !entry_present {
        return ToolHookStatus::not_configured();
    }
    if !script.exists() {
        return ToolHookStatus::error("hooks.json 已配置，但通知脚本缺失");
    }
    let Ok(content) = std::fs::read_to_string(&script) else {
        return ToolHookStatus::error("通知脚本无法读取");
    };
    if !content.contains(CODEX_SCRIPT_MARKER) {
        return ToolHookStatus::error("通知脚本不是桌宠写入（文件已被其它程序占用）");
    }
    if !content.contains(&format!("http://127.0.0.1:{}", settings.tool_hook_port))
        || !content.contains(&format!("\"x-chatpet-token\" = \"{}\"", settings.tool_hook_token))
    {
        return ToolHookStatus::needs_update();
    }
    ToolHookStatus::configured()
}

fn codex_write(settings: &AppSettings) -> Result<ToolHookStatus, String> {
    let script = codex_script_file();
    if let Some(parent) = script.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建脚本目录失败：{e}"))?;
    }
    if script.exists() {
        let content = std::fs::read_to_string(&script).unwrap_or_default();
        if !content.contains(CODEX_SCRIPT_MARKER) {
            backup_existing(&script)?;
        }
    }
    let content = codex_script_content(settings.tool_hook_port, &settings.tool_hook_token);
    write_file_atomic(&script, &content)?;

    let hooks_path = codex_hooks_json();
    if !hooks_path.exists() {
        let sentinel = PathBuf::from(format!("{}.chatpet-original-missing", hooks_path.display()));
        std::fs::write(&sentinel, "").map_err(|e| format!("写入标记失败：{e}"))?;
    } else {
        backup_existing(&hooks_path)?;
    }
    codex_upsert_hooks(&script, 30)?;
    Ok(ToolHookStatus::configured())
}

fn codex_remove() -> Result<ToolHookStatus, String> {
    let script = codex_script_file();
    if script.exists() {
        std::fs::remove_file(&script).map_err(|e| format!("删除通知脚本失败：{e}"))?;
        if let Some(backup) = latest_backup(&script) {
            std::fs::rename(&backup, &script).map_err(|e| format!("恢复原脚本失败：{e}"))?;
        }
    }
    codex_remove_hooks()?;
    Ok(ToolHookStatus::not_configured())
}

// ---------- kimi ----------

fn kimi_dir() -> PathBuf {
    let code = home_dir().join(".kimi-code");
    let cli = home_dir().join(".kimi");
    if code.exists() && !cli.exists() {
        return code;
    }
    cli
}

fn kimi_script_file() -> PathBuf {
    kimi_dir().join("hooks").join(KIMI_SCRIPT_FILE)
}

fn kimi_config_toml() -> PathBuf {
    kimi_dir().join("config.toml")
}

fn kimi_script_content(port: u32, token: &str) -> String {
    format!(
        "# chatpet-hook — written by ChatPet\ntry {{\n  $raw = [Console]::In.ReadToEnd()\n  $ev = $raw | ConvertFrom-Json\n  $project = \"\"\n  if ($ev.cwd) {{ $project = Split-Path $ev.cwd -Leaf }}\n  $body = @{{\n    tool = \"kimi\"\n    event = \"turn_done\"\n    session_id = $ev.session_id\n    project = $project\n    cwd = $ev.cwd\n    timestamp = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()\n  }} | ConvertTo-Json -Compress\n  Invoke-WebRequest -Uri \"http://127.0.0.1:{port}/hook/kimi\" -Method POST -ContentType \"application/json\" -Headers @{{ \"x-chatpet-token\" = \"{token}\" }} -Body $body -UseBasicParsing | Out-Null\n}} catch {{\n  exit 0\n}}\nexit 0\n"
    )
}

fn kimi_command(script: &Path) -> String {
    format!(
        "powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"{}\"",
        script.display()
    )
}

fn kimi_hook_block(script: &Path) -> String {
    let command = kimi_command(script);
    format!(
        "{}\n[[hooks]]\nevent = \"Stop\"\ncommand = '{}'\ntimeout = 30\n{}\n",
        KIMI_HOOK_BEGIN, command, KIMI_HOOK_END
    )
}

fn strip_kimi_hook_block(content: &str) -> String {
    let mut result = String::new();
    let mut in_block = false;
    for line in content.lines() {
        if line.trim() == KIMI_HOOK_BEGIN {
            in_block = true;
            continue;
        }
        if line.trim() == KIMI_HOOK_END {
            in_block = false;
            continue;
        }
        if !in_block {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

fn kimi_detect(settings: &AppSettings) -> ToolHookStatus {
    let config = kimi_config_toml();
    let config_content = std::fs::read_to_string(&config).unwrap_or_default();
    if !config_content.contains(KIMI_HOOK_BEGIN) {
        return ToolHookStatus::not_configured();
    }
    let script = kimi_script_file();
    if !script.exists() {
        return ToolHookStatus::error("config.toml 已配置，但通知脚本缺失");
    }
    let Ok(content) = std::fs::read_to_string(&script) else {
        return ToolHookStatus::error("通知脚本无法读取");
    };
    if !content.contains(KIMI_SCRIPT_MARKER) {
        return ToolHookStatus::error("通知脚本不是桌宠写入（文件已被其它程序占用）");
    }
    if !content.contains(&format!("http://127.0.0.1:{}", settings.tool_hook_port))
        || !content.contains(&format!("\"x-chatpet-token\" = \"{}\"", settings.tool_hook_token))
    {
        return ToolHookStatus::needs_update();
    }
    ToolHookStatus::configured()
}

fn kimi_write(settings: &AppSettings) -> Result<ToolHookStatus, String> {
    let script = kimi_script_file();
    if let Some(parent) = script.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建脚本目录失败：{e}"))?;
    }
    if script.exists() {
        let content = std::fs::read_to_string(&script).unwrap_or_default();
        if !content.contains(KIMI_SCRIPT_MARKER) {
            backup_existing(&script)?;
        }
    }
    let content = kimi_script_content(settings.tool_hook_port, &settings.tool_hook_token);
    write_file_atomic(&script, &content)?;

    let config = kimi_config_toml();
    if config.exists() {
        backup_existing(&config)?;
    }
    let existing = std::fs::read_to_string(&config).unwrap_or_default();
    let stripped = strip_kimi_hook_block(&existing);
    let new_content = format!("{stripped}{}", kimi_hook_block(&script));
    write_file_atomic(&config, &new_content)?;
    Ok(ToolHookStatus::configured())
}

fn kimi_remove() -> Result<ToolHookStatus, String> {
    let script = kimi_script_file();
    if script.exists() {
        std::fs::remove_file(&script).map_err(|e| format!("删除通知脚本失败：{e}"))?;
        if let Some(backup) = latest_backup(&script) {
            std::fs::rename(&backup, &script).map_err(|e| format!("恢复原脚本失败：{e}"))?;
        }
    }
    let config = kimi_config_toml();
    if config.exists() {
        let existing = std::fs::read_to_string(&config).unwrap_or_default();
        let stripped = strip_kimi_hook_block(&existing);
        if stripped.trim().is_empty() {
            std::fs::remove_file(&config).map_err(|e| format!("删除 config.toml 失败：{e}"))?;
        } else {
            write_file_atomic(&config, &stripped)?;
        }
    }
    Ok(ToolHookStatus::not_configured())
}
