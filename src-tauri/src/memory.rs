use crate::db::{self, DbState, Memory};
use serde_json::Value;

#[derive(Clone)]
pub struct MemoryConfig {
    pub qdrant_url: String,
    pub embedding_base_url: String,
    pub embedding_model: String,
    pub embedding_api_key: String,
    pub embedding_dimension: u32,
}

impl MemoryConfig {
    pub fn is_complete(&self) -> bool {
        !self.qdrant_url.is_empty()
            && !self.embedding_base_url.is_empty()
            && !self.embedding_model.is_empty()
            && !self.embedding_api_key.is_empty()
            && self.embedding_dimension > 0
    }
    fn collection(&self) -> String {
        let model: String = self
            .embedding_model
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        format!(
            "roxy_memories_{}_{}",
            model.trim_matches('_'),
            self.embedding_dimension
        )
    }
}

async fn embed(config: &MemoryConfig, text: &str) -> Result<Vec<f32>, String> {
    let response = reqwest::Client::new()
        .post(format!(
            "{}/embeddings",
            config.embedding_base_url.trim_end_matches('/')
        ))
        .bearer_auth(&config.embedding_api_key)
        .json(&serde_json::json!({"model":config.embedding_model,"input":text,"dimensions":config.embedding_dimension,"encoding_format":"float"}))
        .send()
        .await
        .map_err(|e| format!("Embedding 请求失败：{e}"))?;
    if !response.status().is_success() {
        return Err(format!("Embedding API 返回 {}", response.status()));
    }
    let value: Value = response
        .json()
        .await
        .map_err(|e| format!("Embedding 响应无效：{e}"))?;
    let vector: Vec<f32> = serde_json::from_value(
        value
            .pointer("/data/0/embedding")
            .cloned()
            .ok_or("Embedding 响应缺少向量")?,
    )
    .map_err(|e| format!("Embedding 向量无效：{e}"))?;
    if vector.len() != config.embedding_dimension as usize {
        return Err(format!(
            "Embedding 维度不匹配：配置 {}，实际 {}",
            config.embedding_dimension,
            vector.len()
        ));
    }
    Ok(vector)
}

async fn ensure_collection(config: &MemoryConfig) -> Result<(), String> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/collections/{}",
        config.qdrant_url.trim_end_matches('/'),
        config.collection()
    );
    let exists = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("无法连接 Qdrant：{e}"))?;
    if exists.status().is_success() {
        return Ok(());
    }
    let response = client
        .put(&url)
        .json(
            &serde_json::json!({"vectors":{"size":config.embedding_dimension,"distance":"Cosine"}}),
        )
        .send()
        .await
        .map_err(|e| format!("创建 Qdrant collection 失败：{e}"))?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("Qdrant 创建 collection 返回 {}", response.status()))
    }
}

pub async fn health(config: &MemoryConfig) -> bool {
    if !config.is_complete() {
        return false;
    }
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
    else {
        return false;
    };
    client
        .get(format!(
            "{}/collections",
            config.qdrant_url.trim_end_matches('/')
        ))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

pub async fn remember(
    db_state: &DbState,
    config: &MemoryConfig,
    text: &str,
    memory_type: &str,
    importance: f64,
    source_message_id: Option<&str>,
) -> Result<Memory, String> {
    if !config.is_complete() {
        return Err("长期记忆尚未配置".into());
    }
    let text = text.trim();
    if text.is_empty() {
        return Err("记忆内容不能为空".into());
    }
    let memory = {
        let conn = db_state.0.lock().await;
        db::insert_memory(
            &conn,
            text,
            memory_type,
            importance.clamp(0.0, 1.0),
            source_message_id,
        )
        .map_err(|e| e.to_string())?
    };
    let result = async {
        let vector = embed(config, text).await?;
        ensure_collection(config).await?;
        let response = reqwest::Client::new().put(format!("{}/collections/{}/points?wait=true", config.qdrant_url.trim_end_matches('/'), config.collection())).json(&serde_json::json!({"points":[{"id":memory.id,"vector":vector,"payload":{"text":memory.text,"memory_type":memory.memory_type,"importance":memory.importance,"status":"active","created_at":memory.created_at}}]})).send().await.map_err(|e| format!("写入 Qdrant 失败：{e}"))?;
        if !response.status().is_success() { return Err(format!("Qdrant upsert 返回 {}", response.status())); }
        Ok::<(),String>(())
    }.await;
    if let Err(e) = &result {
        log_warn!("memory::remember embedding failed for id={}: {e}", memory.id);
    }
    let conn = db_state.0.lock().await;
    db::set_memory_embedding_status(
        &conn,
        &memory.id,
        if result.is_ok() { "ready" } else { "failed" },
        Some(&config.collection()),
    )
    .map_err(|e| e.to_string())?;
    result?;
    Ok(memory)
}

pub async fn recall(config: &MemoryConfig, query: &str) -> Result<Vec<String>, String> {
    if !config.is_complete() {
        return Ok(vec![]);
    }
    let vector = embed(config, query).await?;
    ensure_collection(config).await?;
    let response = reqwest::Client::new().post(format!("{}/collections/{}/points/search", config.qdrant_url.trim_end_matches('/'), config.collection())).json(&serde_json::json!({"vector":vector,"limit":6,"with_payload":true,"filter":{"must":[{"key":"status","match":{"value":"active"}}]}})).send().await.map_err(|e| format!("检索 Qdrant 失败：{e}"))?;
    if !response.status().is_success() {
        return Err(format!("Qdrant 检索返回 {}", response.status()));
    }
    let value: Value = response
        .json()
        .await
        .map_err(|e| format!("Qdrant 响应无效：{e}"))?;
    Ok(value
        .get("result")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            item.pointer("/payload/text")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "requires Qdrant runtime and embedding credentials"]
    async fn memory_survives_qdrant_restart() {
        let runtime = crate::qdrant_runtime::QdrantRuntime::default();
        let config = MemoryConfig {
            qdrant_url: "http://127.0.0.1:6333".into(),
            embedding_base_url: std::env::var("EMBEDDING_BASE_URL").unwrap(),
            embedding_model: std::env::var("EMBEDDING_MODEL").unwrap(),
            embedding_api_key: std::env::var("EMBEDDING_API_KEY").unwrap(),
            embedding_dimension: std::env::var("EMBEDDING_DIMENSION")
                .unwrap()
                .parse()
                .unwrap(),
        };
        crate::qdrant_runtime::ensure(&runtime, &config.qdrant_url)
            .await
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let db = DbState(tokio::sync::Mutex::new(
            db::open(&dir.path().join("memory.db")).unwrap(),
        ));
        let marker = format!("ChatPet持久化测试记忆{}", uuid::Uuid::new_v4());
        remember(&db, &config, &marker, "fact", 0.8, None)
            .await
            .unwrap();
        crate::qdrant_runtime::shutdown(&runtime).await;
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        crate::qdrant_runtime::ensure(&runtime, &config.qdrant_url)
            .await
            .unwrap();
        let recalled = recall(&config, &marker).await.unwrap();
        assert!(recalled.iter().any(|text| text == &marker));
        crate::qdrant_runtime::shutdown(&runtime).await;
    }
}
