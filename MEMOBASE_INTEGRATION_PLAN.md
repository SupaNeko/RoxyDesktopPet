# Memobase 长期记忆接入方案

## 1. 文档目的

本文档定义 RoxyDesktopPet 接入开源项目 Memobase 作为长期记忆模块的目标架构、职责边界、运行方式、故障降级策略和实施阶段。

本方案直接替换项目现有的简单长期记忆实现。项目仍处于开发阶段，不迁移或兼容既有长期记忆数据。

## 2. 已确定的设计决策

1. 桌宠现有业务数据继续使用当前 SQLite 数据库。
2. Memobase 作为独立模块维护，继续使用 PostgreSQL 和 pgvector 保存长期记忆。
3. 删除 Memobase 对 Redis 的依赖，其他记忆能力保持不变。
4. 主对话只能读取长期记忆，不能创建、修改或删除长期记忆。
5. 所有长期记忆写入和维护行为统一由观察者负责。
6. Memobase、PostgreSQL 和 pgvector 随桌宠安装包一起分发。
7. 记忆模块安装在独立目录，由独立的 `RoxyMemoryService` 管理，并跟随桌宠启动和关闭。
8. 不设计 Memobase、PostgreSQL、pgvector 或记忆数据库的升级方案。
9. PostgreSQL 或 Memobase 不可用时，直接禁用长期记忆，不阻止桌宠启动和运行。
10. 长期记忆不可用时，在设置页显示明确警告和基础诊断信息。
11. 不实现旧长期记忆数据迁移、双写、兼容读取或回滚迁移。

## 3. 范围与非目标

### 3.1 本期范围

- 引入 Memobase 服务端及其客户端适配层。
- 移除 Redis 运行时和 Python Redis 依赖。
- 使用 PostgreSQL `BufferZone` 和进程内协调器替代 Redis 队列及分布式锁。
- 取消用户画像缓存，每次直接查询 PostgreSQL。
- 将主对话改造成长期记忆只读消费者。
- 将观察者改造成唯一长期记忆维护者。
- 删除现有简单长期记忆实现。
- 实现 `RoxyMemoryService` 对 PostgreSQL 和 Memobase 的启动、健康检查与关闭管理。
- 实现设置页中的长期记忆状态与故障警告。
- 实现长期记忆故障时的功能降级。
- 将记忆运行时打包进桌宠安装包。

### 3.2 非目标

- 不迁移现有长期记忆数据。
- 不保留旧长期记忆方案作为备用实现。
- 不设计 PostgreSQL 大版本或小版本升级。
- 不设计 Memobase 上游版本升级和数据库 schema 迁移。
- 不支持多个 Memobase API 实例或多个 worker。
- 不支持远程连接记忆数据库。
- 不提供 Memobase Cloud 模式。
- 不在第一阶段记录桌面行为、文件内容或其他环境观察结果；第一阶段只维护主对话产生的长期记忆。
- 不保证记忆服务不可用期间产生的对话在服务恢复后补录。

## 4. 总体架构

```text
RoxyDesktopPet
├─ UI / 设置页
├─ 主对话
│  └─ LongTermMemoryReader（只读）
├─ 观察者
│  └─ ObserverMemoryWriter（唯一写入者）
├─ 桌宠业务存储
│  └─ SQLite
└─ MemoryServiceClient
       │ HTTP，127.0.0.1
       ▼
RoxyMemoryService
├─ PostgreSQL 生命周期管理
├─ Memobase 生命周期管理
├─ 健康检查
└─ 运行状态输出
       │
       ▼
Memobase
├─ Profile
├─ Event
├─ Event Gist
├─ ChatBlob Buffer
├─ Context API
└─ pgvector 语义检索
       │
       ▼
PostgreSQL + pgvector
```

桌宠与记忆模块之间只通过本地 API 交互。桌宠业务代码不得直接查询或修改 Memobase 的 PostgreSQL 表。

## 5. 数据边界

### 5.1 桌宠 SQLite

继续保存桌宠业务数据，包括但不限于：

- 应用设置；
- 对话历史；
- 桌宠状态；
- UI 状态；
- 任务和提醒；
- 观察者运行状态；
- 与长期记忆无关的其他业务数据。

### 5.2 Memobase PostgreSQL

只保存长期记忆模块数据：

- Memobase 用户；
- GeneralBlob；
- BufferZone；
- UserProfile；
- UserEvent；
- UserEventGist；
- embedding 和 pgvector 索引；
- Memobase 正常运行所需的其他内部表。

两个数据库使用不同的数据文件和生命周期。删除或损坏记忆数据库不得影响桌宠 SQLite。

## 6. 主对话与观察者职责

### 6.1 主对话

主对话仅负责：

