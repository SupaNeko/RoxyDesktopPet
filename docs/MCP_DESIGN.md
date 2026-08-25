# MCP 接入设计

本文档描述洛琪希桌宠的 MCP（Model Context Protocol）接入架构：服务器管理、
工具注入链路、生命周期规则与排错方法。实现参考官方 Rust SDK（`rmcp` crate）。

## 1. 总览

```
设置页（MCP 服务 tab）
   │  add/update/remove/set_enabled（Tauri commands）
   ▼
SQLite mcp_servers 表  ──►  McpState（src-tauri/src/mcp.rs）
                               │  stdio：启动时拉起子进程；remote：启用即连接
                               ▼
                     available_tools()（仅 已启用 && 已连接）
                               │
        ┌──────────────────────┼───────────────────────────┐
        ▼                      ▼                           ▼
  待办审计循环            主聊天系统提示词              旁路任务代理
  run_tool_audit         （能力清单 + 两种            agent.rs
  （简单动作后台执行）     调用方式说明）              （复杂任务、需反馈）
```

关键原则：**只有已启用且已连接的服务器才会把工具注入 AI**；启用开关即时生效。

## 2. 两类服务器与生命周期

| 类型 | transport | 连接时机 | 运行期新增/修改 | 停用行为 |
|------|-----------|----------|------------------|----------|
| 本地命令 | `stdio` | 应用启动时拉起子进程 | 标记 `requires_restart`，重启后生效 | 进程保留，仅停止注入 |
| 远程服务 | `remote`（Streamable HTTP） | 启用时立即连接（动态载入） | 立即按新配置重连 | 立即断开 |

- stdio 子进程带 `CREATE_NO_WINDOW`（不弹控制台）；连接句柄 cancel/drop 时子进程随之退出。
- 应用退出（托盘「退出」）时 `mcp::shutdown` 断开全部连接。
- 连接超时：stdio 30s、remote 15s；工具调用超时 120s，结果超过 4000 字符截断。

## 3. 工具注入：两种触发方式

MCP 工具以 `mcp__{server}__{tool}` 前缀名（清洗为 OpenAI 允许的字符，≤64）暴露，
schema 原样透传。主聊天保持「一次必答」契约（强制 `reply_to_user`），工具循环只发生在两条旁路：

1. **简单任务、无需反馈**（如「帮我播放音乐」）
   待办审计循环 `run_tool_audit` 的 tools 里追加 MCP 工具。审计系统提示词规定：
   仅当请求是简单、可立即完成、不需要向用户汇报结果的动作时才直接调用；
   不创建待办、不输出文字。

2. **复杂任务、需要反馈**（如「今天天气怎么样」）
   - 主聊天正常先回复（如「我查一下，稍后告诉你」），同时在 `reply_to_user` 的
     `follow_up_task` 字段填写委派任务；
   - `process_message` 落一条 `agent_runs`（status=running）并 `agent::schedule`；
   - 代理（`src-tauri/src/agent.rs`）带全部可用 MCP 工具跑 ≤8 轮工具循环，产出中文结论；
   - 结论落库（status=done）后，以 `agent_followup` 触发 `generate_scheduled_message`，
     把「任务 + 结果」注入上下文，由主聊天转告用户（气泡 + 语音链路复用）；
   - 近 3 条 `agent_runs` 摘要持续注入主聊天系统提示词，作为后续对话上下文。

主聊天系统提示词在有可用 MCP 工具时动态追加「可用工具与任务委派」段落，
明确上述两种方式的用法与边界（没有把握用到工具时不要委派）。

## 4. 数据表

```sql
mcp_servers(id, name UNIQUE, transport('stdio'|'remote'),
            command, args, env, url, headers, enabled, created_at, updated_at)
agent_runs(id, task, status('running'|'done'|'error'), summary, created_at, finished_at)
```

- `args`：空格分隔，或 JSON 数组（保留含空格参数）；`env` / `headers`：每行一条 `KEY=VALUE`（本地存储，与 ASR Key 同等处理）。
- `headers`：远程 MCP 的自定义请求头（如 `X-Caiyun-API-Key=…`），随每次请求发送。
- 名称唯一，重名报错「已存在同名 MCP 服务器」。

### JSON 导入

表单与 JSON 两种配置方式等价。JSON 模式接受裸服务器对象或标准 `mcpServers` 包装（取第一个条目，键作默认名称）：

```json
{ "url": "https://mcp-weather.caiyunapp.com/mcp", "headers": { "X-Caiyun-API-Key": "YOUR_KEY" } }
{ "mcpServers": { "filesystem": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "D:\\work"], "env": { "FOO": "bar" } } } }
```

含 `url` 视为远程、含 `command` 视为本地；名称取自 `name` 字段或包装键。解析在后端完成（`parse_mcp_config_json`），`config_json` 非空时忽略表单其余字段。

## 5. Tauri 命令与事件

- `list_mcp_servers` / `add_mcp_server` / `update_mcp_server` / `remove_mcp_server`
  / `set_mcp_server_enabled(id, enabled)` / `list_mcp_server_tools(id)`
- 状态徽标：`connected`（含工具数）/ `disabled` / `pending_restart`（重启后生效）/ `error`（含原因）。
- 每次变更 emit `mcp-servers-changed`，设置页监听后刷新列表。

## 6. 设置页

设置 → 「MCP 服务」tab：服务器卡片列表（名称、本地/远程徽标、状态徽标）、
启用开关（即时生效，不走底部「保存」）、查看工具、编辑、删除、新增表单
（远程填 URL + 请求头；本地填命令/参数/环境变量；或切换 JSON 导入直接粘贴标准 MCP 配置）。

## 7. 排错

- 卡片显示「连接失败」：鼠标查看错误文本；同时查 `data/logs/` 下应用日志中的
  `MCP 服务器「…」连接失败` 行。
- 本地命令常见原因：命令不在 PATH（用绝对路径）、参数含空格（当前按空格切分，
  含空格路径暂不支持）、服务器未实现 stdio 传输。
- 「重启后生效」：本地 MCP 新增/修改配置后的预期状态，重启应用即可。
- 工具调不通：在卡片上点「查看工具」确认已发现工具；代理任务结论可在日志中按
  `agent task` 关键字检索。
