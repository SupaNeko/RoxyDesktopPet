# RoxyDesktopPet 开发设计文档

> 对应需求：`REQUIREMENTS.md`  
> 推荐技术基线：Tauri 2 + Svelte 5 + Rust + SQLite + Qdrant  
> 目标平台：Windows x64

## 1. 设计结论

RoxyDesktopPet 推荐沿用 AgentStage 的桌面技术栈和后端边界：Svelte 只负责界面，Rust/Tauri 后端持有 API Key、数据库、音频状态、调度器、Qdrant 客户端和 VITS 子进程。这样可以直接迁移 AgentStage 已验证的 OpenAI-compatible Provider、DPAPI/加密方式、SQLite 模式以及 VITS Runtime 管理代码。

不建议第一版再引入 Python Web 服务。Silero VAD 使用 ONNX Runtime 在 Rust 后端推理；VITS 继续保持现有独立 EXE，但属于可选组件。缺少 EXE 时应用以纯文本模式正常运行。主程序进程关系如下：

```text
RoxyDesktopPet.exe
├─ WebView2/Svelte UI
├─ Rust Core
│  ├─ Audio Capture + Silero VAD
│  ├─ Conversation Orchestrator
│  ├─ SQLite + Scheduler
│  ├─ Qdrant Client
│  └─ API Providers
├─ qdrant.exe（若采用随应用托管模式）
└─ vits_runtime.exe（可选；存在且启用时按需启动，模型常驻）
```

## 2. 参考实现与复用边界

### 2.1 AgentStage：直接复用优先

已审查的实现位于：

- `D:\code_project\AgentStage\src-tauri\src\vits\protocol.rs`
- `D:\code_project\AgentStage\src-tauri\src\vits\runtime.rs`
- `D:\code_project\AgentStage\src-tauri\src\llm\translate.rs`
- `D:\code_project\AgentStage\src-tauri\src\commands\voice.rs`
- `D:\code_project\AgentStage\vits_runtime\main.py`

可复用内容：

1. `VitsRequest`、`VitsResponse`、`VitsPingResponse` 数据结构。
2. 持久子进程、启动就绪等待、stdin/stdout 单行 JSON、通信失败后重启的生命周期管理。
3. Windows `CREATE_NO_WINDOW` 启动方式和 UTF-8 编码设置。
4. 翻译 Prompt 中的人设、关系、相关记忆注入方式。
5. 翻译响应的 JSON 容错解析。
6. VITS 模型扫描、speaker 解析、消息 ID 音频缓存与使用量分类思路。

需要修改的部分：

- AgentStage 从角色数据库和会话关系中组装上下文；RoxyDesktopPet 改为单角色配置、用户关系和 Qdrant 相关记忆。
- AgentStage 缓存按 session/message 组织；RoxyDesktopPet 可按 `conversation_id/message_id` 组织。
- RoxyDesktopPet 的语音生成由编排器自动触发，不只依赖消息上的手动播放按钮。
- `stderr` 不建议直接丢弃，应接入大小受限且脱敏的运行时日志，便于定位模型加载失败。
- VITS 10 分钟超时可保留为上限，但 UI 应在更短时间给出“仍在合成”状态，并允许取消过期的非可靠语音任务。

### 2.2 其他参考项目：只参考模式

- Open-LLM-VTuber：参考语音状态机、回复/播放状态与主动发言编排，不整体引入。
- Silero VAD：使用 ONNX 模型和流式状态概念。
- valerieliang/desktop-pet：参考持久化提醒、重启恢复和桌面窗口体验。
- DesktopPetLive2D：参考人格、Todo、主动消息和 Windows 活动事件模块边界；不引入 Live2D 依赖。

在正式复制第三方代码前必须逐项确认许可证与 NOTICE；优先自行实现接口层，只迁移 AgentStage 自有且许可证明确的代码。

## 3. 推荐目录结构

