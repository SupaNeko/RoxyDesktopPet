# 编程工具联动提醒（Tool Hook）设计方案

> 文档状态：设计评审稿 v2
> 对应需求：当 AI 编程工具（opencode、Codex、Claude Code 等）的会话任务结束时，桌宠收到 hook 信号并主动提醒用户任务已完成。
> 目标平台：Windows 10/11
> 更新日期：2026-08-23

## 1. 目标与非目标

### 目标

- 桌宠后端暴露一个本地 HTTP 端口，接收编程工具 hook 发送的"任务完成"信号。
- 收到信号后，桌宠以固定文本或 AI 动态文本两种模式，主动生成一条提示消息并展示/朗读。
- 设置页提供"编程联动"标签页：下拉选择软件 → 自动检测该软件各项配置状态 → 一键配置 / 删除配置，配置管理安全、可恢复、不影响用户既有配置。
- 第一版只接入 opencode（一个配置项：任务完成提示），其余工具预留适配层。

### 非目标（第一版不做）

- 不读取编程工具内容/会话全文，只消费事件信号与最小元数据。
- 不做指令下发、文件操作、Shell 执行等"控制工具"能力。
- 不反向把桌宠消息注入编程工具。
- 不监听非"完成"类事件（如每次工具调用）。

## 2. 外部工具 hook 机制调研摘要

| 工具 | 事件/机制 | 配置位置 | 是否支持 HTTP | 完成事件 |
| --- | --- | --- | --- | --- |
| **Claude Code** | hooks：`Stop`（每轮完成）、`SessionEnd`（会话结束）、`Notification(agent_completed)` | `~/.claude/settings.json` 的 `hooks` 段 | **原生 `type:"http"`** | `Stop` / `SessionEnd` / `Notification` |
| **Codex CLI** | hooks（仅 `command` 型）+ `notify`（`agent-turn-complete`） | `~/.codex/hooks.json`、`~/.codex/config.toml` | 无（需脚本 curl） | `SessionEnd` / `Stop` / `notify` |
| **opencode** | 插件（plugin）系统，进程内订阅事件 | `~/.config/opencode/plugins/*.js` 自动加载，**不改 opencode.json** | 无内置 HTTP；插件内 `fetch` | `session.status(idle)` / `session.error` |
| **Aider** | `--notifications-command` | `~/.aider.conf.yml` | 无（命令内 curl） | 通知命令 |
| **Cursor** | 仅云端 Agent webhook（`statusChange`），不投递本机 | 云端 API | 云端 | 本地无法接入 |

**最佳参考：Claude Code 的 hooks**（事件最全、原生 HTTP 直连本机端口）。本设计的事件协议、适配层结构与"hook → 本机端口 → 桌宠"链路以其为蓝本。

### 2.1 关键语义核实（opencode，源码 commit `3a31c4e`）

- **`session.status`（type=`idle`）就是"任务链执行完、等待用户输入"**：`Runner` 每次 run 结束回到 Idle 态并回调 `onIdle`（`packages/opencode/src/session/run-state.ts:60`），`runLoop` 覆盖"一条用户消息的完整链条"（多轮 LLM step、工具调用、compaction、自动续跑都在一个 work 内，全程 `busy`，最后统一回到 idle）。一次用户输入 → 一次 idle。
- **没有独立的"会话结束"事件**。会话长期存在，唯一生命周期事件是 `session.deleted`。子代理是独立 session，各自进入 idle。
- **`session.idle` 已标记 `deprecated`**（`session-status-event.ts:43`），官方推荐 `session.status`（载荷含 `status.type`）。
- 插件 `event` 回调收到的对象为 `{ id, type, properties }`：`session.status` 的 `properties = { sessionID, status }`；`session.error` 的 `properties = { sessionID?, error }`。载荷没有 turn 计数/token/时间戳，插件侧自行补 `Date.now()`；需要更丰富信息时调用 `client.session.get(sessionID)`。
- `project` 对象字段为 `id` / `worktree` / `name` 等，**没有 `directory`**；`directory` 是插件入参的独立字段。

