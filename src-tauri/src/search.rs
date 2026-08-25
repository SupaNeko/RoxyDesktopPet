//! 联网搜索：为旁路任务代理（subagent）提供内置的 web_search 工具。
//!
//! 接入新服务商只需在 SearchProvider 增加变体并实现其请求/解析；
//! 后续链路统一走 web_search()，配置层以外无差异。

use crate::db::AppSettings;

const MAX_RESULTS: usize = 5;
const MAX_OUTPUT_CHARS: usize = 3000;
const REQUEST_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchProvider {
    Bocha,
    Tavily,
}

impl SearchProvider {
    pub fn from_str(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "bocha" => Some(Self::Bocha),
            "tavily" => Some(Self::Tavily),
            _ => None,
        }
    }

    pub fn default_url(&self) -> &'static str {
        match self {
            Self::Bocha => "https://api.bochaai.com/v1/web-search",
            Self::Tavily => "https://api.tavily.com/search",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchConfig {
    pub provider: SearchProvider,
    pub api_key: String,
    pub base_url: String,
}

impl SearchConfig {
    /// 从应用设置构造；未启用或缺少 API Key 时返回 None。
    /// 主会话与 subagent 都用它判断当前是否具备联网能力。
    pub fn from_settings(settings: &AppSettings) -> Option<Self> {
        let provider = SearchProvider::from_str(&settings.search_provider)?;
        let api_key = settings.search_api_key.trim().to_string();
        if api_key.is_empty() {
            return None;
        }
        let raw = settings.search_base_url.trim().trim_end_matches('/');
        let base_url = if raw.is_empty() {
            provider.default_url().to_string()
        } else {
            raw.to_string()
        };
        Some(Self {
            provider,
            api_key,
            base_url,
        })
    }
}

/// subagent 工具循环中注入的 web_search 工具 schema（OpenAI tools 格式）。
pub fn web_search_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "web_search",
            "description": "联网搜索，获取实时或时效性信息（天气、新闻、资料等）。输入关键词，返回相关网页的标题、链接与摘要。",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "搜索关键词或问题"}
                },
                "required": ["query"],
                "additionalProperties": false
            }
        }
    })
}

#[derive(Debug, PartialEq)]
struct SearchResultItem {
    title: String,
    url: String,
    snippet: String,
}

/// 统一入口：按 provider 发请求、解析为通用结果，再统一格式化输出。
pub async fn web_search(
    client: &reqwest::Client,
    config: &SearchConfig,
    query: &str,
) -> Result<String, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("搜索关键词不能为空".into());
    }
    let items = match config.provider {
        SearchProvider::Bocha => bocha_search(client, config, query).await?,
        SearchProvider::Tavily => tavily_search(client, config, query).await?,
    };
    Ok(format_results(items))
}

async fn post_search(
    client: &reqwest::Client,
    config: &SearchConfig,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let response = client
        .post(&config.base_url)
        .bearer_auth(&config.api_key)
        .json(&body)
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .send()
        .await
        .map_err(|e| format!("搜索请求失败：{e}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| format!("读取搜索响应失败：{e}"))?;
    if !status.is_success() {
        return Err(format!(
            "搜索服务返回 {status}：{}",
            text.chars().take(200).collect::<String>()
        ));
    }
    serde_json::from_str(&text).map_err(|e| format!("解析搜索响应失败：{e}"))
}

async fn bocha_search(
    client: &reqwest::Client,
    config: &SearchConfig,
    query: &str,
) -> Result<Vec<SearchResultItem>, String> {
    let body = serde_json::json!({"query": query, "summary": true, "count": MAX_RESULTS});
    let json = post_search(client, config, body).await?;
    Ok(parse_bocha_results(&json))
}

fn parse_bocha_results(json: &serde_json::Value) -> Vec<SearchResultItem> {
    json.pointer("/data/webPages/value")
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let title = row.get("name")?.as_str()?.trim().to_string();
                    let url = row
                        .get("url")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    // 有 AI 摘要时优先用摘要，信息密度更高。
                    let snippet = ["summary", "snippet"]
                        .iter()
                        .filter_map(|key| row.get(key).and_then(|v| v.as_str()))
                        .map(str::trim)
                        .find(|s| !s.is_empty())
                        .unwrap_or_default()
                        .to_string();
                    Some(SearchResultItem {
                        title,
                        url,
                        snippet,
                    })
                })
                .take(MAX_RESULTS)
                .collect()
        })
        .unwrap_or_default()
}