```text
RoxyDesktopPet/
├─ README.md
├─ docs/
│  ├─ REQUIREMENTS.md
│  └─ DEVELOPMENT.md
├─ src/                         # Svelte UI
│  ├─ lib/components/
│  ├─ lib/stores/
│  ├─ lib/types/
│  └─ routes-or-views/
├─ src-tauri/
│  ├─ src/
│  │  ├─ audio/                 # WASAPI、重采样、VAD、切句
│  │  ├─ commands/              # Tauri IPC
│  │  ├─ conversation/          # 编排器、Prompt、工具循环
│  │  ├─ db/                    # SQLite schema/migrations/repos
│  │  ├─ events/                # 内部事件总线
│  │  ├─ llm/                   # LLM/ASR/Embedding provider
│  │  ├─ memory/                # 提取、Qdrant、召回、去重
│  │  ├─ scheduler/             # Todo 与主动消息
│  │  ├─ system_events/         # Windows 低优先级事件
│  │  ├─ vits/                  # AgentStage 兼容协议
│  │  └─ lib.rs
│  └─ migrations/
├─ resources/
│  ├─ vad/silero_vad.onnx
│  └─ default-avatar.png
└─ data/                        # 运行时创建，不入库
   ├─ roxydesktoppet.db
   ├─ qdrant/
   ├─ vits_runtime/
   ├─ vits_models/
   ├─ vits_cache/
   └─ logs/
```

## 4. 模块架构

### 4.1 UI 层

窗口建议拆为：

- `pet`：透明桌宠和最近气泡。
- `chat`：对话历史、输入框、监听开关。
- `settings`：模型、人格、音频、记忆和主动消息配置。
- `todos`：待办、提醒历史和失败重试。
- `memory`：长期记忆管理。

UI 只通过 Tauri `invoke` 和事件订阅访问后端，不持有明文 API Key，不直接请求模型 API。

### 4.2 内部事件总线

统一事件类型：

```rust
enum AppEvent {
    UserTextSubmitted { text: String },
    VoiceUtteranceReady { utterance_id: Uuid, wav: Vec<u8> },
    AsrCompleted { utterance_id: Uuid, text: String },
    ReminderDue { occurrence_id: Uuid, todo_id: Uuid },
    CompanionTick { tick_id: Uuid },
    LlmCompleted { request_id: Uuid, message_id: Uuid },
    VoiceReady { message_id: Uuid, path: PathBuf },
    PlaybackStarted { message_id: Uuid },
    PlaybackFinished { message_id: Uuid },
}
```

事件总线负责解耦采集、模型、调度和播放，但可靠状态仍写入 SQLite，不能只存在内存 channel。

### 4.3 优先级与并发

```text
P0 reliable：到期提醒
P1 interactive：用户文字/语音
P2 ambient：主动消息、系统事件
```

- 同一时刻只允许一个主 LLM 工具循环。
- P1 到来时取消尚未提交的 P2。
- P0 不丢弃，但可以等待当前 P1 完成后立即执行。
- 翻译/VITS 在主回复落库和显示后进入独立队列，不阻塞主对话锁。
- VITS Runtime 当前协议一次请求一响应，天然串行；需要用单 worker 队列保护。

## 5. 音频输入设计

### 5.1 数字链路隔离

Windows 音频设备分为 capture endpoint 和 render endpoint：

```text
麦克风 capture endpoint ──> RoxyDesktopPet Capture ──> VAD ──> ASR

RoxyDesktopPet WAV ──> render endpoint ──> 扬声器/耳机
其他应用 ─────> render endpoint ──> 扬声器/耳机
```

实现约束：

- 输入模块只使用 `eCapture` 设备。
- 不使用 `AUDCLNT_STREAMFLAGS_LOOPBACK`。
- 不打开 Stereo Mix/立体声混音等回录设备；默认过滤已知回录端点名称，并允许用户查看实际选择。
- 播放器只获得 WAV 路径或 PCM 输出，不向输入 channel 发布数据。
- 因需求明确排除扬声器经空气被麦克风拾取的场景，MVP 不实现 AEC。