> 推论：idle 每轮触发，用户持续交互时每问一句都会触发一次。**可配置的"提醒间隔"是防打扰核心机制**（第 6、9 节），而非依赖更晚的事件。

## 3. 总体架构

```text
┌─ 编程工具侧 ────────────────────────────┐
│ opencode 插件 (chatpet-task-done.js)     │
│   └─ 订阅 session.status(idle) / session.error
│        └─ POST http://127.0.0.1:PORT/hook/opencode
│ Claude Code hooks (后续)                 │
│   └─ type:"http" → POST 同一端口          │
│ Codex notify 脚本 (后续)                 │
│   └─ curl → POST 同一端口                 │
└──────────────────────────────────────────┘
                    │ 统一事件 JSON（仅本机回环，无 Token）
                    ▼
┌─ 桌宠侧 (Rust/Tauri 后端) ──────────────┐
│ hook_server.rs  本地 HTTP 监听器          │
│   └─ 校验 路径/体大小/超时 → HookEvent  │
│ tool_hook.rs    事件处理管线               │
│   └─ 去重/提醒间隔/每日上限 → 去抖 → 生成消息
│        ├─ 固定模式: 模板替换               │
│        └─ AI 模式: generate_scheduled_message
│              │
│              ▼
│ 存 SQLite → emit("assistant-message")
│           → voice_output::schedule (可选)
│
│ config.rs     各工具适配层（检测/写入/删除）│
└──────────────────────────────────────────┘
```

- HTTP 服务器只绑定 `127.0.0.1`，仅接收 POST，随应用启动/停止。
- 事件处理完成后走既有"主动消息"链路，复用现成能力。

## 4. 统一事件协议

各工具适配层把工具私有事件归一化为统一 JSON（适配层代码由桌宠写入，载荷可控）：

```json
POST /hook/opencode
Content-Type: application/json

{
  "tool": "opencode",
  "event": "session.idle",
  "session_id": "uuid",
  "project": "RoxyDesktopPet",
  "cwd": "D:\\code_project\\RoxyDesktopPet",
  "timestamp": 1787313600000,
  "last_assistant_message": "可选：最后一条助手消息截断摘要"
}
```

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `tool` | 是 | `opencode` / `codex` / `claude` / `aider`… |
| `event` | 是 | `session.idle`、`session.error`、`turn_done`、`session_done`… |
| `session_id` | 否 | 去重键之一 |
| `project` | 否 | 项目名（供提示文本与 AI 上下文） |
| `cwd` | 否 | 工作目录 |
| `timestamp` | 是 | 事件发生时间（ms） |
| `last_assistant_message` | 否 | AI 模式可选信息来源，适配层截断到约 300 字符 |

适配层把 `session.status(idle)` 归一化为 `event:"session.idle"`、`session.error` 归一化为 `event:"session.error"`，并补 `project`/`cwd`/`timestamp`，接收端与工具私有格式解耦。

## 5. 接收端：本地 HTTP 服务器设计

### 5.1 实现选型

推荐用 **tokio `TcpListener` 手写最小 HTTP/1.1 监听器**（新增 `tokio` 的 `net` feature），约 150 行：

- 只需 `POST /hook/<tool>` + `GET /health`，请求体为小 JSON（≤ 64KB）。
- 只绑定 127.0.0.1，无需 TLS、无需路由框架；与项目"轻依赖、手写协议"风格一致（参考 `voice_output.rs` 手写 WAV 解析）。
- 若后续需要 WebSocket、多端点等再迁移 `axum`（接口封装在模块内，不影响上层）。

### 5.2 约束

- 绑定地址固定 `127.0.0.1`；端口来自设置（默认 `34125`）。
- 每连接读取超时 5s，请求体上限 64KB，超过返回 413。
- 仅接受 `POST`；路径前缀 `/hook/`，工具名白名单校验。
- 响应统一 `application/json`：`200 {"ok":true}` / `400` / `413` / `404`。
- `GET /health` 返回 `{"ok":true}`，供设置页状态检查与调试。
- 绑定失败（端口被占用）不崩溃：状态置 `error`，设置页展示，用户改端口或关闭。

