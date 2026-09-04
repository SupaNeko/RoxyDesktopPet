use crate::db::{self, DbState};
use crate::memory_store::{MemoryAction, MemoryActionKind};
use crate::{ConversationState, MemoryEnvConfig, ModelConfig};
use serde::Deserialize;
use std::collections::HashSet;
use tauri::{AppHandle, Manager};

/// 单次提取的候选事实上限，约束决策轮的动作规模。
const MAX_CANDIDATES: usize = 5;
/// 每个候选检索相关已有画像的数量与相似度下限。
const RELATED_PER_CANDIDATE: usize = 3;
const RELATED_MIN_COSINE: f64 = 0.45;
/// 决策轮动作校验失败时，把错误反馈给模型修正的最大轮数。
const MAX_CORRECTION_ROUNDS: usize = 3;
/// 用户偏好总结观察者的 ReAct 工具循环最大轮数。
const MAX_SUMMARY_ROUNDS: usize = 8;
/// 用户偏好总结写入前的长度上限（提示词要求 500 字，留余量）。
const SUMMARY_MAX_CHARS: usize = 600;

/// 第 1 轮·提取：只看待观察对话，提炼少量候选事实，不给已有画像。
const EXTRACTOR_PROMPT: &str = r#"你是桌宠的记忆提取器。从对话中提取未来对话仍有帮助的稳定用户信息。
规则：
1. 只提取用户事实、偏好、习惯、关系、经历与长期目标。忽略寒暄、临时请求、提醒、凭据、助手陈述和低置信推测。
2. 最多输出 5 条最有价值的候选。每条 content 不超过 5 句；保留具体日期，消除“今天/昨天”等相对时间。
3. memory_type 仅限 fact/preference/habit/relationship/experience/goal；importance 和 confidence 为 0~1。
4. 没有值得记录的内容时返回空数组。
5. 只输出合法 JSON，不要 Markdown：{"candidates":[{"topic":"兴趣爱好","subtopic":"饮食","content":"用户偏好低糖食品","memory_type":"preference","importance":0.7,"confidence":0.95}]}"#;

/// 第 2 轮·决策：候选 + 检索到的相关已有画像，决定 APPEND/UPDATE/DELETE/ABORT，并主动合并去重。
const DECIDER_PROMPT: &str = r#"你是桌宠的长期记忆决策者。针对每个候选事实，结合检索到的相关已有画像，决定如何写入记忆库。
规则：
1. 每个候选必须给出恰好一个动作：
   - APPEND：与所有相关画像都不重复的新事实。
   - UPDATE：与某条已有画像冲突、需要修正或合并；target_id 必须来自提供的相关画像 id，content 输出合并后的完整新内容。
   - ABORT：与已有画像重复、临时或无价值。
2. 若发现多条已有画像语义接近，应主动合并精简：UPDATE 其中一条为合并后的完整内容，并对其余的发 DELETE，避免画像重复冗余。DELETE 的 target_id 同样必须来自提供的相关画像 id。
3. 不得编造 id。候选的 topic/subtopic/memory_type 可以修正；memory_type 仅限 fact/preference/habit/relationship/experience/goal；importance 和 confidence 为 0~1。
4. 只输出合法 JSON，不要 Markdown：{"actions":[{"action":"APPEND|UPDATE|DELETE|ABORT","target_id":null,"topic":"兴趣爱好","subtopic":"饮食","content":"用户偏好低糖食品","memory_type":"preference","importance":0.7,"confidence":0.95,"reason":"简短理由"}]}"#;

/// 用户偏好总结观察者：单上下文 ReAct 循环，通过 update_user_summary 工具做精确子串替换。
const SUMMARY_PROMPT: &str = r#"你是桌宠的用户偏好总结维护者。你维护一段“用户是一个怎么样的人”的滚动总结，它会无条件注入桌宠的系统提示词，因此只放任何对话中都有意义的客观内容：性格倾向、作息与活跃时间规律（参考每条消息的[记录于…]时间）、稳定习惯与偏好。
规则：
1. 总结不超过 500 字，客观简洁，不堆砌细节。
2. 只能通过 update_user_summary 工具更新总结：old_string 必须是当前总结的精确子串，会被 new_string 替换；仅当当前总结为空时，old_string 才能传空字符串进行首次全文写入。
3. 工具返回错误时，根据返回的当前总结全文修正参数后重试。
4. 没有新增信息时不要调用任何工具，直接回复“无更新”结束。"#;