Rust 可选实现：`windows` crate 直接调用 WASAPI，或使用成熟音频 crate 完成采集/播放。无论选择何种库，都应通过端点角色测试证明没有 Loopback。

### 5.2 采集线程

- 专用实时采集线程读取设备帧。
- 转为 16 kHz mono f32/PCM16。
- 通过固定容量 ring buffer 传给 VAD worker。
- UI、网络和磁盘操作禁止发生在音频回调线程。
- 缓冲区满时丢弃最旧的空闲帧并记录计数，不允许无限增长。

### 5.3 VAD 容错状态机

```text
Idle
  └─ 连续 speech >= start_hold_ms ─> Speaking
Speaking
  ├─ speech/短暂停顿 ─────────────> Speaking
  ├─ silence >= end_silence_ms ───> Finalizing
  └─ duration >= max_utterance_ms ─> Finalizing(forced)
Finalizing
  ├─ 校验通过 ───────────────────> ASR Queue
  └─ 校验失败 ───────────────────> Discard
ASR Queue / Discard
  └─ cooldown_ms ─────────────────> Idle
```

容错规则建议：

- 双阈值滞回：开始阈值高于维持阈值，减少边界抖动。
- 保存约 300 ms pre-roll，避免吃掉首字。
- 句中短静音不切句；默认 900 ms 才结束。
- 最短时长与有效语音帧占比双重过滤。
- 对强爆音、纯静音、过低 RMS 直接丢弃。
- 强制截断的 30 秒片段标记 `forced_end=true`，ASR 后可提示用户继续。
- ASR 文本标准化后，用 `(normalized_text, time_window)` 去重。

### 5.4 ASR Provider

```rust
#[async_trait]
trait AsrProvider {
    async fn transcribe(&self, req: AsrRequest) -> Result<AsrResult, AsrError>;
}

struct AsrRequest {
    utterance_id: Uuid,
    wav_bytes: Vec<u8>,
    language: String, // "zh"
}

struct AsrResult {
    text: String,
    confidence: Option<f32>,
    duration_ms: u64,
}
```

第一实现支持 OpenAI-compatible multipart transcription。供应商若不返回 confidence，则依赖 VAD 质量、文本规则和重复过滤，不伪造置信度。

## 6. 对话与工具调用

### 6.1 Prompt 组成

```text
系统安全边界
+ 角色详细人设
+ 与用户的关系
+ 当前时间和时区
+ 相关长期记忆 Top-K
+ 当前触发类型说明
+ 最近对话窗口
+ 当前用户输入/提醒事件
+ Todo 工具定义
```

提醒场景增加：任务标题、原定时间、当前时间、迟到时长、已提醒次数。主动消息场景增加：安静时段验证结果和最近主动消息摘要，但不应虚构未提供的电脑状态。

### 6.2 受限工具

```text
create_todo(title, due_at, timezone, recurrence?)
update_todo(id, patch)
complete_todo(id)
delete_todo(id)
list_todos(status?, time_range?)
remember(text, type, importance?)       # 可选显式记忆
forget_memory(id)
```

所有参数由 Rust JSON Schema 校验。工具循环设置最大轮数，例如 4；超过上限终止并返回可见错误。模型不得直接写数据库。

### 6.3 回复落库与语音派生

主回复的顺序：

1. LLM 产出中文。
2. 持久化 assistant message。
3. 通过 Tauri event 立即显示中文。
4. 检查语音输出开关和 VITS EXE 可用性；条件不满足则以纯文本正常结束。
5. 条件满足时创建 `voice_job`，快照保存人设版本和相关记忆 ID。
6. 翻译成功后调用 VITS。
7. WAV 生成后更新缓存并进入播放队列。

如果用户在第 3～7 步之间产生新消息，旧的普通主动消息语音可取消；用户对话和可靠提醒由配置决定是否继续播放。

## 7. Todo 与可靠调度

### 7.1 不依赖内存定时器作为事实源