### 5.3 鉴权说明

- 服务器只绑定 `127.0.0.1` 本机回环，不接受外部连接，因此**不做 Token 校验**（曾因数据库重建导致 Token 漂移、hook 静默 401 失效，属于过度设计，已移除）。
- 端口仍来自设置（默认 `34125`）；`reconcile_server` 在启动和保存设置时检测外部工具 hook 脚本中的端口，过期则自动重写（自愈），无需重启对应工具。

## 6. 事件处理管线

```
接收 HookEvent
  → 1. 未启用？丢弃
  → 2. 工具白名单（只处理已启用的工具）
  → 3. 提醒间隔：同一 (tool, session_id 或 project) 距上次通知 < 间隔 → 丢弃
  → 4. 去抖（可选，默认 0）：等待 N 秒看会话是否还有后续 idle，有则重置计时器，
        无则视为"一次任务结束"（用于把多轮连续执行合并成一次提醒）
  → 5. 生成消息（固定模式 / AI 模式）
  → 6. 写入 SQLite（trigger_type="tool_hook"）→ emit("assistant-message") → voice_output::schedule
```

- **提醒间隔（默认 10 分钟）**是核心防打扰机制：用户在交互聊天时每轮 idle 都会被间隔挡掉；长任务完成后第一轮 idle 立即提醒。
- 每日上限默认 20；日限计数写入 SQLite，重启保持。
- AI 模式失败（未配置 API Key / 模型调用失败）时**降级为固定文本**，保证提醒一定到达。
- `tool_hook` 消息不会被记忆观察者提取（只处理 `user_text`/`user_voice`），也不触发工具审计。

## 7. 两种提示模式

### 7.1 固定提示（不调用 AI）

- 提示文本**按工具独立配置**：每个工具各有一份中文固定文本与日文语音文本（存于 `app_settings.tool_hook_tool_texts`，JSON map，key 为工具 id）；用户留空时回退到该工具在注册表（`ToolSpec`）中的默认语句（如 `OpenCode 已完成。` / `OpenCode のタスクが完了しました。`）。
- 支持占位符：`{tool}`、`{project}`、`{event}`、`{time}`。
- 日文语音文本：GPT-SoVITS 需要 `japanese_text`，固定模式下不调用翻译，默认语句已预填，用户可按工具覆盖。语音关闭/非 GPT-SoVITS 模式下仅用中文文本。
- 旧的全局共享字段 `tool_hook_fixed_text` / `tool_hook_fixed_voice_text` 已废弃：库中列保留但代码不再读写；升级时若用户自定义过旧文本，一次性迁移复制给所有已接入工具。
- 消息 emotion 固定 `calm`。

### 7.2 消息提示（调用 AI）

- 复用现有 `generate_scheduled_message(app, "tool_hook", &event_prompt)`，已含角色人设、自然语言规则、双语输出与情绪。
- `event_prompt` 信息来源组装：

```
这次是外部编程工具事件触发的主动提醒。来源信息：
- 工具：{tool}
- 项目：{project}
- 工作目录：{cwd}
- 事件类型：{event}
{若开启"附带最后消息摘要"且载荷提供：- 最后一条助手消息摘要：{last_assistant_message}}
请以角色身份、用 1～2 句自然的话提醒用户该工具的任务已完成。不要虚构载荷中不存在的信息。
```

- 开关"附带最后消息摘要"默认开；关闭时 AI 只拿到工具/项目/事件三个来源字段。

## 8. 配置管理：按工具 × 配置项

### 8.1 概念模型（可扩展）

每个软件是一个**工具**，每个工具下有若干个**配置项**（第一版只有一个：任务完成提示）。后续可在同一工具下新增配置项（如"开始执行时提示"、"出错提示"），结构不变。

