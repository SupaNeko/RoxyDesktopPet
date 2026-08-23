# RoxyDesktopPet

## 1. 项目定位

RoxyDesktopPet 是专为洛琪希设计的 Windows 桌面陪伴应用，提供文字交流、语音输入和可选的本地语音输出。

项目只支持 Windows，定位是固定角色的日常陪伴桌宠，不是通用角色平台，也不以生产力工具为目标。

## 2. 功能简介

- 以洛琪希人设进行中文回复。
- 支持文字输入、持续语音输入和全局按住说话。
- 支持主动对话、待办提醒。
- 支持可选的 GPT-SoVITS v2ProPlus 本地日语语音输出(需下载扩展包)。
- 基于向量数据库实现基本的长期记忆。

## 3. 服务与依赖

### 大模型

对话使用 OpenAI-compatible Chat Completions 接口。将以下字段写入开发环境项目根目录的 `.env`，或发布版 `RoxyDesktopPet.exe` 同目录的 `.env`：

```dotenv
DEEPSEEK_API_KEY=
DEEPSEEK_BASE_URL=
DEEPSEEK_MODEL=
```

缺少有效的大模型配置时，角色对话、主动对话和模型驱动的待办工具调用不可用，桌宠界面和本地设置仍可使用。

### 语音输入

语音识别使用科大讯飞语音听写 WebAPI：

```dotenv
XFYUN_ASR_APP_ID=
XFYUN_ASR_API_KEY=
XFYUN_ASR_API_SECRET=
```

凭据也可以在应用设置中填写。缺少这些凭据时无法将麦克风语音识别为文字，但文字输入不受影响。

### 语音输出

语音输出使用独立的洛琪希 GPT-SoVITS `v2ProPlus` 扩展包，不包含在桌宠安装包中。目前仅支持配备 NVIDIA 显卡的 Windows 电脑，暂不提供 CPU 推理。

配置步骤：

1. 手动下载与当前桌宠版本匹配的语音扩展 ZIP。
2. 将 ZIP 解压到桌宠安装目录，确保最终路径为 `RoxyDesktopPet.exe` 同目录下的 `voice/manifest.json`，不要额外嵌套一层目录。
3. 启动桌宠，在“设置 → 语音输出”中选择“GPT-SoVITS”并保存。
4. 应用会检查 NVIDIA 显卡、扩展清单和固定版本运行时，并自动启动本地推理服务。模型和情绪参考音频均由扩展包提供，无需手动逐项配置。

缺少扩展包、显卡不受支持或扩展校验失败时，桌宠保持纯文本回复。

### 长期记忆

长期记忆使用 OpenAI-compatible Embedding API 生成向量，并由本地 Qdrant 完成语义检索：

```dotenv
QDRANT_URL=
EMBEDDING_BASE_URL=
EMBEDDING_MODEL=
EMBEDDING_API_KEY=
EMBEDDING_DIMENSION=
```

Embedding 参数也可以在应用设置中填写。缺少 Embedding 或 Qdrant 时仍可正常对话，但不会进行基于向量检索的长期记忆写入与召回。

## 4. 开发与打包

开发环境需要 Node.js、pnpm、Rust stable 和 Microsoft Edge WebView2 Runtime。

```powershell
Copy-Item .env.example .env
pnpm install
pnpm check
pnpm build
pnpm tauri dev
```

Rust 单独检查与测试：

```powershell
cd src-tauri
cargo check
cargo test
```

生成 Windows NSIS 安装包：

```powershell
pnpm tauri build
```

安装包输出到 `src-tauri/target/release/bundle/nsis/`。

项目不使用 Docker 启动 Qdrant。桌宠自带固定版本的 `qdrant.exe`，应用会在需要时自动启动，并把数据保存在 `data/qdrant/`；也可以通过 `QDRANT_URL` 连接已有的 Qdrant 服务。

在已准备好 GPT-SoVITS 官方源码、固定 Python/CUDA 运行时和洛琪希模型的开发机器上，可构建独立语音扩展：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build-gpt-sovits-voice-bundle.ps1
```

生成的扩展 ZIP 位于 `artifacts/`，该目录不进入版本控制。

## 5. 后续扩展计划

- 接入 MCP 服务器，扩展桌宠与外部能力的连接方式。
- 读取电脑状态，为主动陪伴和情境回复提供更多上下文。
- 改进长期记忆的提取、整理、召回和状态管理。
- 提供不依赖向量数据库的长期记忆降级方案。
- 探索 GPT-SoVITS 的 CPU 推理支持。
- 建立更清晰、可靠的应用状态管理。
- 视素材和效果评估，使用更多帧动画替代当前的简单表情差分。