本地调度器每隔短周期查询数据库中的下一批到期 occurrence，也可维护最近定时器用于降低延迟。数据库状态决定是否应提醒。

```text
Pending -> Claimed -> LlmGenerating -> Delivered
                  ├-> RetryWaiting
                  └-> FallbackDelivered
```

通过事务更新 `Pending -> Claimed` 并写入 lease，防止同一 occurrence 重复消费。崩溃后过期 lease 可被恢复任务重新领取。

### 7.2 建议表

```sql
CREATE TABLE todos (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  due_at_utc INTEGER NOT NULL,
  timezone TEXT NOT NULL,
  recurrence_rule TEXT,
  status TEXT NOT NULL,
  source_message_id TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE reminder_occurrences (
  id TEXT PRIMARY KEY,
  todo_id TEXT NOT NULL,
  scheduled_at_utc INTEGER NOT NULL,
  status TEXT NOT NULL,
  attempt_count INTEGER NOT NULL DEFAULT 0,
  lease_until INTEGER,
  delivered_at INTEGER,
  last_error TEXT,
  UNIQUE(todo_id, scheduled_at_utc)
);
```

### 7.3 模型失败降级

```text
LLM 成功：展示角色化提醒并派生日语语音
LLM 超时/限流：有限重试，同时立即展示“提醒：{title}”
VITS 失败：保留文本提醒和 Windows 通知
应用离线：启动恢复后按补发策略处理
```

“可靠”指提醒事实不丢失，不代表第三方模型或语音永远成功。

## 8. 主动消息调度

配置：

```json
{
  "enabled": true,
  "min_interval_minutes": 45,
  "max_interval_minutes": 120,
  "daily_limit": 6,
  "quiet_hours": { "start": "23:00", "end": "08:00" }
}
```

下次触发时间在每次成功或跳过后重新随机。触发前检查：

- 是否安静时段；
- 最近是否有用户交互；
- 当前是否有 P0/P1 调用；
- 今日是否达到上限；
- 距离上次主动消息是否足够；
- 应用是否可见/系统是否锁屏（事件模块可用后）。

主动消息不建立可靠 occurrence，不在重启后补发。

## 9. Qdrant 长期记忆设计

### 9.1 部署

开发阶段可用 Docker；产品阶段建议评估随应用管理的 `qdrant.exe` 与本地数据目录。Qdrant 仅绑定 `127.0.0.1`，不暴露局域网端口。启动前检查端口占用和数据版本，退出时优雅停止由应用启动的实例。

### 9.2 Collection

名称建议包含 embedding 身份：

```text
roxydesktoppet_memories_{provider}_{model}_{dimension}_{distance}
```

示例 payload：

```json
{
  "memory_id": "uuid",
  "character_id": "uuid",
  "text": "用户通常在周三晚上游泳",
  "type": "habit",
  "importance": 0.72,
  "confidence": 0.91,
  "status": "active",
  "source_message_id": "uuid",
  "created_at": 1787313600000,
  "updated_at": 1787313600000
}
```

建议为 `character_id`、`type`、`status` 建 payload index。

### 9.3 SQLite 与 Qdrant 一致性

SQLite 是记忆业务事实源：

```sql
CREATE TABLE memories (
  id TEXT PRIMARY KEY,
  character_id TEXT NOT NULL,
  text TEXT NOT NULL,
  memory_type TEXT NOT NULL,
  importance REAL NOT NULL,
  confidence REAL NOT NULL,
  status TEXT NOT NULL,
  embedding_collection TEXT,
  embedding_point_id TEXT,
  embedding_status TEXT NOT NULL,
  source_message_id TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
```

写入采用 outbox/状态补偿，而不是假设 SQLite 与 Qdrant 跨库事务：

1. SQLite 插入记忆，`embedding_status=pending`。
2. 调用 Embedding API。
3. Upsert Qdrant point。
4. SQLite 更新为 `ready`。
5. 后台任务重试 pending/failed。

删除先将 SQLite 设为 inactive，再删除 Qdrant point；即使 Qdrant 删除失败，该记忆也不会被应用层注入。