1. 在生成回复前读取相关长期记忆上下文。
2. 将长期记忆上下文作为受限的系统上下文注入提示词。
3. 在长期记忆不可用时使用当前会话上下文正常完成对话。

主对话必须移除：

- `remember`、`save_memory`、`forget_memory` 或同类工具；
- 提示词中的记忆抽取与保存指令；
- 对现有长期记忆存储的写入调用；
- 对 Memobase Profile、Event、Blob、flush 和删除接口的访问能力。

在依赖注入和类型层面，主对话只能获得 `LongTermMemoryReader`，不能获得 Writer 或管理接口。

### 6.2 观察者

观察者是唯一长期记忆写入者，负责：

1. 接收已完成的主对话轮次。
2. 过滤隐藏提示词、工具内部数据、密钥、文件全文和其他禁止进入长期记忆的内容。
3. 将允许处理的用户和助手消息规范化为 Memobase `ChatBlob`。
4. 将 ChatBlob 写入 Memobase。
5. 根据对话结束或缓冲策略触发 flush。
6. 处理用户在对话中表达的记住、遗忘和纠正意图。
7. 在长期记忆不可用时跳过写入，不影响主对话。

第一阶段观察者只提交主对话内容，不提交桌面活动、窗口标题、文件内容或其他环境信息。

## 7. 记忆读取流程

```text
用户发送消息
    ↓
检查长期记忆运行状态
    ├─ unavailable：跳过记忆读取
    └─ ready：调用 Memobase Context API
                  ↓
             获取 Profile + 相关 Event
                  ↓
             按 token 预算注入主对话
                  ↓
             生成正常回复
```

长期记忆上下文建议分为：

- 稳定用户画像；
- 与本轮输入相关的历史事件；
- 最近事件；
- 使用记忆的行为约束，例如不得无关地主动提及隐私信息。

记忆读取必须设置短超时。请求失败后立即按无长期记忆模式继续对话，不能等待服务反复重试。

## 8. 记忆写入流程

```text
主对话轮次完成
    ↓
观察者接收完整轮次
    ↓
检查长期记忆运行状态
    ├─ unavailable：跳过本次长期记忆写入
    └─ ready：执行隐私与内容过滤
                  ↓
             构造 ChatBlob
                  ↓
             写入 Memobase Buffer
                  ↓
             达到阈值或会话结束时 flush
                  ↓
             生成 Profile + Event + Event Gist
```

长期记忆写入属于后台能力，不得增加主对话响应延迟。

本方案不要求服务故障期间的记忆写入补偿。服务不可用时产生的对话不会自动补录进长期记忆。

## 9. Redis 移除方案

### 9.1 用户画像缓存

删除 Redis Profile Cache。`get_user_profiles()` 每次直接查询 PostgreSQL。

同时删除：

- Profile cache key；
- cache TTL；
- cache set/get/delete；
- Profile 更新后的缓存失效调用。

桌宠为单用户场景，Profile 数量较少，直接查询 PostgreSQL 的额外延迟可以接受。

### 9.2 后台队列

Redis List 不再保存 BufferZone ID。PostgreSQL `BufferZone` 是唯一持久化任务状态。

进程内 `asyncio.Queue` 仅作为后台 worker 的唤醒信号，不能作为可靠数据源。队列信号丢失时，可通过定时扫描 `BufferZone` 找回待处理任务。

### 9.3 并发锁

使用按 `project_id + user_id + blob_type` 建立的进程内 `asyncio.Lock`，保证同一用户的同类 buffer 串行 flush。

Memobase API 固定单进程、单 worker。禁止以多 worker 模式启动。

### 9.4 崩溃恢复

启动时扫描：

- `idle`：重新通知后台 worker；
- 超时的 `processing`：恢复为 `idle` 后重新处理；
- `failed`：保留失败状态并记录日志，不无限自动重试。

LLM 请求期间不得持有长时间 PostgreSQL 事务。

## 10. 独立运行目录

建议的安装目录：

```text
RoxyDesktopPet/
├─ RoxyDesktopPet.exe
├─ app/
└─ memory-runtime/
   ├─ RoxyMemoryService.exe
   ├─ memobase/
   ├─ python/
   ├─ postgres/
   ├─ pgvector/
   ├─ config/
   ├─ migrations/
   └─ licenses/
```

建议的用户数据目录：

```text
%LOCALAPPDATA%/RoxyDesktopPet/
├─ roxy.db
└─ memory/
   ├─ postgres-data/
   ├─ logs/
   └─ runtime-state.json
```

原则：

- 程序文件与用户数据分离；
- 不修改系统 PATH；
- 不复用系统现有 PostgreSQL；
- PostgreSQL 不注册为全局 Windows Service；
- PostgreSQL 和 Memobase 只监听 `127.0.0.1`；
- 使用私有端口和随机本地访问 token；
- 不对局域网或互联网暴露数据库和 Memobase API。

