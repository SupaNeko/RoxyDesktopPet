# RoxyDesktopPet

以洛琪希为固定角色的 Windows 本地优先陪伴型桌宠。本项目从 ChatPet 派生，目标是打通语音输入、角色对话、情绪参考音频选择和 GPT-SoVITS 本地语音输出全链路，同时尽量保持开箱即用。

语音输出是仅面向 NVIDIA 显卡用户的可选组件：桌宠主体保持轻量，应用内按固定版本清单下载独立 Python、CUDA 版 PyTorch、GPT-SoVITS 官方源码、基础模型和角色资产。长期记忆暂时保留当前设计。

详细产品边界与技术方案见 [`docs/PRODUCT_AND_VOICE_ARCHITECTURE.md`](docs/PRODUCT_AND_VOICE_ARCHITECTURE.md)。

## 当前已实现

- Tauri 2 + Svelte 5 + Rust 工程。
- 透明、置顶、不显示在任务栏的静态桌宠窗口。
- 左键点击宠物后打开极简文字输入气泡。
- 系统托盘右键菜单：设置、显示/隐藏桌宠、退出。
- 独立设置窗口：角色人设、DeepSeek 模型状态和互斥的语音输出模式。
- SQLite WAL 本地持久化：设置和消息历史。
- 通过 `.env` 接入 DeepSeek V4 Flash 的 `/chat/completions` 文本对话。
- 语音输出可选择关闭、通用 TTS API 或本地 VITS；API 支持千问 DashScope 和 OpenAI-compatible `/audio/speech`。
- VITS 会自动扫描 `data/vits_models` 的模型、语言和说话人；合成前可由 DeepSeek 注入人设与长期记忆翻译。缺少 EXE 时保持纯文本。
- 助手文本先显示，语音在后台串行生成并播放；普通对话、语音输入、到点提醒和主动消息都会进入同一播放队列。
- WebView 默认右键菜单已禁用；托盘图标右键菜单不受影响。
- Windows 麦克风持续监听，可在设置中选择设备并开关。
- 本地 Silero VAD V5 + 端点容错：预滚动、连续语音确认、句末静音、最短语句与 30 秒截断。
- 讯飞语音听写流式 WebAPI：完整语句识别后自动调用 DeepSeek。
- 讯飞凭据支持项目 `.env` 或设置页配置，设置页不会回显 Key 和 Secret。
- DeepSeek 受限工具调用可创建和查询 Todo；SQLite occurrence 调度器负责到点提醒并在重启后补发。
- 主动消息支持开关、随机最短/最长间隔和每日上限，到点时由模型按人设生成消息。
- 长期记忆以 SQLite 为事实源，通过外部 OpenAI-compatible Embedding API 写入本地 Qdrant，并在对话前执行语义召回。
- 模型仅在长期记忆配置完整时获得 `remember` 工具；Qdrant 不可用时自动退化为普通对话。
- 可配置的记忆观察者默认每 30 条用户对话消息在后台总结一次，将新发现的稳定事实通过现有 Embedding + Qdrant 链路保存；可在设置中关闭。
- Qdrant Runtime 管理器优先连接现有服务，否则启动 `data/qdrant_runtime/qdrant.exe` 或系统 `PATH` 中的 Qdrant；仅关闭自己启动的进程。

## 尚未实现

- Windows 媒体、空闲和锁屏事件。

## 开发

环境：Node.js、pnpm、Rust stable、WebView2。

复制环境变量模板并填入自己的 DeepSeek API Key：

```powershell
Copy-Item .env.example .env
```

```powershell
pnpm install
pnpm check
pnpm build
pnpm tauri dev
```

Rust 单独验证：

```powershell
cd src-tauri
cargo check
cargo test
```

本地 Qdrant（需要先安装 Docker Desktop）：

```powershell
docker compose -f docker-compose.qdrant.yml up -d
```

Windows 发布/开发环境也可将官方 `qdrant.exe` 放入 `data/qdrant_runtime/`。ChatPet 会隐藏启动它，并将持久数据固定保存到 `data/qdrant/storage/`，不要求安装 Docker。

产品需求与技术设计见 [docs/REQUIREMENTS.md](docs/REQUIREMENTS.md) 和 [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)。

## 运行时目录

开发环境的数据写入仓库根目录 `data/`。该目录不进入版本控制。

```text
data/
├─ chatpet.db
├─ qdrant/              # 后续阶段
├─ vits_runtime/
│  └─ vits_runtime.exe  # 可选
├─ vits_models/         # 可选
└─ voice_cache/         # API TTS 与 VITS 的本地音频缓存
```