### 9.4 召回

1. 对当前输入调用 Embedding API。
2. 在 Qdrant 按 `character_id`、`status=active` 过滤检索，例如 Top 12。
3. 按相似度、importance、时效性做轻量重排。
4. 去重并限制 token，总计注入约 3～8 条。
5. 记录被召回的 memory ID，便于调试和用户解释。

基础评分可从以下形式开始：

```text
final = 0.70 * similarity + 0.20 * importance + 0.10 * recency
```

不要在未评估前堆叠知识图谱或复杂 RAG。

### 9.5 记忆提取

每次完整对话后异步调用记忆提取 Prompt，要求结构化 JSON：

```json
{
  "memories": [
    {
      "text": "...",
      "type": "preference|fact|habit|relationship|experience",
      "importance": 0.0,
      "confidence": 0.0,
      "operation": "add|update|ignore",
      "target_memory_id": null
    }
  ]
}
```

写入前对候选文本 embedding 并搜索近邻：高相似候选进入更新/合并判断，避免同义记忆无限增长。低 confidence 或模型推测内容不写入。

## 10. 翻译与 VITS 设计

### 10.1 翻译调用

沿用 AgentStage 设计：这是独立的 LLM 调用，不进入对话历史。该调用仅在语音输出已启用且 VITS EXE 可用时发生；纯文本模式不得为无后续用途的日语文本支付翻译调用成本。输入包括：

- 原始中文显示文本；
- 目标语言 `ja`；
- 当前角色人设版本；
- 与用户的关系；
- 本次主回复实际使用的相关记忆；
- 标点和句子边界约束。

返回：

```json
{"need_translate": true, "translated_text": "..."}
```

解析允许外层 Markdown fence，但最终必须通过 JSON schema 和非空校验。翻译失败时记录 voice job 失败，不回滚中文消息。

### 10.2 VITS 协议

启动后：

```json
{"ready": true, "version": "1.0.0"}
```

请求：

```json
{
  "action": "generate",
  "text": "もう九時だよ。",
  "model_path": "D:\\...\\vits_models\\model-a",
  "speaker_id": "speaker-name",
  "emotion_params": "{\"noise\":0.667}",
  "speed": 1.0,
  "target_language": "ja",
  "output_path": "D:\\...\\vits_cache\\message-id.wav"
}
```

成功响应：

```json
{"success": true, "output_path": "...", "duration_ms": 1234}
```

协议要求：stdout 只能输出协议 JSON；日志写 stderr；每行一条消息；UTF-8；通信异常后终止子进程，下次请求重启。VITS 模型按路径常驻缓存的行为保持不变。启动时只探测 EXE 是否存在，不因缺失而返回应用级错误；只有用户启用语音且实际请求合成时才按需启动子进程。

### 10.3 可选能力门控

```text
中文消息已显示
  ├─ voice_output_enabled == false ─> 完成（纯文本）
  └─ voice_output_enabled == true
       ├─ vits_runtime.exe 不存在 ─> 完成（纯文本，设置页显示未配置）
       └─ vits_runtime.exe 存在
            └─ 创建 Voice Job → 翻译 → VITS → 播放
```

缺少 EXE 属于能力未配置，不属于运行故障。若 EXE 原本存在但启动或通信失败，则该条 voice job 标记失败并降级为文本；不得影响主对话、Todo 提醒的文本交付或后续模型调用。

### 10.4 Voice Job

```sql
CREATE TABLE voice_jobs (
  id TEXT PRIMARY KEY,
  message_id TEXT NOT NULL UNIQUE,
  status TEXT NOT NULL,
  source_text TEXT NOT NULL,
  translated_text TEXT,
  output_path TEXT,
  trigger_type TEXT NOT NULL,
  attempt_count INTEGER NOT NULL DEFAULT 0,
  last_error TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
```

状态：`pending_translate -> translating -> pending_vits -> synthesizing -> ready -> playing -> completed`，任一步可进入 `failed` 或非可靠任务的 `cancelled`。