## 11. RoxyMemoryService 生命周期

### 11.1 启动

```text
桌宠启动
    ↓
检查现有 RoxyMemoryService
    ├─ 已健康运行：复用
    └─ 未运行：启动 RoxyMemoryService
                   ↓
              初始化或启动 PostgreSQL
                   ↓
              等待 PostgreSQL ready
                   ↓
              启动单 worker Memobase API
                   ↓
              等待 Memobase healthcheck
                   ↓
              发布 ready 状态
```

桌宠 UI 不等待记忆服务完成启动。记忆服务启动期间桌宠按长期记忆不可用模式运行。

### 11.2 正常关闭

```text
桌宠准备退出
    ↓
停止提交新的记忆写入
    ↓
通知 RoxyMemoryService 关闭
    ↓
在有限时间内结束当前后台任务
    ↓
关闭 Memobase API
    ↓
使用 PostgreSQL fast shutdown
    ↓
RoxyMemoryService 退出
```

关闭必须有时间上限，不能因为长期记忆阻止桌宠退出。禁止在正常路径中直接强杀 PostgreSQL。

### 11.3 异常退出

RoxyMemoryService 独立管理子进程：

- 桌宠正常退出时显式请求关闭；
- 桌宠意外退出后，MemoryService 等待短暂宽限期；
- 宽限期内桌宠重新启动时复用现有服务；
- 宽限期结束后安全关闭 PostgreSQL 和自身；
- MemoryService 意外退出时，桌宠直接禁用长期记忆。

## 12. 健康状态与降级

### 12.1 状态模型

建议统一状态：

```text
disabled    用户主动关闭长期记忆
starting    记忆服务正在启动
ready       PostgreSQL 与 Memobase 均可用
degraded    进程存在，但部分健康检查失败
unavailable 记忆服务不可用
```

只有 `ready` 状态允许读写长期记忆。

### 12.2 健康检查

至少检查：

- `RoxyMemoryService` 进程是否存活；
- PostgreSQL 是否接受连接；
- pgvector 扩展是否可用；
- Memobase `/healthcheck` 是否成功；
- Memobase 是否可以执行一次基础数据库查询；
- 配置的 LLM 和 embedding 服务错误由具体记忆请求上报，不阻止桌宠运行。

### 12.3 降级行为

当 PostgreSQL 或 Memobase 不可用时：

- 立即将长期记忆状态设为 `unavailable` 或 `degraded`；
- 主对话跳过长期记忆读取；
- 观察者跳过长期记忆写入和 flush；
- 不重放故障期间产生的对话；
- 不影响当前会话上下文；
- 不影响桌宠 UI、角色动画、普通对话和其他功能；
- 后台可以低频检查服务是否恢复；
- 服务恢复并通过完整健康检查后，允许后续新对话重新使用长期记忆。

禁止因为长期记忆故障而：

- 阻止桌宠启动；
- 阻止用户发送消息；
- 长时间阻塞主对话；
- 自动切回旧长期记忆实现；
- 自动清空或重建记忆数据库。

## 13. 设置页设计

设置页增加“长期记忆”区域。

### 13.1 正常状态

显示：

- 长期记忆：已启用；
- 服务状态：运行正常；
- 当前记忆模块版本；
- 数据目录；
- 可选的启用/禁用开关。

### 13.2 异常状态

显示非阻塞警告，例如：

> 长期记忆当前不可用。桌宠仍可正常聊天，但不会读取或保存跨会话记忆。

同时显示：

- 状态：启动中、数据库不可用、Memobase 不可用或健康检查失败；
- 最近一次检查时间；
- 简短且可理解的错误摘要；
- “重新检查”按钮；
- “打开日志目录”按钮；
- 可选的“重新启动记忆服务”按钮。

设置页不得显示数据库密码、访问 token、完整连接串或敏感异常堆栈。

### 13.3 对话界面提示

默认不在每条对话中重复提示记忆不可用，以免干扰体验。可以在会话顶部或状态区域显示一个低干扰图标；详细信息统一放在设置页。

## 14. 安全约束

- PostgreSQL 仅绑定回环地址；
- Memobase API 仅绑定回环地址；
- 使用随机端口或私有固定端口；
- Memobase API 使用随机访问 token；
- token 文件限制为当前 Windows 用户可读；
- 日志不得记录 token、数据库密码或完整对话正文；
- 观察者写入前过滤密钥、隐藏提示词和工具内部数据；
- 主对话不持有写记忆接口；
- 清除长期记忆必须是明确的用户操作；
- 记忆故障时不得自动删除或重新初始化数据库。

## 15. 配置建议

