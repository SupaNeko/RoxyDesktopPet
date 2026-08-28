# AGENTS.md

> 本文面向 AI 编码代理，提供 RoxyDesktopPet 项目的架构、构建与协作约定。阅读本文前不需要任何项目背景知识。
> 项目文档与代码注释均以**中文**为主要语言，修改代码和文档时请沿用中文。

## 1. 项目概览

RoxyDesktopPet 是一个**仅限 Windows x64** 的本地优先（local-first）AI 桌面陪伴应用，绑定固定角色「洛琪希」（Roxy），提供文字交流、语音输入和可选的本地语音输出。项目不是通用角色平台，也不以生产力工具为目标。

核心功能：

- 以洛琪希人设进行中文回复（OpenAI-compatible Chat Completions API）。
- 文字输入、持续语音输入和全局「按住说话」（科大讯飞语音听写 WebAPI 做 ASR）。
- 主动对话、待办提醒（SQLite 可靠调度）。
- 可选 GPT-SoVITS v2ProPlus 本地日语语音输出（独立扩展包，需 NVIDIA GPU，不在安装包内）。
- 基于本地 SQLite + Embedding API 的长期记忆（观察者模式，**不依赖** Qdrant/PostgreSQL/Redis/Python 记忆服务；README 与部分旧文档中提到的 Qdrant 方案已被 `docs/local-memory-design.md` 的 Rust + SQLite 方案取代）。
- MCP 服务器接入（rmcp crate）、编程工具完成提醒（Tool Hook）、只读系统感知（任务栏应用 / 硬件占用 / SMTC 当前播放）、联网搜索工具。

**命名说明**：仓库名/产品名/包名已统一为 `RoxyDesktopPet`（npm 包与 Cargo 包名为 `roxydesktoppet`，Tauri identifier 为 `com.roxydesktoppet.desktop`，Rust lib 为 `roxydesktoppet_lib`，二进制名为 `RoxyDesktopPet`）。历史名为 `chatpet`，启动时会自动将旧数据库 `chatpet.db` 迁移为 `roxydesktoppet.db`；更早文档中残留的「ChatPet」均指本项目。

## 2. 技术栈

| 层 | 技术 |
| --- | --- |
| 前端 | Svelte 5（runes 语法，`$state`/`$derived`）+ TypeScript + Vite 6，`lucide-svelte` 图标 |
| 桌面壳 | Tauri 2（WebView2），包管理用 pnpm |
| 后端 | Rust 2021（rust-version 1.80），tokio 异步运行时 |
| 数据 | SQLite（rusqlite bundled），单文件 `data/roxydesktoppet.db` |
| 音频 | cpal（采集）、rodio（播放）、voice_activity_detector（VAD）、tokio-tungstenite（讯飞 ASR WebSocket） |
| 网络 | reqwest（LLM/Embedding/TTS）、rmcp（MCP 客户端，stdio + Streamable HTTP） |
| Windows 集成 | windows-sys / windows crate（托盘、全局输入钩子、命中测试、SMTC、DWM）、sysinfo（硬件占用） |

## 3. 目录结构与模块划分