```rust
// 配置项枚举（后续新增值即可扩展）
enum ToolHookItem {
    TaskDone, // 任务完成提示
}

struct ToolHookToolInfo {
    id: String,      // "opencode"
    name: String,    // "OpenCode"
    items: Vec<ToolHookItemInfo>, // 每个配置项：id / 名称 / 检测到的状态
}

enum ToolHookItemStatus {
    NotConfigured,      // 未配置
    Configured,         // 已配置且与当前设置一致
    NeedsUpdate,        // 已配置，但端口/Token/文本与当前设置不一致，需重新写入
    Error(String),      // 目录不存在、文件损坏、权限等
}
```

### 8.2 安全配置管理模式（成熟开源参考）

参考并借鉴以下成熟项目（已在本地核对其源码）：

- **Claude Code 官方插件机制**：配置内容放独立插件目录，settings 里只留最小注册键；`@skills-dir` 插件零 settings 写入，"删除文件夹即卸载"。
- **claude-code-router（CCR，36.8k★）**：JSON 读-合并-写回只改自己管理的键；TOML 用 `# BEGIN CCR managed …` 标记块；备份三件套（永久快照 `.ccr-original`、缺失哨兵 `.ccr-original-missing`、时间戳备份 `.ccr-backup-<ts>`）；删除时**仅当当前文件内容仍判定为"纯 CCR 管理"才恢复快照**，用户改过就拒绝覆盖。
- **Codex CLI（openai/codex）**：`toml_edit::DocumentMut` 保注释 round-trip；`NamedTempFile.persist()`（同目录临时文件 + rename）原子写；`/hooks` 不写 hooks.json，只管理信任状态。
- **HarnessKit**：`locked_modify_json` 加 fs2 独占锁；`deploy_hook`/`remove_hook` 按 `(event, matcher, command)` 精确指纹幂等增删，绝不错删他人 hook；JSONC 用 CST round-trip 保留注释。
- **ai-config-sync-manager**：apply 前备份镜像树 + 审计 ledger（before/after hash），每项操作可精确恢复。

**推荐的通用 playbook**（按优先级）：

1. **优先选"目录即配置"通道（零侵入）**。opencode 插件只要把 JS 文件放进 `~/.config/opencode/plugins/` 就自动加载，**完全不改 opencode.json**，删除就是删文件、无副作用。
2. **必须改 JSON 时**：读-合并-写回 + 幂等 upsert，只操作自己的专用键；写入后重新解析自校验，失败回滚。
3. **必须改 TOML 时**：marker 注释块（`# BEGIN chatpet …` / `# END chatpet`），删除即整块移除。
4. **写前三件套备份**：首次写入 `.chatpet-original` 永久快照（原文件不存在则写 `.chatpet-original-missing` 哨兵）；每次写入 `.chatpet-backup-<ISO时间戳>`。
5. **归属标记**：写入的插件文件/配置块带 `// chatpet-hook` / marker 注释，标识"这是我们写的"。
6. **删除恢复规则**：确认当前内容仍是我们写的才恢复；用户改过则拒绝覆盖并提示。
7. **原子写**：同目录临时文件 + rename。
8. **幂等**：重复执行安装不产生重复条目；删除不存在的条目是 no-op。

### 8.3 适配层接口（Rust）

实现采用静态注册表（`tool_hook_config.rs`）：

```rust
pub struct ToolSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub default_fixed_text: &'static str,        // 固定提示默认中文语句
    pub default_fixed_voice_text: &'static str,  // 固定提示默认日文语音语句
    pub detect: fn(&AppSettings) -> ToolHookStatus,
    pub write: fn(&AppSettings) -> Result<ToolHookStatus, String>,
    pub remove: fn() -> Result<ToolHookStatus, String>,
}

pub const TOOLS: &[ToolSpec] = &[ /* opencode / codex / kimi … */ ];
pub fn find_tool(id: &str) -> Option<&'static ToolSpec>;
```

`list_supported` / `write` / `remove` 与接收端的工具白名单全部经 `TOOLS` / `find_tool` 查表分发。新增工具 = 实现 detect/write/remove 三个函数 + 注册表加一行 + 事件归一化映射，接收端与管线零改动。