Memobase 初始配置建议：

```yaml
language: zh
use_timezone: Asia/Shanghai
persistent_chat_blobs: false
enable_event_embedding: true
profile_validate_mode: true
buffer_flush_interval: 3600
max_chat_blob_buffer_token_size: 1024
```

建议根据桌宠场景增加受控 Profile：

- 用户基础信息；
- 称呼偏好；
- 兴趣和厌恶；
- 沟通偏好；
- 日常习惯；
- 重要人物和宠物；
- 当前项目；
- 长期目标；
- 重要日期和计划。

实际字段应通过测试对话验证后确定，避免收集与桌宠体验无关的信息。

## 16. 实施阶段

### 阶段一：现状审计

- 定位旧长期记忆存储、抽取和检索入口；
- 定位主对话 prompt 和工具定义；
- 定位观察者输入、运行时机和生命周期；
- 定位设置页状态管理方式；
- 确定桌宠与后台进程的通信方式。

### 阶段二：Memobase 无 Redis 化

- 引入并固定 Memobase 源码版本；
- 删除 Profile Redis Cache；
- Profile 改为 PostgreSQL 直查；
- 删除 Redis 队列和分布式锁；
- 实现进程内后台协调器；
- 使用 BufferZone 作为唯一持久化任务状态；
- 实现 stale processing 恢复；
- 删除 Redis 依赖和部署配置；
- 补齐并发、失败和恢复测试。

### 阶段三：桌宠记忆接口

- 定义 `LongTermMemoryReader`；
- 定义 `ObserverMemoryWriter`；
- 实现 Memobase HTTP 适配器；
- 设置读取和写入超时；
- 实现全局记忆状态机；
- 实现 unavailable 降级。

### 阶段四：职责切换

- 主对话接入只读 Context；
- 删除主对话所有记忆写工具和相关 prompt；
- 观察者接管 ChatBlob 写入与 flush；
- 删除旧长期记忆代码、配置和数据表；
- 验证长期记忆不可用时主对话正常。

### 阶段五：独立运行时

- 实现 `RoxyMemoryService`；
- 打包私有 PostgreSQL 和 pgvector；
- 打包 Memobase 和 Python 运行时；
- 实现启动、健康检查和关闭流程；
- 实现异常退出宽限期；
- 实现日志和运行状态文件。

### 阶段六：设置页与安装包

- 增加长期记忆状态区域；
- 增加异常警告与重新检查功能；
- 增加打开日志目录功能；
- 将 `memory-runtime` 加入安装包；
- 验证安装、首次启动、正常关闭和卸载行为。

## 17. 验收标准

### 17.1 职责边界

- 主对话不存在任何长期记忆写工具。
- 主对话代码不能获得 `ObserverMemoryWriter`。
- 关闭观察者后不会产生新的 Profile 或 Event。
- 所有新长期记忆都能追溯到观察者写入流程。

### 17.2 正常功能

- 新对话能够形成 Profile 和 Event。
- 后续会话能够召回相关用户画像和事件。
- Event Gist 和 pgvector 语义检索保持可用。
- 多次并发写入不会重复 flush 同一 BufferZone。
- 正常退出不会破坏 PostgreSQL 数据目录。

### 17.3 故障降级

- PostgreSQL 未启动时桌宠能够正常启动和聊天。
- Memobase 未启动或 healthcheck 失败时桌宠能够正常聊天。
- 长期记忆不可用时主对话不会长时间等待。
- 长期记忆不可用时观察者不会持续产生错误弹窗。
- 设置页能显示长期记忆不可用警告和错误摘要。
- 服务恢复后，后续新对话可以重新使用长期记忆。
- 服务故障不会触发旧记忆方案或清空记忆数据。

### 17.4 打包与安全

- 最终用户不需要安装 Docker、PostgreSQL 或 Redis。
- PostgreSQL 和 Memobase 不监听外部网络接口。
- 安装目录与用户记忆数据目录分离。
- 日志和设置页不泄露密钥或数据库密码。
- 卸载桌宠时不会在无明确确认的情况下删除长期记忆数据。

## 18. 最终目标状态

完成后，RoxyDesktopPet 将具有以下长期记忆架构：

```text
桌宠业务数据：SQLite
长期记忆数据：独立 PostgreSQL + pgvector
记忆服务：无 Redis 的 Memobase
主对话：只读长期记忆
观察者：唯一长期记忆维护者
进程管理：RoxyMemoryService
故障策略：禁用长期记忆并在设置页告警，桌宠其余功能正常
部署方式：随桌宠安装，在独立目录中跟随桌宠启动和关闭
```

该方案优先保证桌宠主体稳定、记忆模块隔离和 Memobase 核心能力完整，不承担既有数据迁移与组件升级需求。