```text
├─ src/                     # Svelte 前端（刻意保持极简）
│  ├─ App.svelte            # 唯一视图组件：按窗口 label / ?view= 参数切换桌宠、菜单、设置、待办、历史会话
│  ├─ lib/api.ts            # Tauri invoke 封装；非 Tauri 环境（纯浏览器 dev）返回演示数据
│  └─ lib/types.ts          # 与 Rust 端对应的类型定义
├─ src-tauri/
│  ├─ src/
│  │  ├─ main.rs            # 入口，仅调用 roxydesktoppet_lib::run()
│  │  ├─ lib.rs             # 应用装配：.env 读取、状态注册、托盘、窗口事件、全部 Tauri commands 注册
│  │  ├─ commands.rs        # Tauri IPC 命令层（最大文件，~1700 行）
│  │  ├─ db.rs              # SQLite schema 与全部存储操作（settings/messages/todos/memories/mcp_servers/agent_runs 等）
│  │  ├─ audio.rs / asr.rs  # 麦克风采集 + VAD 状态机 + 讯飞 ASR
│  │  ├─ voice_output.rs    # 语音输出编排：翻译 → GPT-SoVITS / TTS API → 播放队列
│  │  ├─ gpt_sovits.rs      # GPT-SoVITS 本地推理子进程生命周期管理
│  │  ├─ memory.rs / memory_store.rs / observer.rs  # 长期记忆：观察者提取→检索→决策（带纠错循环）；独立用户偏好总结观察者（ReAct 工具循环，无条件注入系统提示词）
│  │  ├─ agent.rs           # 旁路任务代理（复杂 MCP 工具循环）
│  │  ├─ mcp.rs             # MCP 服务器连接与工具注入
│  │  ├─ scheduler.rs       # 待办提醒与主动消息调度
│  │  ├─ tool_hook.rs / tool_hook_config.rs / hook_server.rs  # 编程工具（Claude Code/Codex/opencode）完成提醒
│  │  ├─ system_monitor.rs / media_control.rs  # 只读系统感知（任务栏、硬件、SMTC）
│  │  ├─ search.rs          # 联网搜索工具
│  │  ├─ global_input.rs / native_hit_test.rs / pet_interaction.rs / fullscreen_watch.rs  # 桌面交互
│  │  └─ logger.rs          # log_info!/log_warn!/log_error! 宏（脱敏日志，写入 data/logs/）
│  ├─ tauri.conf.json       # 5 个窗口定义（pet / pet-menu / settings / todos / history）、CSP、NSIS 打包配置
│  └─ capabilities/default.json  # Tauri 权限
├─ docs/                    # 设计文档（见第 8 节）
├─ scripts/                 # PowerShell：语音扩展包构建与 GPT-SoVITS 开发环境准备
├─ public/                  # 洛琪希 8 种情绪 PNG（构建时复制到 dist/）
├─ data/                    # 运行时数据（roxydesktoppet.db、日志、窗口位置），gitignored
├─ voice/                   # GPT-SoVITS 语音扩展（运行时、模型、参考音频），gitignored
├─ tools/                   # 美术素材处理脚本（Python/Node），gitignored
└─ artifacts/               # 发布产物（语音扩展 ZIP），gitignored
```

架构要点：

- **UI 只通过 Tauri `invoke` 和事件订阅访问后端**，不持有明文 API Key，不直接请求模型 API。
- 所有可靠状态（消息、待办、记忆、提醒 occurrence）写入 SQLite，不只存在内存中。
- 同一时刻只允许一个主 LLM 工具循环（`ConversationState` 互斥锁）；P0 提醒 > P1 用户交互 > P2 主动消息。
- 语音输出是可选能力门控：任何环节缺失（扩展包、GPU、凭据）都降级为纯文本，**不得影响主对话链路**。
- `.env` 读取位置：开发时读项目根目录，发布版读 exe 同目录（见 `lib.rs` 中 `cfg!(debug_assertions)` 分支）；`data_dir()` 同理。

## 4. 构建与开发命令

环境要求：Node.js、pnpm、Rust stable、Microsoft Edge WebView2 Runtime。

```powershell
Copy-Item .env.example .env   # 首次；填入大模型 / 讯飞 ASR / Embedding 凭据
pnpm install
pnpm check                    # svelte-check 类型检查（改动前端后必跑）
pnpm build                    # vite build → dist/
pnpm tauri dev                # 开发模式（自动先起 vite，端口固定 127.0.0.1:1421）
```

Rust 单独检查与测试：

```powershell
cd src-tauri
cargo check
cargo test                    # 单元测试以 #[cfg(test)] 模块内联在各 .rs 文件中
```

## 5. 测试策略与现状

- **Rust 单元测试为主**：约 35 个 `#[test]`/`#[tokio::test]`，分布在 `asr.rs`、`audio.rs`、`commands.rs`、`db.rs`、`hook_server.rs`、`memory.rs`、`memory_store.rs`、`search.rs`、`system_monitor.rs`、`voice_output.rs` 的 `#[cfg(test)] mod tests` 中。改动这些模块时应运行 `cargo test` 并为纯逻辑函数补充测试。
- **前端目前没有任何自有的 vitest 测试**。注意：`pnpm test`（`vitest run`，无 vitest 配置文件）会误扫 `voice/runtime/` 里 gradio 的 vendored 测试并失败——这是已知现状，不代表项目测试失败。若要给前端加测试，应先添加 vitest 配置排除 `voice/`、`dist/` 等目录。
- `docs/DEVELOPMENT.md` 第 13 节描述了更完整的测试策略（集成测试、Windows 专项验收），多数属于规划，未全部落地。
- 无 CI/CD：仓库没有 `.github/workflows` 或其他流水线配置。

