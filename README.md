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

#### 语音服务常见问题

**1. Hyper-V 端口预留冲突（语音服务拉不起来）**

GPT-SoVITS 推理服务固定监听本机 `127.0.0.1:9880`。启用 Hyper-V、WSL2 或 Docker Desktop 后，Windows 会动态预留大段 TCP 端口，9880 可能正好落入预留范围，导致服务无法绑定端口、桌宠一直显示“启动中”。

排查（无需管理员）：

```powershell
netsh interface ipv4 show excludedportrange protocol=tcp
```

如果输出中包含 `9880`，说明端口被 Hyper-V 预留。解决方法（管理员 PowerShell）：

```powershell
net stop winnat
netsh int ipv4 add excludedportrange protocol=tcp startport=9880 numberofports=1
net start winnat
```

该命令将 9880 永久排除在动态预留范围之外，重启后依然有效。若 `net stop winnat` 提示服务未启动，说明预留已随 winnat 停止而释放，直接执行中间的 `add excludedportrange` 命令固定端口即可。

**2. 系统代理拦截本机语音请求（状态卡“启动中”、测试无声音）**

开启 Clash 等系统代理时，旧版本桌宠发往 `127.0.0.1:9880` 的健康检查与合成请求可能被代理拦截并返回 502，表现为服务进程正常但桌宠一直显示“启动中”、设置页测试没有声音。当前版本已对本机回环请求强制绕过代理；使用旧版本时可临时关闭系统代理恢复语音。日志中如出现 `GPT-SoVITS 返回 502 Bad Gateway` 即为此问题。

### 长期记忆

长期记忆由观察者独占维护。记忆文本和 embedding 向量保存在桌宠自己的 SQLite 数据库中，召回时由 Rust 在进程内完成语义相似度、词面相关度和排序计算：

```dotenv
EMBEDDING_BASE_URL=
EMBEDDING_MODEL=
EMBEDDING_API_KEY=
EMBEDDING_DIMENSION=
```

Embedding 参数也可以在应用设置中填写。缺少 Embedding 时仍可正常对话，但不会进行长期记忆写入与召回。该方案不需要 Qdrant、PostgreSQL、Redis、Python 或独立记忆服务。

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

## 6. 参考

- [GPT-SoVits](https://github.com/RVC-Boss/GPT-SoVITS)
- Roxy GPT-SoVits[语音模型](https://www.bilibili.com/video/BV1HfF6zNEK8)
- Roxy [人设参考](https://github.com/umikok7/Roxy-SKILL)