async fn tavily_search(
    client: &reqwest::Client,
    config: &SearchConfig,
    query: &str,
) -> Result<Vec<SearchResultItem>, String> {
    let body = serde_json::json!({"query": query, "max_results": MAX_RESULTS});
    let json = post_search(client, config, body).await?;
    Ok(parse_tavily_results(&json))
}

fn parse_tavily_results(json: &serde_json::Value) -> Vec<SearchResultItem> {
    json.get("results")
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let title = row.get("title")?.as_str()?.trim().to_string();
                    let url = row
                        .get("url")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    let snippet = row
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    Some(SearchResultItem {
                        title,
                        url,
                        snippet,
                    })
                })
                .take(MAX_RESULTS)
                .collect()
        })
        .unwrap_or_default()
}

fn format_results(items: Vec<SearchResultItem>) -> String {
    if items.is_empty() {
        return "未找到相关结果。".into();
    }
    let mut out = String::from("搜索结果：");
    for (index, item) in items.iter().enumerate() {
        let block = format!(
            "\n{}. {}\n   {}\n   {}",
            index + 1,
            item.title,
            item.url,
            item.snippet
        );
        if out.chars().count() + block.chars().count() > MAX_OUTPUT_CHARS {
            out.push_str("\n（更多结果已截断）");
            break;
        }
        out.push_str(&block);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with(provider: &str, key: &str, base_url: &str) -> AppSettings {
        AppSettings {
            search_provider: provider.into(),
            search_api_key: key.into(),
            search_base_url: base_url.into(),
            ..AppSettings::default()
        }
    }

    #[test]
    fn from_settings_disabled_without_provider_or_key() {
        assert!(SearchConfig::from_settings(&settings_with("", "k", "")).is_none());
        assert!(SearchConfig::from_settings(&settings_with("bocha", "", "")).is_none());
        assert!(SearchConfig::from_settings(&settings_with("unknown", "k", "")).is_none());
    }

    #[test]
    fn from_settings_uses_default_url_and_trims_custom() {
        let config = SearchConfig::from_settings(&settings_with("bocha", "k", "")).unwrap();
        assert_eq!(config.provider, SearchProvider::Bocha);
        assert_eq!(config.base_url, "https://api.bochaai.com/v1/web-search");
        let config =
            SearchConfig::from_settings(&settings_with("tavily", "k", " https://proxy.example.com/ "))
                .unwrap();
        assert_eq!(config.base_url, "https://proxy.example.com");
    }

    #[test]
    fn parses_bocha_results_preferring_summary() {
        let json = serde_json::json!({
            "code": 200,
            "data": {"webPages": {"value": [
                {"name": "标题一", "url": "https://a.example", "snippet": "摘要一", "summary": "AI 摘要一"},
                {"name": "标题二", "url": "https://b.example", "snippet": "摘要二"}
            ]}}
        });
        let items = parse_bocha_results(&json);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].snippet, "AI 摘要一");
        assert_eq!(items[1].snippet, "摘要二");
        assert!(parse_bocha_results(&serde_json::json!({"code": 404})).is_empty());
    }

    #[test]
    fn parses_tavily_results() {
        let json = serde_json::json!({
            "results": [{"title": "标题", "url": "https://a.example", "content": "内容"}]
        });
        let items = parse_tavily_results(&json);
        assert_eq!(
            items,
            vec![SearchResultItem {
                title: "标题".into(),
                url: "https://a.example".into(),
                snippet: "内容".into()
            }]
        );
        assert!(parse_tavily_results(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn format_results_handles_empty_and_truncates() {
        assert_eq!(format_results(Vec::new()), "未找到相关结果。");
        let long = (0..MAX_RESULTS)
            .map(|i| SearchResultItem {
                title: format!("标题{i}"),
                url: "https://a.example".into(),
                snippet: "长".repeat(1000),
            })
            .collect();
        let out = format_results(long);
        assert!(out.contains("已截断"));
        assert!(out.chars().count() <= MAX_OUTPUT_CHARS + 32);
    }
}
