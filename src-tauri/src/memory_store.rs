use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct StoredMemory {
    pub id: String,
    pub topic: String,
    pub subtopic: String,
    pub text: String,
    pub memory_type: String,
    pub importance: f64,
    pub confidence: f64,
    #[serde(skip)]
    pub embedding: Option<Vec<f32>>,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_recalled_at: Option<i64>,
    pub recall_count: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum MemoryActionKind {
    Append,
    Update,
    Delete,
    Abort,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemoryAction {
    pub action: MemoryActionKind,
    #[serde(default)]
    pub target_id: Option<String>,
    #[serde(default = "default_topic")]
    pub topic: String,
    #[serde(default = "default_subtopic")]
    pub subtopic: String,
    #[serde(default)]
    pub content: String,
    #[serde(default = "default_type")]
    pub memory_type: String,
    #[serde(default = "default_importance")]
    pub importance: f64,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    #[serde(default)]
    pub reason: String,
}

fn default_topic() -> String {
    "其他".into()
}
fn default_subtopic() -> String {
    "一般".into()
}
fn default_type() -> String {
    "fact".into()
}
fn default_importance() -> f64 {
    0.5
}
fn default_confidence() -> f64 {
    0.8
}

pub struct EventRecord {
    pub id: String,
    pub first_rowid: i64,
    pub last_rowid: i64,
}

fn encode_embedding(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn decode_embedding(bytes: Option<Vec<u8>>) -> Option<Vec<f32>> {
    let bytes = bytes?;
    if bytes.len() % 4 != 0 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect(),
    )
}

pub fn list_active(conn: &Connection, limit: usize) -> rusqlite::Result<Vec<StoredMemory>> {
    let mut stmt = conn.prepare("SELECT id,topic,subtopic,text,memory_type,importance,confidence,embedding,created_at,updated_at,last_recalled_at,recall_count FROM memories WHERE status='active' ORDER BY updated_at DESC LIMIT ?")?;
    let rows = stmt
        .query_map([limit as i64], |r| {
            Ok(StoredMemory {
                id: r.get(0)?,
                topic: r.get(1)?,
                subtopic: r.get(2)?,
                text: r.get(3)?,
                memory_type: r.get(4)?,
                importance: r.get(5)?,
                confidence: r.get(6)?,
                embedding: decode_embedding(r.get(7)?),
                created_at: r.get(8)?,
                updated_at: r.get(9)?,
                last_recalled_at: r.get(10)?,
                recall_count: r.get(11)?,
            })
        })?
        .collect();
    rows
}

pub fn begin_event(
    conn: &Connection,
    first_rowid: i64,
    last_rowid: i64,
    transcript: &str,
) -> rusqlite::Result<EventRecord> {
    let event = EventRecord {
        id: uuid::Uuid::new_v4().to_string(),
        first_rowid,
        last_rowid,
    };
    conn.execute("INSERT INTO memory_events(id,first_message_rowid,last_message_rowid,transcript,status,created_at) VALUES(?,?,?,?, 'processing', ?)", params![event.id,event.first_rowid,event.last_rowid,transcript,chrono::Utc::now().timestamp_millis()])?;
    Ok(event)
}

pub fn fail_event(conn: &Connection, event_id: &str, error: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE memory_events SET status='failed',error=?,processed_at=? WHERE id=?",
        params![error, chrono::Utc::now().timestamp_millis(), event_id],
    )?;
    Ok(())
}

/// 观察者多轮交互结束后，把合并的各轮原始输出（提取/决策/总结的账本 JSON）写入事件。
pub fn finalize_event(conn: &Connection, event_id: &str, raw_result: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE memory_events SET observer_result=?,status='applied',processed_at=? WHERE id=?",
        params![raw_result, chrono::Utc::now().timestamp_millis(), event_id],
    )?;
    Ok(())
}

fn revision(
    tx: &Transaction<'_>,
    memory_id: &str,
    action: &str,
    old: Option<&str>,
    new: Option<&str>,
    event_id: &str,
    now: i64,
) -> rusqlite::Result<()> {
    tx.execute("INSERT INTO memory_revisions(id,memory_id,action,old_text,new_text,source_event_id,created_at) VALUES(?,?,?,?,?,?,?)", params![uuid::Uuid::new_v4().to_string(),memory_id,action,old,new,event_id,now])?;
    Ok(())
}