#[derive(Deserialize)]
struct ExtractResult {
    #[serde(default)]
    candidates: Vec<Candidate>,
}

#[derive(Debug, Clone, Deserialize)]
struct Candidate {
    #[serde(default)]
    topic: String,
    #[serde(default)]
    subtopic: String,
    #[serde(default)]
    content: String,
    #[serde(default = "default_candidate_type")]
    memory_type: String,
    #[serde(default = "default_candidate_importance")]
    importance: f64,
    #[serde(default = "default_candidate_confidence")]
    confidence: f64,
}

fn default_candidate_type() -> String {
    "fact".into()
}
fn default_candidate_importance() -> f64 {
    0.5
}
fn default_candidate_confidence() -> f64 {
    0.8
}

#[derive(Deserialize)]
struct DecideResult {
    #[serde(default)]
    actions: Vec<MemoryAction>,
}

pub fn schedule(app: AppHandle) {
    log_info!("memory observer triggered");
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run(&app).await {
            log_warn!("memory observer failed: {e}")
        }
    });
}

async fn run(app: &AppHandle) -> Result<(), String> {
    let db_state = app.state::<DbState>();
    let model = app.state::<ModelConfig>();
    let memory_env = app.state::<MemoryEnvConfig>();
    let conversation = app.state::<ConversationState>();
    let _guard = conversation.0.lock().await;
    let (mut settings, batch) = {
        let conn = db_state.0.lock().await;
        (
            db::get_settings(&conn, !model.api_key.is_empty()).map_err(|e| e.to_string())?,
            db::observer_message_batch(&conn, 500).map_err(|e| e.to_string())?,
        )
    };
    if batch.is_empty() {
        log_info!("memory observer skipped: no pending messages");
        return Ok(());
    }
    let last_rowid = batch.last().unwrap().0;
    if !settings.memory_observer_enabled {
        let conn = db_state.0.lock().await;
        db::advance_memory_observer(&conn, last_rowid).map_err(|e| e.to_string())?;
        log_info!("memory observer skipped: disabled; checkpoint={last_rowid}");
        return Ok(());
    }
    let threshold = settings.memory_observer_interval.clamp(2, 500) as usize;
    if batch.len() < threshold {
        log_info!("memory observer deferred: pending={}, threshold={threshold}", batch.len());
        return Ok(());
    }
    if model.api_key.is_empty() {
        log_warn!("memory observer skipped: llm_not_configured");
        return Ok(());
    }
    if settings.embedding_base_url.is_empty() {
        settings.embedding_base_url = memory_env.embedding_base_url.clone()
    }
    if settings.embedding_model.is_empty() {
        settings.embedding_model = memory_env.embedding_model.clone()
    }
    if settings.embedding_api_key.is_empty() {
        settings.embedding_api_key = memory_env.embedding_api_key.clone()
    }
    if settings.embedding_dimension == 0 {
        settings.embedding_dimension = memory_env.embedding_dimension
    }
    let config = crate::commands::memory_config(&settings);
    let transcript = build_transcript(&batch);
    let event = {
        let conn = db_state.0.lock().await;
        crate::memory_store::begin_event(&conn, batch.first().unwrap().0, last_rowid, &transcript)
            .map_err(|e| e.to_string())?
    };
    log_info!("memory observer event started: event_id={}, messages={}, rowids={}..{}", event.id, batch.len(), event.first_rowid, event.last_rowid);
    let mut ledger = serde_json::json!({"extract": null, "decide": [], "summary": []});

    // 长期记忆观察者：提取 → 检索 → 决策（带纠错循环）。需要 embedding 配置；失败则整批重试。
    if !config.is_complete() {
        log_warn!("memory observer long-term skipped: embedding_not_configured");
    } else if let Err(e) = run_long_term_observer(&db_state, &model, &config, &transcript, &event.id, &mut ledger).await {
        let conn = db_state.0.lock().await;
        let _ = crate::memory_store::fail_event(&conn, &event.id, &e);
        return Err(e);
    }

    // 用户偏好总结观察者：独立上下文，只需 LLM；失败不阻塞游标推进。
    match run_summary_observer(&db_state, &model, &transcript).await {
        Ok(trace) => ledger["summary"] = trace,
        Err(e) => {
            log_warn!("user preference summary observer failed: {e}");
            ledger["summary_error"] = serde_json::Value::String(e);
        }
    }

    {
        let conn = db_state.0.lock().await;
        crate::memory_store::finalize_event(&conn, &event.id, &ledger.to_string())
            .map_err(|e| e.to_string())?;
        db::advance_memory_observer(&conn, last_rowid).map_err(|e| e.to_string())?;
        log_info!("memory observer committed: event_id={}, checkpoint={last_rowid}", event.id);
    }
    Ok(())
}

