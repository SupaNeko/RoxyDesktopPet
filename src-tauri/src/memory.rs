use crate::db::DbState;
use crate::memory_store::StoredMemory;
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Clone)]
pub struct MemoryConfig {
    pub embedding_base_url: String,
    pub embedding_model: String,
    pub embedding_api_key: String,
    pub embedding_dimension: u32,
}
impl MemoryConfig {
    pub fn is_complete(&self) -> bool {
        !self.embedding_base_url.is_empty()
            && !self.embedding_model.is_empty()
            && !self.embedding_api_key.is_empty()
            && self.embedding_dimension > 0
    }
}
#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingItem>,
}
#[derive(Deserialize)]
struct EmbeddingItem {
    embedding: Vec<f32>,
}

pub async fn embed(config: &MemoryConfig, text: &str) -> Result<Vec<f32>, String> {
    if !config.is_complete() {
        return Err("长期记忆 embedding 尚未配置".into());
    }
    let response = reqwest::Client::new()
        .post(format!(
            "{}/embeddings",
            config.embedding_base_url.trim_end_matches('/')
        ))
        .bearer_auth(&config.embedding_api_key)
        .json(&serde_json::json!({"model":config.embedding_model,"input":text}))
        .send()
        .await
        .map_err(|e| format!("Embedding 请求失败：{e}"))?;
    if !response.status().is_success() {
        return Err(format!("Embedding 返回 {}", response.status()));
    }
    let value: EmbeddingResponse = response
        .json()
        .await
        .map_err(|e| format!("Embedding 响应无效：{e}"))?;
    let vector = value
        .data
        .into_iter()
        .next()
        .map(|v| v.embedding)
        .ok_or("Embedding 响应缺少向量")?;
    if vector.len() != config.embedding_dimension as usize {
        return Err(format!(
            "Embedding 维度不匹配：配置 {}，实际 {}",
            config.embedding_dimension,
            vector.len()
        ));
    }
    Ok(vector)
}
pub async fn health(config: &MemoryConfig) -> bool {
    config.is_complete()
}
fn cosine(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut aa, mut bb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        let (x, y) = (*x as f64, *y as f64);
        dot += x * y;
        aa += x * x;
        bb += y * y
    }
    if aa == 0.0 || bb == 0.0 {
        0.0
    } else {
        dot / (aa.sqrt() * bb.sqrt())
    }
}
fn terms(text: &str) -> HashSet<String> {
    let chars: Vec<char> = text
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_ascii_punctuation())
        .collect();
    let mut out = HashSet::new();
    for c in &chars {
        out.insert(c.to_string());
    }
    for w in chars.windows(2) {
        out.insert(w.iter().collect());
    }
    out
}
fn lexical(query: &HashSet<String>, text: &str) -> f64 {
    if query.is_empty() {
        return 0.0;
    }
    let other = terms(text);
    query.intersection(&other).count() as f64 / query.len() as f64
}
fn freshness(updated: i64, now: i64) -> f64 {
    let days = ((now - updated).max(0) as f64) / 86_400_000.0;
    (-days / 180.0).exp()
}

const MIN_SEMANTIC: f64 = 0.42;
const MIN_RELEVANCE: f64 = 0.44;
const EXACT_LEXICAL: f64 = 0.55;
const MAX_RELATIVE_GAP: f64 = 0.08;
const MAX_RECALL_RESULTS: usize = 4;

fn relevance(semantic: f64, lexical: f64) -> f64 {
    semantic * 0.82 + lexical * 0.18
}

fn is_relevant(semantic: f64, lexical: f64) -> bool {
    (semantic >= MIN_SEMANTIC && relevance(semantic, lexical) >= MIN_RELEVANCE)
        || lexical >= EXACT_LEXICAL
}