### 8.4 opencode 适配器（第一版）

配置项 `TaskDone` 对应文件：`%USERPROFILE%\.config\opencode\plugins\chatpet-task-done.js`（opencode 在 Windows 按 XDG 解析 `~`；若 `%APPDATA%\opencode\plugins` 存在则优先使用，实现时探测两者）。

- **detect**：
  - 文件不存在 → `NotConfigured`。
  - 文件存在但无 `// chatpet-hook` 标记 → `Error("文件已被其它工具占用")`，不覆盖，提示用户。
  - 是我们的文件，但内嵌端口/Token 与当前设置不一致 → `NeedsUpdate`。
  - 一致 → `Configured`。
- **write**（一键配置，幂等）：
  1. 目标文件是他人文件 → 先备份为 `<file>.chatpet.bak-<ts>`。
  2. 首次写入做 `.chatpet-original` 快照（或 `-missing` 哨兵）。
  3. 写临时文件 + rename 原子替换，内容见下。
  4. 回读文件头校验标记与端口/Token，失败则回滚。
- **remove**（删除配置）：
  1. 文件是我们的 → 删除。
  2. 存在 `.chatpet.bak-<ts>`（说明覆盖了他人文件）→ 恢复备份。
  3. 存在 `.chatpet-original` 快照且删除后恢复；原文件本来不存在则直接删除文件。
  4. 文件不是我们的 → no-op 并提示。

插件内容（Bun 运行时，`fetch` 可用）：

```js
// chatpet-hook — written by ChatPet
export const ChatPetHook = async ({ project, directory }) => {
  return {
    event: async ({ event }) => {
      const { type, properties } = event;
      let kind = null;
      let sessionID = "";
      if (type === "session.status" && properties?.status?.type === "idle") {
        kind = "session.idle";          // 任务链完成，等待用户输入
        sessionID = properties.sessionID ?? "";
      } else if (type === "session.error") {
        kind = "session.error";         // 本轮执行出错
        sessionID = properties?.sessionID ?? "";
      } else {
        return;
      }
      try {
        await fetch("http://127.0.0.1:PORT/hook/opencode", {
          method: "POST",
          headers: { "content-type": "application/json", "x-chatpet-token": "TOKEN" },
          body: JSON.stringify({
            tool: "opencode",
            event: kind,
            session_id: sessionID,
            project: project?.id ?? "",
            cwd: directory ?? "",
            timestamp: Date.now(),
          }),
        });
      } catch (e) { /* 桌宠未运行时静默失败 */ }
    },
  };
};
```

### 8.5 后续工具适配预留

| 工具 | 配置方式 | 说明 |
| --- | --- | --- |
| Claude Code | 写 `~/.claude/settings.json` 的 `hooks.Stop`/`SessionEnd`，`type:"http"` | JSON 读-合并-写回 + 幂等 + 三件套备份 |
| Codex | 写 `~/.codex/hooks.json`（command 型脚本 curl）或 `config.toml` 的 `notify` | hooks.json 归用户自持，我们只写脚本目录 + hooks.json 中我们的条目 |
| Aider | 写 `~/.aider.conf.yml` 的 `notifications_command` | YAML 用 marker 注释块 |

## 9. 设置页新增标签页

设置窗口改为顶部标签页：标签一「常规」（现有全部设置），标签二「编程联动」。