## 11. Windows 事件模块（低优先级）

模块以 Provider 形式隔离：

```rust
trait SystemEventProvider {
    async fn start(&mut self, sink: EventSink) -> Result<()>;
    async fn stop(&mut self) -> Result<()>;
}
```

候选实现：

- Session notification：锁屏/解锁。
- `GetLastInputInfo`：空闲/恢复，仅返回持续时间。
- Power setting notification：电源、电量。
- Windows Runtime `GlobalSystemMediaTransportControlsSessionManager`：播放器和媒体元数据。

事件去抖后写入短期状态表，不直接调用 LLM：

```sql
CREATE TABLE system_state (
  key TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  observed_at INTEGER NOT NULL,
  expires_at INTEGER
);
```

### 11.1 已落地：系统感知（只读）

2026-08 已落地的子集，实现于 `system_monitor.rs` 与 `media_control.rs`：

- 任务栏应用列表 + 前台窗口：`EnumWindows` + Shell 任务栏规则过滤（`system_monitor.rs`，纯函数 `should_include_window` 便于测试），进程名经 sysinfo 按 PID 关联。
- 硬件状态：sysinfo 读 CPU/内存占用；GPU 名称/占用/显存/温度走 `nvidia-smi`（无需管理员）。CPU 温度明确不做：Windows 无可靠用户态接口（需加载内核驱动读 MSR），超出本项目范围。
- 当前播放：WinRT GSMTC 只读（`media_control.rs`）。WinRT 对象非 Send，查询在专用线程 + current_thread runtime 执行，oneshot 回传纯数据。
- 授权模型：三个独立设置开关（默认关），存储于 `app_settings`；`get_taskbar_apps` / `get_hardware_stats` / `get_now_playing` 命令未授权时直接拒绝。
- 对话注入：`process_message` 按已授权项实时读取并生成简短摘要注入系统提示词，不引入工具调用轮次、不增加 LLM 调用数；未采用上面的 `system_state` 缓存表（按读即用即可，后续若加主动事件再引入）。

## 12. API、错误与可观测性

每类 Provider 定义独立超时、重试和用量记录：

```text
chat
reminder
proactive
memory_extract
embedding_write
embedding_query
asr
tts_translate
```

日志使用 correlation ID 串起：utterance → ASR → message → LLM → translation → VITS。不得记录 API Key。完整 Prompt、ASR 音频和翻译文本默认不写日志；诊断模式必须提醒隐私风险。

健康状态：

- SQLite：必需，失败则禁止启动核心功能。
- LLM/ASR/Embedding：远端状态，按调用反馈。
- Qdrant：可降级。
- VITS：可选且可降级；未配置 EXE 时报告 `not_configured`，不报告系统错误。
- 麦克风：可关闭或失败，不影响文字对话。

## 13. 测试策略

### 13.1 单元测试

- VAD 状态机：短噪声、首字预滚动、句中停顿、最大时长、冷却。
- ASR 文本过滤和时间窗去重。
- Todo 时间解析后的结构校验、occurrence 幂等和 lease 恢复。
- Prompt 触发类型隔离。
- 翻译 JSON 容错、空结果和标点约束。
- VITS 单行协议、超时、进程退出和自动重启。
- VITS EXE 缺失时不创建翻译调用、不创建失败 Voice Job，主流程正常完成。
- 记忆去重、inactive 过滤和重建状态。

### 13.2 集成测试

- 使用录制 WAV 注入采集层，不依赖真实麦克风。
- Mock ASR/LLM/Embedding HTTP Server，验证请求次数和失败重试。
- 启动临时 Qdrant，验证 collection、payload filter、upsert/search/delete。
- 使用假的 VITS EXE/脚本模拟 ready、成功、错误、污染 stdout 和超时。
- 使用临时时钟测试到期、错过提醒和安静时段。

### 13.3 Windows 专项验收