pub async fn recall(
    db: &DbState,
    config: &MemoryConfig,
    query: &str,
) -> Result<Vec<String>, String> {
    log_info!("memory recall started: query={query}");
    if query.trim().is_empty() {
        log_info!("memory recall skipped: empty query");
        return Ok(vec![]);
    }
    if !config.is_complete() {
        log_warn!("memory recall skipped: embedding not configured");
        return Ok(vec![]);
    }
    log_info!("memory recall step=embedding request: model={}, dimensions={}, query={query}", config.embedding_model, config.embedding_dimension);
    let query_vector = embed(config, query).await?;
    log_info!("memory recall step=embedding ready: actual_dimensions={}", query_vector.len());
    let memories = {
        let conn = db.0.lock().await;
        crate::memory_store::list_active(&conn, 5000).map_err(|e| e.to_string())?
    };
    log_info!("memory recall step=sqlite candidates loaded: count={}", memories.len());
    let query_terms = terms(query);
    log_info!("memory recall step=lexical terms built: count={}, terms={:?}", query_terms.len(), query_terms);
    let now = chrono::Utc::now().timestamp_millis();
    let candidate_count = memories.len();
    let mut ranked: Vec<(f64, StoredMemory)> = Vec::new();
    for memory in memories {
        let semantic = memory.embedding.as_ref().map(|v| cosine(&query_vector, v)).unwrap_or(0.0).max(0.0);
        let keyword = lexical(&query_terms, &memory.text);
        let relevance_score = relevance(semantic, keyword);
        let freshness_score = freshness(memory.updated_at, now);
        let recall_boost = ((memory.recall_count as f64 + 1.0).ln() / 10.0).min(1.0);
        let score = semantic * 0.62
            + keyword * 0.18
            + memory.importance * 0.10
            + memory.confidence * 0.05
            + freshness_score * 0.03
            + recall_boost * 0.02;
        let accepted = is_relevant(semantic, keyword);
        log_info!("memory recall candidate: id={}, accepted={}, semantic={:.6}, lexical={:.6}, relevance={:.6}, importance={:.6}, confidence={:.6}, freshness={:.6}, recall_boost={:.6}, final_score={:.6}, topic={}, subtopic={}, content={}", memory.id, accepted, semantic, keyword, relevance_score, memory.importance, memory.confidence, freshness_score, recall_boost, score, memory.topic, memory.subtopic, memory.text);
        if accepted {
            ranked.push((score, memory));
        }
    }
    log_info!("memory recall step=absolute relevance filter: input={}, accepted={}, min_semantic={}, min_relevance={}, exact_lexical={}", candidate_count, ranked.len(), MIN_SEMANTIC, MIN_RELEVANCE, EXACT_LEXICAL);
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (index, (score, memory)) in ranked.iter().enumerate() {
        log_info!("memory recall sorted[{}]: id={}, score={:.6}, content={}", index + 1, memory.id, score, memory.text);
    }
    if let Some((best_score, _)) = ranked.first() {
        let best_score = *best_score;
        let relative_floor = best_score - MAX_RELATIVE_GAP;
        ranked.retain(|(score, _)| *score >= relative_floor);
        log_info!("memory recall step=relative score filter: best_score={:.6}, max_gap={}, relative_floor={:.6}, remaining={}", best_score, MAX_RELATIVE_GAP, relative_floor, ranked.len());
    }
    ranked.truncate(MAX_RECALL_RESULTS);
    let ids = ranked.iter().map(|(_, m)| m.id.clone()).collect::<Vec<_>>();
    log_info!("memory recall step=topk selected: topk={}, selected={}, ids={:?}", MAX_RECALL_RESULTS, ids.len(), ids);
    {
        let conn = db.0.lock().await;
        crate::memory_store::mark_recalled(&conn, &ids).map_err(|e| e.to_string())?;
    }
    let results = ranked
        .into_iter()
        .map(|(_, m)| format!("[{} / {}] {}", m.topic, m.subtopic, m.text))
        .collect::<Vec<_>>();
    log_info!("memory recall completed: query={query}, result_count={}, results={:?}", results.len(), results);
    Ok(results)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cosine_identity() {
        assert!((cosine(&[1.0, 2.0], &[1.0, 2.0]) - 1.0).abs() < 1e-6)
    }
    #[test]
    fn lexical_matches_chinese() {
        assert!(lexical(&terms("低糖食物"), "用户不喜欢甜食，偏好低糖") > 0.2)
    }
    #[test]
    fn freshness_decays() {
        assert!(freshness(0, 0) > freshness(0, 86_400_000 * 365))
    }
    #[test]
    fn weak_semantic_match_is_rejected() {
        assert!(!is_relevant(0.40, 0.10));
        assert!(!is_relevant(0.43, 0.20));
    }
    #[test]
    fn strong_semantic_or_exact_lexical_match_is_accepted() {
        assert!(is_relevant(0.55, 0.10));
        assert!(is_relevant(0.20, 0.60));
    }
}