```
编程联动提醒（总开关）
├─ 工具选择（下拉选择框）
│    └─ [OpenCode ▾]  ← 选择后自动检测该工具各项配置状态
├─ 当前工具的配置项列表（每项独立一行）
│    任务完成提示 hook
│      状态徽标：未配置 / 已配置 / 已配置·端口已变更 / 错误详情
│      按钮： [一键配置]（未配置或需更新时可点）
│             [删除配置]（已配置时可点）
│      说明：写入后需重启 opencode 生效
├─ 提示设置
│    ├─ 提醒间隔（分钟，默认 10）：同一工具+项目两次提醒最短间隔
│    ├─ 每日上限（默认 20）
│    ├─ 去抖秒数（默认 0，多轮连续执行合并提醒）
│    ├─ 语音播报（开关，默认开；关闭时仅气泡）
│    └─ 提示方式：(radio) 固定提示 / 消息提示(AI)
│         ├─ 固定提示：中文文本(textarea，支持 {tool} {project} 占位符)
│         │            + 语音文本(日文，GPT-SoVITS 用，可留空)
│         └─ 消息提示：附带最后一条助手消息摘要（开关）
├─ 监听状态
│    ├─ 端口（number，默认 34125）
│    ├─ 服务器状态：运行中 / 未运行 / 端口被占用 / 启动失败
│    └─ 简单校验（Token）开关
└─ 测试
     └─ [模拟发送一次事件] → 验证 端口→收包→生成提示→气泡/语音 全链路
```

交互要点：

- 下拉选择软件后自动刷新该工具配置项状态（调用 `list_tool_hook_support`）。
- **一键配置**在状态为"未配置"或"需更新"时可点；**删除配置**在"已配置"时可点，其余置灰。
- 配置项列表按工具扩展：后续新增配置项会自动出现在列表里，无需改 UI 骨架。

## 10. 数据模型与设置字段

### 10.1 `app_settings` 新增列

| 字段 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `tool_hook_enabled` | INTEGER | 0 | 功能总开关 |
| `tool_hook_mode` | TEXT | `fixed` | `fixed` / `ai` |
| `tool_hook_port` | INTEGER | 34125 | 监听端口 |
| `tool_hook_token` | TEXT | '' | 随机 Token，前端不回显 |
| `tool_hook_token_enabled` | INTEGER | 1 | 简单校验开关 |
| `tool_hook_fixed_text` | TEXT | '' | （已废弃）旧的全局固定提示中文，仅作迁移来源 |
| `tool_hook_fixed_voice_text` | TEXT | '' | （已废弃）旧的全局日文语音文本，仅作迁移来源 |
| `tool_hook_tool_texts` | TEXT | '' | 按工具的文本配置，JSON map：`{"<tool_id>": {"fixed_text", "fixed_voice_text"}}` |
| `tool_hook_include_last_message` | INTEGER | 1 | AI 模式信息来源开关 |
| `tool_hook_min_interval_minutes` | INTEGER | 10 | 提醒间隔 |
| `tool_hook_daily_limit` | INTEGER | 20 | 每日上限 |
| `tool_hook_debounce_seconds` | INTEGER | 0 | 去抖秒数 |
| `tool_hook_voice_enabled` | INTEGER | 1 | 语音播报开关 |

迁移：沿用 `db::open` 中"列不存在则 ALTER TABLE"模式。日限计数单独存 `scheduler_state` 或新表 `tool_hook_state`。

### 10.2 运行时状态（内存）

- `ToolHookState`（Tauri managed）：`JoinHandle`、`AtomicBool 运行中`、`Mutex<端口/Token 快照>`、`Mutex<提醒间隔缓存 (tool+session → last_notify_at)>`、`Mutex<去抖计时器>`。

## 11. 代码改动清单

### Rust（`src-tauri/src/`）

| 文件 | 改动 |
| --- | --- |
| `hook_server.rs`（新增） | 最小 HTTP 监听器：`start(port, token)` / `stop()`；解析 POST → 回调；`/health` |
| `tool_hook.rs`（新增） | 事件处理管线（校验/间隔/去抖/生成消息/emit/语音）；固定模板替换；AI 模式调用 |
| `tool_hook_config.rs`（新增） | 工具适配层：`list_tool_hook_support`、opencode 插件文件检测/写入/删除/备份恢复 |
| `db.rs` | `AppSettings` 增字段 + Default + `open()` 迁移 + `get_settings`/`save_settings` SQL |
| `commands.rs` | `SaveSettingsRequest` 增字段；`save_settings` 落库；新增命令：`list_tool_hook_support`、`write_tool_hook_config`、`remove_tool_hook_config`、`test_tool_hook` |
| `lib.rs` | `mod` 声明；`.manage(ToolHookState::new())`；setup 按设置启动/停止监听器；注册新命令；退出优雅停止 |