## 6. 代码风格与约定

- **注释、日志、文档一律使用中文**；代码标识符使用英文。
- 前端是单文件 `App.svelte`（~620 行），刻意保持紧凑；新增功能优先考虑在现有视图/标签页内扩展，不要贸然拆分路由框架。
- Rust 各模块为扁平单文件，不建子目录；新功能先评估能否归入现有模块。
- 日志使用 `logger.rs` 的 `log_info!/log_warn!/log_error!` 宏，**不得记录 API Key**；完整 Prompt、ASR 音频、翻译文本默认不写日志。
- 与 LLM 的 JSON 交互必须做容错解析（允许 Markdown fence 包裹），工具参数由 Rust 端校验，模型不得直接写数据库。
- 设置项的增改是跨层工作：`db.rs`（schema/默认值）→ `commands.rs`（save/get）→ `src/lib/types.ts` + `src/lib/api.ts` → `App.svelte`（设置页 UI），四层需同步。

## 7. 打包与发布

```powershell
pnpm tauri build   # 输出 NSIS 安装包到 src-tauri/target/release/bundle/nsis/（currentUser 安装模式）
```

语音扩展包（仅限已备好 GPT-SoVITS 源码、固定 Python/CUDA 运行时与洛琪希模型的机器）：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build-gpt-sovits-voice-bundle.ps1   # 产物在 artifacts/
```

- release profile 开启 `debug = true`：保留调试符号以便从 .dmp 反解崩溃堆栈，不影响优化等级。
- 发布版运行时目录约定：exe 同目录下的 `.env`、`data/`、`voice/manifest.json`（语音扩展解压后不嵌套额外目录）。

## 8. 文档索引

| 文件 | 内容 |
| --- | --- |
| `README.md` | 项目定位、外部服务凭据配置、开发与打包步骤 |
| `docs/REQUIREMENTS.md` | 需求规格（产品目标、非目标、隐私边界） |
| `docs/DEVELOPMENT.md` | 总体设计文档（模块架构、音频链路、调度、测试策略） |
| `docs/PRODUCT_AND_VOICE_ARCHITECTURE.md` | 产品边界与语音全链路架构 |
| `docs/local-memory-design.md` | 当前长期记忆方案（Rust + SQLite，取代旧 Qdrant 设计） |
| `docs/MCP_DESIGN.md` | MCP 接入架构（服务器生命周期、工具注入两条旁路） |
| `docs/TOOL_HOOK_DESIGN.md` | 编程工具联动提醒设计 |
| `docs/TODO.md` | 待办与已知限制 |

注意文档时效性：`DEVELOPMENT.md` 等早期文档中的 Qdrant、VITS EXE、目录结构等描述是历史方案，与现状冲突时**以代码和 `local-memory-design.md`、`MCP_DESIGN.md` 等较新文档为准**。

## 9. 安全与隐私注意事项

- `.env` 含 API 凭据，**绝不提交**（已 gitignored；只有 `.env.example` 入库）。同样不入库的还有 `data/`、`voice/`、`tools/`、`artifacts/`、`*.log`。
- `tauri.conf.json` 配置了收紧的 CSP（`connect-src 'self'` 等），改动前端网络请求前先看它。
- 项目明确承诺的能力边界（见 `REQUIREMENTS.md` 非目标）：不读用户文件、不执行命令、不截屏、不读剪贴板/浏览器历史。新增功能不得突破这些边界；系统感知（任务栏/硬件/播放）必须有独立的用户授权开关且默认关闭。
- Tool Hook 的本地 HTTP 端口只接收事件信号，不开放控制类能力。
- 第三方素材（GPT-SoVITS 运行时、洛琪希模型、表情 PNG）的分发许可需单独审查，不因主项目许可自动获得分发权。
- 不要执行 `git commit`/`push` 等变更操作，除非用户明确要求。