pub fn apply_actions(
    conn: &mut Connection,
    event_id: &str,
    actions: &[MemoryAction],
    raw_result: &str,
    embedding_model: &str,
    embeddings: &[Option<Vec<f32>>],
) -> rusqlite::Result<usize> {
    let tx = conn.transaction()?;
    let now = chrono::Utc::now().timestamp_millis();
    let mut changed = 0;
    for (index, action) in actions.iter().enumerate() {
        match action.action {
            MemoryActionKind::Abort => {
                log_info!("memory store ABORT: event_id={event_id}, content={}, reason={}", action.content, action.reason);
            }
            MemoryActionKind::Append => {
                let text = action.content.trim();
                if text.is_empty() {
                    continue;
                }
                let id = uuid::Uuid::new_v4().to_string();
                let vector = embeddings
                    .get(index)
                    .and_then(|v| v.as_deref())
                    .map(encode_embedding);
                let dimension = embeddings
                    .get(index)
                    .and_then(|v| v.as_ref())
                    .map_or(0, Vec::len) as i64;
                tx.execute("INSERT INTO memories(id,topic,subtopic,text,memory_type,importance,confidence,status,embedding,embedding_model,embedding_dimension,embedding_status,source_message_id,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'active',?,?,?,'ready',?,?,?)", params![id,action.topic,action.subtopic,text,action.memory_type,action.importance.clamp(0.0,1.0),action.confidence.clamp(0.0,1.0),vector,embedding_model,dimension,event_id,now,now])?;
                revision(&tx, &id, "APPEND", None, Some(text), event_id, now)?;
                log_info!("memory store APPEND: event_id={event_id}, memory_id={id}, topic={}, subtopic={}, type={}, importance={}, confidence={}, content={text}", action.topic, action.subtopic, action.memory_type, action.importance, action.confidence);
                changed += 1;
            }
            MemoryActionKind::Update => {
                let Some(id) = action.target_id.as_deref() else {
                    log_warn!("memory store UPDATE skipped: event_id={event_id}, missing target_id");
                    continue;
                };
                let text = action.content.trim();
                if text.is_empty() {
                    continue;
                }
                let old: Option<String> = tx
                    .query_row(
                        "SELECT text FROM memories WHERE id=? AND status='active'",
                        [id],
                        |r| r.get(0),
                    )
                    .optional()?;
                let Some(old) = old else {
                    log_warn!("memory store UPDATE skipped: event_id={event_id}, target_not_found={id}");
                    continue;
                };
                let vector = embeddings
                    .get(index)
                    .and_then(|v| v.as_deref())
                    .map(encode_embedding);
                let dimension = embeddings
                    .get(index)
                    .and_then(|v| v.as_ref())
                    .map_or(0, Vec::len) as i64;
                tx.execute("UPDATE memories SET topic=?,subtopic=?,text=?,memory_type=?,importance=?,confidence=?,embedding=?,embedding_model=?,embedding_dimension=?,embedding_status='ready',updated_at=? WHERE id=?",params![action.topic,action.subtopic,text,action.memory_type,action.importance.clamp(0.0,1.0),action.confidence.clamp(0.0,1.0),vector,embedding_model,dimension,now,id])?;
                revision(&tx, id, "UPDATE", Some(&old), Some(text), event_id, now)?;
                log_info!("memory store UPDATE: event_id={event_id}, memory_id={id}, old={old}, new={text}, topic={}, subtopic={}", action.topic, action.subtopic);
                changed += 1;
            }
            MemoryActionKind::Delete => {
                let Some(id) = action.target_id.as_deref() else {
                    log_warn!("memory store DELETE skipped: event_id={event_id}, missing target_id");
                    continue;
                };
                let old: Option<String> = tx
                    .query_row(
                        "SELECT text FROM memories WHERE id=? AND status='active'",
                        [id],
                        |r| r.get(0),
                    )
                    .optional()?;
                if let Some(old) = old {
                    tx.execute(
                        "UPDATE memories SET status='deleted',updated_at=? WHERE id=?",
                        params![now, id],
                    )?;
                    revision(&tx, id, "DELETE", Some(&old), None, event_id, now)?;
                    log_info!("memory store DELETE: event_id={event_id}, memory_id={id}, old={old}");
                    changed += 1;
                } else {
                    log_warn!("memory store DELETE skipped: event_id={event_id}, target_not_found={id}");
                }
            }
        }
    }
    tx.execute(
        "UPDATE memory_events SET observer_result=?,status='applied',processed_at=? WHERE id=?",
        params![raw_result, now, event_id],
    )?;
    tx.commit()?;
    Ok(changed)
}

pub fn mark_recalled(conn: &Connection, ids: &[String]) -> rusqlite::Result<()> {
    let now = chrono::Utc::now().timestamp_millis();
    for id in ids {
        conn.execute(
            "UPDATE memories SET recall_count=recall_count+1,last_recalled_at=? WHERE id=?",
            params![now, id],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedding_round_trip() {
        let v = vec![0.1, -2.5, 3.0];
        assert_eq!(decode_embedding(Some(encode_embedding(&v))).unwrap(), v);
    }

    #[test]
    fn actions_are_atomic_and_revisioned() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::db::open(&dir.path().join("memory.db")).unwrap();
        let event = begin_event(&conn, 1, 2, "用户：我喜欢低糖食品").unwrap();
        let append = MemoryAction {
            action: MemoryActionKind::Append,
            target_id: None,
            topic: "兴趣爱好".into(),
            subtopic: "饮食".into(),
            content: "用户偏好低糖食品".into(),
            memory_type: "preference".into(),
            importance: 0.7,
            confidence: 0.95,
            reason: "新增长期偏好".into(),
        };
        assert_eq!(
            apply_actions(
                &mut conn,
                &event.id,
                &[append],
                "{}",
                "test",
                &[Some(vec![1.0, 0.0])]
            )
            .unwrap(),
            1
        );
        let stored = list_active(&conn, 10).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].embedding.as_deref(), Some(&[1.0, 0.0][..]));
        let revisions: i64 = conn
            .query_row("SELECT COUNT(*) FROM memory_revisions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(revisions, 1);
    }
}