### 前端（`src/`）

| 文件 | 改动 |
| --- | --- |
| `lib/types.ts` | `AppSettings` 增字段；新增 `ToolHookToolInfo`/`ToolHookItemStatus` 类型 |
| `lib/api.ts` | demoSettings 补齐；新增 4 个命令封装 |
| `App.svelte` | 设置页改标签页；新增"编程联动"标签（下拉 + 配置项状态 + 按钮 + 提示设置） |

## 12. 安全与隐私

- HTTP 服务器仅绑定 `127.0.0.1`，Windows 回环不触发防火墙提示。
- Token 校验阻止本机其它进程伪造事件；Token 只存在于设置库与工具配置文件中，前端不回显。
- 配置写入遵守第 8.2 节 playbook：不改坏用户配置、可恢复、幂等、归属明确。
- 服务器不持久化原始载荷，只保留限流所需 (tool, session, 时间) 缓存。
- 载荷只含最小元数据；`last_assistant_message` 由适配层截断，AI 模式默认可关闭。

## 13. 测试与验收

### 单元测试（Rust）

- `hook_server`：正常 POST、错误路径、超长体、坏 JSON、错误 Token、仅接受 POST。
- `tool_hook`：模板占位符替换；提醒间隔去重；去抖计时；日限跨天重置。
- 固定/AI 模式消息字段正确（`trigger_type="tool_hook"`）。
- `tool_hook_config`：插件文件写入/幂等/他人文件备份/NeedsUpdate 检测/删除恢复（用临时目录模拟 `~/.config/opencode/plugins`）。

### 集成测试

- 向本机端口 POST 模拟 opencode 载荷 → 验证消息落库、`assistant-message` 事件、语音调度被调用。
- 配置管理：先写入再删除，`%USERPROFILE%\.config\opencode\plugins` 恢复原状。

### 手工验收

1. 设置页启用 → 下拉选择 opencode → 状态"未配置" → 一键配置 → "已配置"。
2. 重启 opencode → 跑一个任务 → 桌宠气泡出现提示。
3. 固定模式不产生 LLM 调用；AI 模式载荷带工具/项目信息。
4. 连续多轮执行 opencode：按提醒间隔限流，不刷屏。
5. 修改端口后状态变"需更新"，重新一键配置生效。
6. 关闭桌宠再跑 opencode：插件静默失败，无异常。
7. 删除配置：插件文件消失，原用户文件被覆盖场景下恢复备份。
8. `/health` 与模拟事件测试可用。

## 14. 后续扩展（预留）

- **新增配置项**：同一工具下加枚举值即可（如"开始执行提示""出错提示"），UI 列表自动渲染。
- **Claude Code**：`~/.claude/settings.json` 的 `hooks.Stop`/`SessionEnd`，`type:"http"` 直连，JSON 安全合并。
- **Codex**：`~/.codex/hooks.json`（command 型脚本 curl）或 `config.toml` 的 `notify`。
- **Aider**：`~/.aider.conf.yml` 的 `notifications_command`。
- 事件协议与 Claude Code `Stop`/`Notification(agent_completed)` 对齐，天然兼容后台会话完成。

## 15. 决策点（待确认）

1. HTTP 实现：推荐手写最小监听器（轻依赖）；若偏好健壮性可用 axum。→ 默认按推荐。
2. opencode 插件写**全局目录**（`~/.config/opencode/plugins/`，所有项目生效，写入一次即可）→ 已按用户选择定稿。
3. 去抖默认 0（每轮 idle 即触发，靠**提醒间隔默认 10 分钟**挡掉交互聊天噪音）；间隔与去抖均可配置。→ 已按用户反馈定稿。
4. Token 校验默认开启，提供关闭开关。
5. 设置页顶部标签页（常规 / 编程联动）→ 已按用户选择定稿。
6. 配置管理默认只支持 opencode + `TaskDone` 一个配置项；多配置项结构已就位。