fn build_transcript(batch: &[(i64, crate::db::Message)]) -> String {
    batch
        .iter()
        .map(|(_, m)| {
            format!(
                "{}：{} [记录于{}]",
                if m.role == "user" { "用户" } else { "助手" },
                m.content,
                chrono::DateTime::from_timestamp_millis(m.created_at)
                    .map(|v| v.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 长期记忆观察者：提取候选 → 检索相关画像 → 决策并纠错（最多 3 轮）。
async fn run_long_term_observer(
    db_state: &DbState,
    model: &ModelConfig,
    config: &crate::memory::MemoryConfig,
    transcript: &str,
    event_id: &str,
    ledger: &mut serde_json::Value,
) -> Result<(), String> {
    // 第 1 轮：提取候选（不注入已有画像）。
    let extract_messages = serde_json::json!([
        {"role": "system", "content": EXTRACTOR_PROMPT},
        {"role": "user", "content": format!("待观察对话：\n{transcript}")}
    ]);
    let raw = call_chat_json(model, extract_messages.as_array().unwrap()).await?;
    ledger["extract"] = serde_json::Value::String(raw.clone());
    let mut extracted: ExtractResult = parse_json(&raw).map_err(|e| format!("提取结果 JSON 无效：{e}"))?;
    extracted.candidates.truncate(MAX_CANDIDATES);
    extracted
        .candidates
        .retain(|c| !c.content.trim().is_empty());
    if extracted.candidates.is_empty() {
        log_info!("memory observer extract: event_id={event_id}, no candidates");
        return Ok(());
    }
    for (index, c) in extracted.candidates.iter().enumerate() {
        log_info!("memory observer candidate[{}]: event_id={}, topic={}, subtopic={}, type={}, content={}", index + 1, event_id, c.topic, c.subtopic, c.memory_type, c.content);
    }

    // 检索：每个候选 embed 后与全部活跃画像算余弦，取 top 3（≥阈值），按 id 去重。
    let (related, valid_ids) = retrieve_related(db_state, config, &extracted.candidates).await?;
    log_info!("memory observer related: event_id={}, candidates={}, related={}", event_id, extracted.candidates.len(), related.len());

    // 第 2 轮：决策 + 纠错循环。
    let mut messages = vec![
        serde_json::json!({"role": "system", "content": DECIDER_PROMPT}),
        serde_json::json!({"role": "user", "content": build_decide_input(&extracted.candidates, &related)}),
    ];
    let mut round = 0;
    loop {
        let raw = call_chat_json(model, &messages).await?;
        ledger["decide"]
            .as_array_mut()
            .expect("decide 是数组")
            .push(serde_json::Value::String(raw.clone()));
        let parsed: DecideResult = match parse_json(&raw) {
            Ok(p) => p,
            Err(e) => {
                if round >= MAX_CORRECTION_ROUNDS {
                    return Err(format!("决策结果 JSON 无效且已达纠错上限：{e}"));
                }
                round += 1;
                messages.push(serde_json::json!({"role": "assistant", "content": raw}));
                messages.push(serde_json::json!({"role": "user", "content": format!("输出无法解析为合法 JSON：{e}。请重新只输出合法 JSON。")}));
                continue;
            }
        };
        let (valid, errors) = validate_actions(parsed.actions, &valid_ids);
        if !valid.is_empty() {
            apply_valid_actions(db_state, config, event_id, &valid).await?;
        }
        if errors.is_empty() {
            break;
        }
        if round >= MAX_CORRECTION_ROUNDS {
            log_warn!("memory observer corrections exhausted: event_id={}, remaining_errors={}", event_id, errors.len());
            break;
        }
        round += 1;
        let feedback = format!(
            "以下动作未通过校验，未被执行。请只针对这些动作重新输出修正后的 JSON（其他已成功的动作不要重复）：\n{}",
            errors.join("\n")
        );
        log_info!("memory observer correction round {}: event_id={}, errors={}", round, event_id, errors.len());
        messages.push(serde_json::json!({"role": "assistant", "content": raw}));
        messages.push(serde_json::json!({"role": "user", "content": feedback}));
    }
    Ok(())
}

/// 候选检索：返回给决策者的相关画像 JSON（含 id）与合法 id 集合。
async fn retrieve_related(
    db_state: &DbState,
    config: &crate::memory::MemoryConfig,
    candidates: &[Candidate],
) -> Result<(Vec<serde_json::Value>, HashSet<String>), String> {
    let existing = {
        let conn = db_state.0.lock().await;
        crate::memory_store::list_active(&conn, 5000).map_err(|e| e.to_string())?
    };
    let mut picked: Vec<&crate::memory_store::StoredMemory> = Vec::new();
    for candidate in candidates {
        let vector = crate::memory::embed(config, &candidate.content).await?;
        let mut scored: Vec<(f64, &crate::memory_store::StoredMemory)> = existing
            .iter()
            .filter_map(|m| {
                m.embedding
                    .as_deref()
                    .map(|e| (crate::memory::cosine(&vector, e), m))
            })
            .filter(|(score, _)| *score >= RELATED_MIN_COSINE)
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (score, m) in scored.into_iter().take(RELATED_PER_CANDIDATE) {
            if picked.iter().any(|p| p.id == m.id) {
                continue;
            }
            log_info!("memory observer related hit: candidate={}, memory_id={}, score={:.3}, content={}", candidate.content, m.id, score, m.text);
            picked.push(m);
        }
    }
    let valid_ids = picked.iter().map(|m| m.id.clone()).collect::<HashSet<_>>();
    let related = picked
        .iter()
        .map(|m| {
            serde_json::json!({"id": m.id, "topic": m.topic, "subtopic": m.subtopic, "content": m.text, "type": m.memory_type})
        })
        .collect::<Vec<_>>();
    Ok((related, valid_ids))
}

fn build_decide_input(candidates: &[Candidate], related: &[serde_json::Value]) -> String {
    let candidate_text = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            format!(
                "{}. [{} / {} | {} | 重要度 {:.2} | 置信度 {:.2}] {}",
                i + 1,
                c.topic,
                c.subtopic,
                c.memory_type,
                c.importance,
                c.confidence,
                c.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let related_text = if related.is_empty() {
        "（记忆库中未检索到相关画像）".to_string()
    } else {
        serde_json::to_string_pretty(related).unwrap_or_default()
    };
    format!("候选事实：\n{candidate_text}\n\n相关已有画像（UPDATE/DELETE 的 target_id 只能从这里取）：\n{related_text}\n\n请为每个候选决定动作，并合并精简语义接近的已有画像。")
}

/// 校验决策动作：合法的留下执行，非法的生成给模型修正的错误描述。
fn validate_actions(
    actions: Vec<MemoryAction>,
    valid_ids: &HashSet<String>,
) -> (Vec<MemoryAction>, Vec<String>) {
    let mut valid = Vec::new();
    let mut errors = Vec::new();
    let id_list = || {
        let mut ids = valid_ids.iter().cloned().collect::<Vec<_>>();
        ids.sort();
        ids.join(", ")
    };
    for (index, mut action) in actions.into_iter().enumerate() {
        let label = format!(
            "动作{}（{}）",
            index + 1,
            action.content.chars().take(20).collect::<String>()
        );
        match action.action {
            MemoryActionKind::Abort => valid.push(action),
            MemoryActionKind::Append | MemoryActionKind::Update => {
                if !matches!(
                    action.memory_type.as_str(),
                    "fact" | "preference" | "habit" | "relationship" | "experience" | "goal"
                ) {
                    errors.push(format!("{label}：memory_type「{}」无效，仅限 fact/preference/habit/relationship/experience/goal。", action.memory_type));
                    continue;
                }
                if action.content.trim().is_empty() {
                    errors.push(format!("{label}：content 不能为空。"));
                    continue;
                }
                if matches!(action.action, MemoryActionKind::Update) {
                    match action.target_id.as_deref() {
                        Some(id) if valid_ids.contains(id) => {}
                        Some(id) => {
                            errors.push(format!("{label}：target_id「{id}」不在提供的相关画像中，合法 id：{}。", id_list()));
                            continue;
                        }
                        None => {
                            errors.push(format!("{label}：UPDATE 必须提供 target_id（来自相关画像）。"));
                            continue;
                        }
                    }
                }
                action.importance = action.importance.clamp(0.0, 1.0);
                action.confidence = action.confidence.clamp(0.0, 1.0);
                valid.push(action);
            }
            MemoryActionKind::Delete => match action.target_id.as_deref() {
                Some(id) if valid_ids.contains(id) => valid.push(action),
                Some(id) => {
                    errors.push(format!("{label}：target_id「{id}」不在提供的相关画像中，合法 id：{}。", id_list()));
                }
                None => {
                    errors.push(format!("{label}：DELETE 必须提供 target_id（来自相关画像）。"));
                }
            },
        }
    }
    (valid, errors)
}

/// 应用一轮校验通过的动作：embedding + 事务落库（沿用现有管线）。
async fn apply_valid_actions(
    db_state: &DbState,
    config: &crate::memory::MemoryConfig,
    event_id: &str,
    actions: &[MemoryAction],
) -> Result<(), String> {
    let mut embeddings = Vec::with_capacity(actions.len());
    for action in actions {
        if matches!(
            action.action,
            MemoryActionKind::Append | MemoryActionKind::Update
        ) && !action.content.trim().is_empty()
        {
            let vector = crate::memory::embed(config, &action.content).await?;
            embeddings.push(Some(vector));
        } else {
            embeddings.push(None);
        }
    }
    let mut conn = db_state.0.lock().await;
    let changed = crate::memory_store::apply_actions(
        &mut conn,
        event_id,
        actions,
        "",
        &config.embedding_model,
        &embeddings,
    )
    .map_err(|e| e.to_string())?;
    log_info!("memory observer actions applied: event_id={}, applied={}, changed={}", event_id, actions.len(), changed);
    Ok(())
}

/// 用户偏好总结观察者：ReAct 工具循环，模型通过 update_user_summary 工具做精确子串替换。
async fn run_summary_observer(
    db_state: &DbState,
    model: &ModelConfig,
    transcript: &str,
) -> Result<serde_json::Value, String> {
    let current = {
        let conn = db_state.0.lock().await;
        db::get_user_preference_summary(&conn).unwrap_or_default()
    };
    let mut working = current.trim().to_string();
    let mut messages = vec![
        serde_json::json!({"role": "system", "content": SUMMARY_PROMPT}),
        serde_json::json!({"role": "user", "content": format!(
            "当前用户偏好总结：\n{}\n\n待观察对话：\n{transcript}",
            if working.is_empty() { "（空）" } else { working.as_str() }
        )}),
    ];
    let tool = serde_json::json!({
        "type": "function",
        "function": {
            "name": "update_user_summary",
            "description": "更新用户偏好总结。old_string 必须是当前总结的精确子串，会被 new_string 替换；仅当当前总结为空时，old_string 才能传空字符串进行首次全文写入。没有新增信息时不要调用本工具。",
            "parameters": {
                "type": "object",
                "properties": {
                    "old_string": {"type": "string", "description": "当前总结中要替换的精确子串；仅当前总结为空时才能传空字符串"},
                    "new_string": {"type": "string", "description": "替换后的文本"}
                },
                "required": ["old_string", "new_string"],
                "additionalProperties": false
            }
        }
    });
    let mut trace = Vec::new();
    for _ in 0..MAX_SUMMARY_ROUNDS {
        let message = call_chat_with_tools(model, &messages, &[tool.clone()]).await?;
        let tool_calls = message
            .get("tool_calls")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        messages.push(message);
        if tool_calls.is_empty() {
            break;
        }
        for call in tool_calls {
            let call_id = call.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let arguments = call
                .get("function")
                .and_then(|f| f.get("arguments"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let parsed_args: Option<serde_json::Value> = match &arguments {
                serde_json::Value::String(raw) => parse_json::<serde_json::Value>(raw).ok(),
                other => Some(other.clone()),
            };
            let result = match parsed_args.as_ref().and_then(|v| {
                Some((
                    v.get("old_string")?.as_str()?.to_string(),
                    v.get("new_string")?.as_str()?.to_string(),
                ))
            }) {
                Some((old, new)) => match apply_summary_edit(&mut working, &old, &new) {
                    Ok(()) => {
                        trace.push(serde_json::json!({"old": old, "new": new, "ok": true}));
                        format!("更新成功。当前总结全文：\n{working}")
                    }
                    Err(e) => {
                        trace.push(serde_json::json!({"old": old, "new": new, "ok": false, "error": e}));
                        format!("更新失败：{e}。请修正后重试。当前总结全文：\n{working}")
                    }
                },
                None => {
                    trace.push(serde_json::json!({"ok": false, "error": "工具参数无效"}));
                    "更新失败：工具参数无效，需要 old_string 和 new_string 两个字符串参数。".to_string()
                }
            };
            messages.push(serde_json::json!({"role": "tool", "tool_call_id": call_id, "content": result}));
        }
    }
    let final_text = working
        .trim()
        .chars()
        .take(SUMMARY_MAX_CHARS)
        .collect::<String>();
    if final_text != current.trim() {
        let conn = db_state.0.lock().await;
        db::set_user_preference_summary(&conn, &final_text).map_err(|e| e.to_string())?;
        log_info!("user preference summary updated: len={}", final_text.chars().count());
    } else {
        log_info!("user preference summary unchanged");
    }
    Ok(serde_json::Value::Array(trace))
}

/// 精确子串替换：old 为空串表示全文写入，但仅在当前总结为空时允许；否则视为未匹配。
fn apply_summary_edit(current: &mut String, old: &str, new: &str) -> Result<(), String> {
    if old.is_empty() {
        if current.trim().is_empty() {
            *current = new.trim().to_string();
            return Ok(());
        }
        return Err("当前总结非空，不允许全文写入，请提供要替换内容的精确子串。".to_string());
    }
    let count = current.matches(old).count();
    if count == 0 {
        return Err(format!(
            "未找到要替换的文本「{}」",
            old.chars().take(40).collect::<String>()
        ));
    }
    if count > 1 {
        return Err(format!(
            "「{}」出现了 {count} 次，请提供更长的上下文以唯一定位",
            old.chars().take(40).collect::<String>()
        ));
    }
    *current = current.replacen(old, new, 1);
    Ok(())
}

/// JSON mode 调用：要求模型只输出 JSON 文本，返回原始内容。
async fn call_chat_json(
    model: &ModelConfig,
    messages: &[serde_json::Value],
) -> Result<String, String> {
    let request_body = serde_json::json!({"model": model.model, "messages": messages, "temperature": 0.1, "thinking": {"type": "disabled"}, "response_format": {"type": "json_object"}});
    let value = send_chat_request(model, &request_body).await?;
    value
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "观察模型没有返回内容".to_string())
}

/// 工具调用（ReAct）循环的单轮请求：返回 assistant message 对象。
async fn call_chat_with_tools(
    model: &ModelConfig,
    messages: &[serde_json::Value],
    tools: &[serde_json::Value],
) -> Result<serde_json::Value, String> {
    let request_body = serde_json::json!({"model": model.model, "messages": messages, "tools": tools, "tool_choice": "auto", "temperature": 0.1, "thinking": {"type": "disabled"}});
    let value = send_chat_request(model, &request_body).await?;
    value
        .pointer("/choices/0/message")
        .cloned()
        .ok_or_else(|| "观察模型没有返回消息".to_string())
}

async fn send_chat_request(
    model: &ModelConfig,
    request_body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    log_info!("memory observer llm request: endpoint={}/chat/completions\n{}", model.base_url.trim_end_matches('/'), serde_json::to_string_pretty(request_body).unwrap_or_else(|_| request_body.to_string()));
    let response = reqwest::Client::new()
        .post(format!("{}/chat/completions", model.base_url.trim_end_matches('/')))
        .bearer_auth(&model.api_key)
        .json(request_body)
        .send()
        .await
        .map_err(|e| format!("观察模型请求失败：{e}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        log_error!("memory observer llm error response: status={status}, body={body}");
        return Err(format!("观察模型返回 {status}"));
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("观察响应无效：{e}"))?;
    log_info!("memory observer llm response: status={status}\n{}", serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string()));
    crate::usage::record_chat_response(crate::usage::CAT_OBSERVER, &value).await;
    Ok(value)
}

/// 容错解析模型输出的 JSON：截取第一个 { 到最后一个 } 之间的内容。
fn parse_json<T: for<'de> Deserialize<'de>>(raw: &str) -> Result<T, String> {
    let start = raw.find('{').ok_or("响应缺少 JSON")?;
    let end = raw.rfind('}').ok_or("响应缺少 JSON")?;
    serde_json::from_str(&raw[start..=end]).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(
        kind: MemoryActionKind,
        target_id: Option<&str>,
        content: &str,
        memory_type: &str,
    ) -> MemoryAction {
        MemoryAction {
            action: kind,
            target_id: target_id.map(|s| s.to_string()),
            topic: "话题".into(),
            subtopic: "子类".into(),
            content: content.into(),
            memory_type: memory_type.into(),
            importance: 0.5,
            confidence: 0.8,
            reason: "测试".into(),
        }
    }

    #[test]
    fn validate_actions_accepts_valid() {
        let ids = HashSet::from(["id-1".to_string()]);
        let actions = vec![
            action(MemoryActionKind::Append, None, "用户喜欢喝咖啡", "preference"),
            action(MemoryActionKind::Update, Some("id-1"), "用户每天喝两杯咖啡", "habit"),
            action(MemoryActionKind::Delete, Some("id-1"), "", "fact"),
            action(MemoryActionKind::Abort, None, "重复内容", "fact"),
        ];
        let (valid, errors) = validate_actions(actions, &ids);
        assert_eq!(valid.len(), 4);
        assert!(errors.is_empty());
    }

    #[test]
    fn validate_actions_rejects_bad_target_id() {
        let ids = HashSet::from(["id-1".to_string()]);
        let actions = vec![
            action(MemoryActionKind::Update, Some("id-x"), "内容", "fact"),
            action(MemoryActionKind::Delete, Some("id-y"), "", "fact"),
            action(MemoryActionKind::Update, None, "内容", "fact"),
        ];
        let (valid, errors) = validate_actions(actions, &ids);
        assert!(valid.is_empty());
        assert_eq!(errors.len(), 3);
        assert!(errors[0].contains("id-1")); // 错误信息里带合法 id 列表
    }

    #[test]
    fn validate_actions_rejects_bad_type_and_empty_content() {
        let ids = HashSet::new();
        let actions = vec![
            action(MemoryActionKind::Append, None, "内容", "unknown"),
            action(MemoryActionKind::Append, None, "   ", "fact"),
        ];
        let (valid, errors) = validate_actions(actions, &ids);
        assert!(valid.is_empty());
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn validate_actions_clamps_scores() {
        let ids = HashSet::new();
        let mut a = action(MemoryActionKind::Append, None, "内容", "fact");
        a.importance = 7.0;
        a.confidence = -1.0;
        let (valid, errors) = validate_actions(vec![a], &ids);
        assert!(errors.is_empty());
        assert_eq!(valid[0].importance, 1.0);
        assert_eq!(valid[0].confidence, 0.0);
    }

    #[test]
    fn summary_edit_exact_replace() {
        let mut text = "用户习惯凌晨 3 点睡。喜欢喝咖啡。".to_string();
        apply_summary_edit(&mut text, "凌晨 3 点", "凌晨 1 点").unwrap();
        assert_eq!(text, "用户习惯凌晨 1 点睡。喜欢喝咖啡。");
    }

    #[test]
    fn summary_edit_full_write_when_old_empty() {
        let mut text = String::new();
        apply_summary_edit(&mut text, "", "用户是一名开发者。").unwrap();
        assert_eq!(text, "用户是一名开发者。");
    }

    #[test]
    fn summary_edit_full_write_rejected_when_not_empty() {
        let mut text = "用户是一名开发者。".to_string();
        let err = apply_summary_edit(&mut text, "", "重写全文").unwrap_err();
        assert!(err.contains("非空"));
        assert_eq!(text, "用户是一名开发者。"); // 失败不改变原文
    }

    #[test]
    fn summary_edit_reports_missing() {
        let mut text = "用户喜欢喝咖啡。".to_string();
        let err = apply_summary_edit(&mut text, "喝茶", "喝水").unwrap_err();
        assert!(err.contains("未找到"));
        assert_eq!(text, "用户喜欢喝咖啡。"); // 失败不改变原文
    }

    #[test]
    fn summary_edit_reports_ambiguous() {
        let mut text = "咖啡。咖啡。".to_string();
        let err = apply_summary_edit(&mut text, "咖啡", "茶").unwrap_err();
        assert!(err.contains("2 次"));
    }
}
