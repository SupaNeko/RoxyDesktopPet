# 长期记忆冲突解决机制 —— 设计讨论记录

> 状态：**待实现**（设计方向已初步确定，细节未定，实现前先参考成熟开源方案）
> 记录日期：2026-08-24

## 1. 现状与问题

### 当前实现（`src-tauri/src/observer.rs` + `memory.rs`）

- 记忆观察器（memory observer）在对话后异步运行，攒够 `memory_observer_interval` 条新消息后，用 LLM 从对话中提取长期记忆（fact / preference / habit / relationship / experience）
- 插入前的唯一去重手段：`db::memory_text_exists` **精确文本匹配**，文本完全相同才跳过
- 记忆只增不改：全代码库没有 `update_memory` / `delete_memory`，Qdrant 侧也只有 upsert 新点，从不修改或删除旧点
- prompt 中会把已有记忆（最多 80 条）喂给提取模型并要求"不要重复已有记忆"，但这只防重复、不防冲突，且依赖 LLM 自觉

### 存在的问题

- **矛盾记忆共存**：例如旧记忆"用户喜欢喝咖啡"与新记忆"用户最近改喝拿铁了"会同时存在，互不覆盖
- **召回时自相矛盾**：`recall` 是 Qdrant 向量相似度搜索，聊到相关话题时矛盾的记忆可能都被检索出来拼进上下文，导致桌宠回答前后不一
- **用户无管理出口**：前端没有查看/删除记忆的 UI，错误记忆无法清理

## 2. 已确定的方案方向

结合"向量粗筛 + LLM 细判"两段式记忆维护（思路接近 mem0 / MemGPT 的记忆维护流水线）：

```
observer 提取出 N 条候选新记忆
   ↓ 逐条 embed，做宽泛相似度检索（低阈值，top 3~5）
   ├─ 无相似记忆 → 直接插入（走现有 remember）
   └─ 有相似记忆 → 把「新记忆 + 相似旧记忆」打包给 LLM 维护裁决（整个 batch 一次调用）
        ├─ add：信息不冲突，照常插入
        ├─ update：新信息覆盖某条旧记忆（改 SQLite + 重 embed + 同 id 写回 Qdrant）
        ├─ skip：纯属重复/旧记忆已涵盖，丢弃新记忆
        └─ delete：用户明确推翻过去（"我其实不喝咖啡"），删除旧记忆
```

要点：

- **宽阈值向量匹配做粗筛，LLM 做细判**：中文 embedding 对 paraphrase 的区分不靠谱，阈值宁低勿高（如 cosine 0.55~0.7），误判交给 LLM
- **update 保持 Qdrant 点 id 不变**，只换 vector + payload，是真正的"修改"，不留孤儿数据
- **维护裁决集中为一次 LLM 调用**（整个 batch 的冲突一起裁决），不逐条调用，成本可控
- 维护阶段独立于提取阶段（第二次 LLM 调用），职责清晰，且"每条新记忆精准检索 3~5 条相似"比"80 条全量记忆"更聚焦

## 3. 待定细节（实现前再定）

1. **LLM 如何指认要修改的旧记忆**
   - 初步倾向：索引号为主（候选列表编号 `[1] [2] [3]`，LLM 返回 `"target": 1`，后端映射到记忆 id），字符串精确匹配/子串匹配仅作 fallback
   - 原因：纯字符串匹配脆弱，LLM 可能改写原文导致匹配失败、维护静默失效
2. **update 的字段语义**
   - `text` 用新文本替换；`importance` 取 max 或由 LLM 裁决时给出；`memory_type` 一般不变但允许修正；`source_message_id` 指向新消息；`created_at` 保留或新增 `updated_at`
3. **delete 用硬删除还是软删除**
   - 初步倾向软删除：`status='retracted'`，recall 时过滤，可恢复可审计；LLM 误判 delete 的代价最高
4. **先 embed 后裁决的浪费**：裁决为 skip 时 embedding 白调一次，但 embedding 便宜、换来流程简单，倾向不优化
5. **成本增量**：每 batch 多出每条新记忆 1 次 embed + 1 次检索，有候选时 1 次维护 LLM 调用；observer 本身是低频批处理，可接受
6. **是否顺带加记忆管理 UI**（查看/手动删除），独立于本机制也可做

## 4. 实现时的改动面（预估）

- `db.rs`：加 `update_memory_text`、（软）`delete_memory`
- `memory.rs`：加 `update`（重 embed + 同 id upsert Qdrant）和 `remove`
- `observer.rs`：提取后插入维护阶段，解析维护裁决 JSON
- 均为增量改动，无结构调整

## 5. 实现前参考的开源方案

- **mem0**（https://github.com/mem0ai/mem0）：记忆 ADD / UPDATE / DELETE / NOOP 裁决机制，与本方案最接近
- **Letta（原 MemGPT）**（https://github.com/letta-ai/letta）：记忆块（memory block）自我编辑机制
- **Zep**（https://github.com/getzep/zep）：时序知识图谱式的记忆管理，事实失效（invalidation）处理

重点看它们：冲突裁决的 prompt 设计、UPDATE/DELETE 的判定标准、相似度阈值选取、软删除/事实失效的建模方式。