- 枚举结果只选择 capture endpoint。
- 播放系统音频且麦克风无物理回采时，不出现输入帧。
- VITS 播放 PCM 不被软件管线送入 ASR。
- 默认输入设备切换/拔出时正确停止并提示。
- 锁屏、休眠、唤醒后调度恢复。
- 托盘退出能关闭麦克风、VITS 与由应用启动的 Qdrant 进程。

## 14. 开发阶段建议

### Phase 0：工程与契约

- 创建 Tauri/Svelte/Rust 工程。
- 建立配置、DPAPI、SQLite migration、Provider trait 和内部事件类型。
- 定义 Mock Server 与可控时钟，先写契约测试。

### Phase 1：文字 MVP

- 静态桌宠、托盘、聊天 UI。
- 人格配置、LLM Provider、消息存储。
- Todo 工具、可靠 occurrence、模型提醒与降级文本。

### Phase 2：长期记忆

- 本地 Qdrant 生命周期或连接配置。
- Embedding Provider、memory outbox、检索和管理 UI。
- 记忆提取、去重、停用和重建。

### Phase 3：输入语音

- WASAPI capture、重采样、Silero VAD ONNX。
- 端点状态机、ASR Provider、过滤与去重。
- 设备/隐私 UI 和长期空闲稳定性测试。

### Phase 4：日语语音输出

- 移植 AgentStage 翻译和 VITS 协议。
- Voice Job、缓存、播放队列和失败降级。
- 增加可选能力门控：无 EXE 时跳过翻译与合成并保持纯文本模式。
- 验证输入/输出数字链路隔离。

### Phase 5：主动陪伴与 Windows 事件

- 可配置主动消息、安静时段、每日上限。
- 媒体、idle、锁屏、电源事件；只缓存状态，按策略注入。

### Phase 6：Live2D（后续待办）

- [ ] 接入用户已有的标准 Cubism 3/4 `model3.json` 模型，不包含模型制作。
- [ ] 扫描 `data/live2d_models/`，支持模型选择、缩放和 X/Y 偏移。
- [ ] 支持模型自带 Idle、自动眨眼，保留点击、窗口拖动和桌宠右键菜单。
- [ ] 窗口隐藏时暂停渲染，加载失败时自动回退静态图。
- [ ] 后续按需增加语音口型与回复动作联动。

## 15. 关键风险

1. **ASR 误触发**：靠本地 VAD、音量/时长规则、文本过滤、重复窗口和调用频率上限共同控制。
2. **模型成本膨胀**：持续监听只在本地运行；仅有效语句调用 ASR；系统事件不逐条调用 LLM。
3. **提醒可靠性与模型不可靠冲突**：调度事实由 SQLite 保证，模型只生成措辞，失败立即降级。
4. **双数据库一致性**：SQLite 为事实源，Qdrant 使用 pending/outbox 补偿。
5. **翻译改变人格**：翻译调用注入同一人设、关系和相关记忆，并保存人设版本快照。
6. **VITS 子进程不稳定**：沿用 AgentStage 的 ready、超时、互斥和失败重启；主文本链路独立。
7. **第三方分发许可**：VITS Runtime、模型、Silero ONNX、参考代码和桌宠素材需分别审查，不因主项目许可证自动获得分发权。

## 16. 开工前技术决策清单

- [ ] 确认复用 Tauri 2 + Svelte 5 + Rust。
- [ ] 确认 Qdrant 开发/生产部署方式和固定版本。
- [ ] 确认默认 ASR multipart 协议及音频格式。
- [ ] 确认 Embedding 模型、向量维度、distance 和批量限制。
- [ ] 确认 LLM 是否流式，以及回复按整条还是按句进入翻译/VITS。
- [ ] 确认 VITS Runtime 与模型由安装包携带还是用户配置路径。
- [ ] 确认 API Key 使用 DPAPI 的具体封装。
- [ ] 确认错过提醒、重试和 Windows 通知策略。
- [ ] 确认默认 VAD 参数通过目标麦克风环境实测校准。
