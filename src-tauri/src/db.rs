use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio::sync::Mutex;

pub struct DbState(pub Mutex<Connection>);

/// 编程联动中单个工具的文本配置（固定提示文本 + 语音文本）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolHookToolText {
    #[serde(default)]
    pub fixed_text: String,
    #[serde(default)]
    pub fixed_voice_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub pet_name: String,
    pub persona: String,
    pub user_name: String,
    pub api_base_url: String,
    pub api_model: String,
    pub api_key_configured: bool,
    pub voice_output_enabled: bool,
    pub voice_output_mode: String,
    pub tts_api_protocol: String,
    pub tts_api_base_url: String,
    pub tts_api_model: String,
    pub tts_api_key: String,
    pub tts_api_configured: bool,
    pub tts_api_voice: String,
    pub tts_api_language: String,
    pub vits_model_name: String,
    pub vits_model_path: String,
    pub vits_speaker_id: Option<String>,
    pub vits_target_language: String,
    pub vits_speed: f64,
    pub vits_emotion_params: String,
    pub vits_translate_enabled: bool,
    pub microphone_device_name: Option<String>,
    pub asr_app_id: String,
    pub asr_api_key: String,
    pub asr_api_secret: String,
    pub asr_configured: bool,
    pub proactive_enabled: bool,
    pub proactive_min_minutes: u32,
    pub proactive_max_minutes: u32,
    pub proactive_daily_limit: u32,
    pub qdrant_url: String,
    pub embedding_base_url: String,
    pub embedding_model: String,
    pub embedding_api_key: String,
    pub embedding_dimension: u32,
    pub memory_configured: bool,
    pub memory_observer_enabled: bool,
    pub memory_observer_interval: u32,
    pub tool_hook_enabled: bool,
    pub tool_hook_mode: String,
    pub tool_hook_port: u32,
    /// 按工具独立存储的文本配置（JSON map 入库，key 为工具 id）。
    pub tool_hook_tool_texts: HashMap<String, ToolHookToolText>,
    pub tool_hook_include_last_message: bool,
    pub tool_hook_min_interval_minutes: u32,
    pub tool_hook_daily_limit: u32,
    pub tool_hook_debounce_seconds: u32,
    pub tool_hook_voice_enabled: bool,
    pub system_status_enabled: bool,
    pub taskbar_apps_enabled: bool,
    pub now_playing_enabled: bool,
    pub voice_input_mode: String,
    pub push_to_talk_shortcut: String,
    pub pet_show_on_fullscreen: bool,
    /// 桌宠当前服装 id（"default" 为默认服装，无表情差分的服装由前端回退到平静表情图）
    pub pet_outfit: String,
    /// 联网搜索服务商："" 关闭 | "bocha" | "tavily"
    pub search_provider: String,
    pub search_api_key: String,
    pub search_base_url: String,
    pub search_api_configured: bool,
    /// 后台代理最大工具调用轮数
    pub agent_max_tool_rounds: u32,
    /// 是否登录 Windows 后自动启动（默认关闭）。
    pub autostart_enabled: bool,
    /// 日语学习模式总开关（默认关闭）。
    pub study_enabled: bool,
    /// 学习会话是否与主会话隔离（默认开启）。
    pub study_isolated: bool,
    /// 主动复习出题开关（默认开启，需学习模式开启才生效）。
    pub study_review_enabled: bool,
    /// 主动复习间隔区间（分钟）。
    pub study_review_min_minutes: i64,
    pub study_review_max_minutes: i64,
    /// 每日主动复习出题上限。
    pub study_review_daily_limit: i64,
    /// 启用的题型（逗号分隔：meaning 释义 / spelling 拼写 / reading 读音，默认全部）。
    pub study_quiz_types: String,
    /// 持续出题模式：开启后不使用间隔复习，而是答完一题立即出下一题（默认关闭）。
    pub study_continuous_enabled: bool,
    /// 答题气泡显示时间（秒）：超时气泡消失后该题视为放弃（默认 60）。
    pub study_quiz_ttl_seconds: i64,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            pet_name: "小栞".into(),
            persona: "温柔、克制、偶尔有一点俏皮的陪伴者。".into(),
            user_name: "你".into(),
            api_base_url: String::new(),
            api_model: String::new(),
            api_key_configured: false,
            voice_output_enabled: false,
            voice_output_mode: "disabled".into(),
            tts_api_protocol: "dashscope".into(),
            tts_api_base_url: "https://dashscope.aliyuncs.com/api/v1".into(),
            tts_api_model: "qwen3-tts-flash".into(),
            tts_api_key: String::new(),
            tts_api_configured: false,
            tts_api_voice: "Cherry".into(),
            tts_api_language: "Chinese".into(),
            vits_model_name: String::new(),
            vits_model_path: String::new(),
            vits_speaker_id: None,
            vits_target_language: "ja".into(),
            vits_speed: 1.0,
            vits_emotion_params: String::new(),
            vits_translate_enabled: true,
            microphone_device_name: None,
            asr_app_id: String::new(),
            asr_api_key: String::new(),
            asr_api_secret: String::new(),
            asr_configured: false,
            proactive_enabled: false,
            proactive_min_minutes: 45,
            proactive_max_minutes: 120,
            proactive_daily_limit: 6,
            qdrant_url: "http://127.0.0.1:6333".into(),
            embedding_base_url: String::new(),
            embedding_model: String::new(),
            embedding_api_key: String::new(),
            embedding_dimension: 0,
            memory_configured: false,
            memory_observer_enabled: true,
            memory_observer_interval: 30,
            tool_hook_enabled: false,
            tool_hook_mode: "fixed".into(),
            tool_hook_port: 34125,
            tool_hook_tool_texts: HashMap::new(),
            tool_hook_include_last_message: true,
            tool_hook_min_interval_minutes: 10,
            tool_hook_daily_limit: 20,
            tool_hook_debounce_seconds: 0,
            tool_hook_voice_enabled: true,
            system_status_enabled: false,
            taskbar_apps_enabled: false,
            now_playing_enabled: false,
            voice_input_mode: "disabled".into(),
            push_to_talk_shortcut: String::new(),
            pet_show_on_fullscreen: true,
            pet_outfit: "default".into(),
            search_provider: String::new(),
            search_api_key: String::new(),
            search_base_url: String::new(),
            search_api_configured: false,
            agent_max_tool_rounds: 30,
            autostart_enabled: false,
            study_enabled: false,
            study_isolated: true,
            study_review_enabled: true,
            study_review_min_minutes: 30,
            study_review_max_minutes: 90,
            study_review_daily_limit: 8,
            study_quiz_types: "meaning,spelling,reading".into(),
            study_continuous_enabled: false,
            study_quiz_ttl_seconds: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub japanese_text: Option<String>,
    #[serde(default)]
    pub emotion: Option<String>,
    pub trigger_type: String,
    pub created_at: i64,
    /// 会话隔离标识："main" 主会话 | "study" 学习会话。
    #[serde(default = "default_session")]
    pub session: String,
    /// 选择题视图（发给前端渲染，不含 correct_index；无题目时整个字段不序列化）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quiz: Option<QuizView>,
}

fn default_session() -> String {
    "main".into()
}

/// 发给前端的选题视图：刻意不含 correct_index，判定完全在 Rust 端完成。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizView {
    pub quiz_id: String,
    pub question: String,
    pub options: Vec<String>,
}

/// 日语学习模式的单词组（一个 xlsx 文件 = 一个组，文件名即组名）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordGroup {
    pub id: String,
    pub name: String,
    pub file_name: String,
    pub enabled: bool,
    pub word_count: i64,
    /// 各掌握度计数（统计派生，供设置页进度条展示）。
    pub mastered: i64,
    pub shaky: i64,
    pub forgotten: i64,
    pub unlearned: i64,
    /// 对应的 xlsx 文件已不在 wordgroups 目录中。
    pub file_missing: bool,
}

/// 单词条目：答题计数入库，掌握度（mastery）为查询时派生。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordItem {
    pub id: i64,
    pub group_id: String,
    pub word: String,
    pub kana: String,
    pub meaning: String,
    pub correct_count: i64,
    pub wrong_count: i64,
    pub streak: i64,
    pub last_outcome: Option<String>,
    pub last_reviewed_at: Option<i64>,
    /// 派生掌握度标签：已掌握 / 勉强记得 / 不记得 / 未学。
    pub mastery: String,
}

/// 选题作答记录：出题即插入（selected_index 为空），作答时回填。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizRecord {
    pub id: String,
    pub word_id: i64,
    pub group_id: String,
    pub quiz_type: String,
    pub question: String,
    pub options: Vec<String>,
    pub correct_index: i64,
    pub selected_index: Option<i64>,
    pub is_correct: Option<bool>,
    /// 出题来源："chat" 主会话/学习会话对话中出题 | "study_review" 主动复习出题。
    pub source: String,
    pub created_at: i64,
    pub answered_at: Option<i64>,
    /// 目标词本体（查询时 LEFT JOIN words 带出，供历史页回顾；词已删除时为空串）。
    #[serde(default)]
    pub word: String,
    #[serde(default)]
    pub kana: String,
    #[serde(default)]
    pub meaning: String,
}

/// answer_quiz_record 的结果：already_answered=true 表示重复作答，直接返回已落库的结果。
#[derive(Debug, Clone)]
pub struct QuizAnswerResult {
    pub record: QuizRecord,
    pub already_answered: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id: String,
    pub title: String,
    pub due_at_utc: i64,
    pub timezone: String,
    pub status: String,
    #[serde(default)]
    pub repeat_interval_minutes: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub id: i64,
    pub name: String,
    /// "stdio" | "remote"
    pub transport: String,
    pub command: String,
    /// 空格分隔的参数（stdio 用）
    pub args: String,
    /// 每行一条 KEY=VALUE（stdio 用）
    pub env: String,
    pub url: String,
    /// 每行一条 KEY=VALUE（remote 请求头用）
    pub headers: String,
    /// 单次工具调用超时（秒）
    pub timeout_seconds: u32,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct McpServerInput {
    pub name: String,
    pub transport: String,
    pub command: String,
    pub args: String,
    pub env: String,
    pub url: String,
    pub headers: String,
    pub timeout_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRun {
    pub id: i64,
    pub task: String,
    /// "running" | "done" | "error"
    pub status: String,
    pub summary: String,
    pub created_at: i64,
    pub finished_at: Option<i64>,
}

/// 单个分类的 API 消耗汇总（今日 / 累计两段）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSummary {
    pub category: String,
    pub calls_today: i64,
    pub prompt_today: i64,
    pub completion_today: i64,
    pub calls_total: i64,
    pub prompt_total: i64,
    pub completion_total: i64,
}

#[derive(Debug, Clone)]
pub struct DueReminder {
    pub occurrence_id: String,
    pub todo_id: String,
    pub title: String,
    pub scheduled_at_utc: i64,
}

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            pet_name TEXT NOT NULL,
            persona TEXT NOT NULL,
            user_name TEXT NOT NULL,
            api_base_url TEXT NOT NULL,
            api_model TEXT NOT NULL,
            voice_output_enabled INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            trigger_type TEXT NOT NULL,
            created_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS todos (id TEXT PRIMARY KEY, title TEXT NOT NULL, due_at_utc INTEGER NOT NULL, timezone TEXT NOT NULL, status TEXT NOT NULL, source_message_id TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS reminder_occurrences (id TEXT PRIMARY KEY, todo_id TEXT NOT NULL, scheduled_at_utc INTEGER NOT NULL, status TEXT NOT NULL, attempt_count INTEGER NOT NULL DEFAULT 0, lease_until INTEGER, delivered_at INTEGER, last_error TEXT, UNIQUE(todo_id, scheduled_at_utc));
         CREATE TABLE IF NOT EXISTS scheduler_state (id INTEGER PRIMARY KEY CHECK (id = 1), next_proactive_at INTEGER, proactive_day TEXT, proactive_count INTEGER NOT NULL DEFAULT 0);
         CREATE TABLE IF NOT EXISTS memories (id TEXT PRIMARY KEY, topic TEXT NOT NULL DEFAULT '其他', subtopic TEXT NOT NULL DEFAULT '一般', text TEXT NOT NULL, memory_type TEXT NOT NULL, importance REAL NOT NULL, confidence REAL NOT NULL DEFAULT 0.8, status TEXT NOT NULL, embedding BLOB, embedding_model TEXT, embedding_dimension INTEGER NOT NULL DEFAULT 0, embedding_collection TEXT, embedding_status TEXT NOT NULL DEFAULT 'pending', source_message_id TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, last_recalled_at INTEGER, recall_count INTEGER NOT NULL DEFAULT 0);
         CREATE TABLE IF NOT EXISTS memory_events (id TEXT PRIMARY KEY, first_message_rowid INTEGER NOT NULL, last_message_rowid INTEGER NOT NULL, transcript TEXT NOT NULL, observer_result TEXT, status TEXT NOT NULL, error TEXT, created_at INTEGER NOT NULL, processed_at INTEGER);
         CREATE TABLE IF NOT EXISTS memory_revisions (id TEXT PRIMARY KEY, memory_id TEXT NOT NULL, action TEXT NOT NULL, old_text TEXT, new_text TEXT, source_event_id TEXT, created_at INTEGER NOT NULL, FOREIGN KEY(memory_id) REFERENCES memories(id));
         CREATE TABLE IF NOT EXISTS memory_observer_state (id INTEGER PRIMARY KEY CHECK (id = 1), last_message_rowid INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL DEFAULT 0);
         CREATE TABLE IF NOT EXISTS user_preference_summary (id INTEGER PRIMARY KEY CHECK (id = 1), text TEXT NOT NULL DEFAULT '', updated_at INTEGER NOT NULL DEFAULT 0);
         INSERT OR IGNORE INTO scheduler_state(id, proactive_count) VALUES(1, 0);
         INSERT OR IGNORE INTO memory_observer_state(id) VALUES(1);
         INSERT OR IGNORE INTO user_preference_summary(id) VALUES(1);
         CREATE TABLE IF NOT EXISTS mcp_servers (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, transport TEXT NOT NULL, command TEXT NOT NULL DEFAULT '', args TEXT NOT NULL DEFAULT '', env TEXT NOT NULL DEFAULT '', url TEXT NOT NULL DEFAULT '', headers TEXT NOT NULL DEFAULT '', timeout_seconds INTEGER NOT NULL DEFAULT 120, enabled INTEGER NOT NULL DEFAULT 1, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS agent_runs (id INTEGER PRIMARY KEY AUTOINCREMENT, task TEXT NOT NULL, status TEXT NOT NULL, summary TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL, finished_at INTEGER);
         CREATE TABLE IF NOT EXISTS usage_events (id INTEGER PRIMARY KEY AUTOINCREMENT, created_at INTEGER NOT NULL, category TEXT NOT NULL, prompt_tokens INTEGER NOT NULL, completion_tokens INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS word_groups (id TEXT PRIMARY KEY, name TEXT NOT NULL, file_name TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 0, word_count INTEGER NOT NULL DEFAULT 0, imported_at INTEGER NOT NULL DEFAULT 0, file_missing INTEGER NOT NULL DEFAULT 0);
         CREATE TABLE IF NOT EXISTS words (id INTEGER PRIMARY KEY AUTOINCREMENT, group_id TEXT NOT NULL, word TEXT NOT NULL, kana TEXT NOT NULL DEFAULT '', meaning TEXT NOT NULL DEFAULT '', correct_count INTEGER NOT NULL DEFAULT 0, wrong_count INTEGER NOT NULL DEFAULT 0, streak INTEGER NOT NULL DEFAULT 0, last_outcome TEXT, last_reviewed_at INTEGER, UNIQUE(group_id, word, kana));
         CREATE TABLE IF NOT EXISTS study_scheduler_state (id INTEGER PRIMARY KEY CHECK (id = 1), next_review_at INTEGER, review_day TEXT, review_count INTEGER NOT NULL DEFAULT 0);
         INSERT OR IGNORE INTO study_scheduler_state(id, review_count) VALUES(1, 0);
         CREATE TABLE IF NOT EXISTS quiz_records (id TEXT PRIMARY KEY, word_id INTEGER NOT NULL, group_id TEXT NOT NULL, quiz_type TEXT NOT NULL, question TEXT NOT NULL, options_json TEXT NOT NULL, correct_index INTEGER NOT NULL, selected_index INTEGER, is_correct INTEGER, source TEXT NOT NULL, created_at INTEGER NOT NULL, answered_at INTEGER);
         CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(created_at);
         CREATE INDEX IF NOT EXISTS idx_usage_events_created_at ON usage_events(created_at);
         CREATE INDEX IF NOT EXISTS idx_occurrences_due ON reminder_occurrences(status, scheduled_at_utc);",
    )?;
    let message_columns: Vec<String> = conn
        .prepare("PRAGMA table_info(messages)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    if !message_columns.iter().any(|name| name == "japanese_text") {
        conn.execute("ALTER TABLE messages ADD COLUMN japanese_text TEXT", [])?;
    }
    if !message_columns.iter().any(|name| name == "emotion") {
        conn.execute("ALTER TABLE messages ADD COLUMN emotion TEXT", [])?;
    }
    if !message_columns.iter().any(|name| name == "session") {
        conn.execute(
            "ALTER TABLE messages ADD COLUMN session TEXT NOT NULL DEFAULT 'main'",
            [],
        )?;
    }
    if !message_columns.iter().any(|name| name == "quiz_json") {
        conn.execute("ALTER TABLE messages ADD COLUMN quiz_json TEXT", [])?;
    }
    let memory_columns: Vec<String> = conn
        .prepare("PRAGMA table_info(memories)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for (name, sql) in [
        (
            "topic",
            "ALTER TABLE memories ADD COLUMN topic TEXT NOT NULL DEFAULT '其他'",
        ),
        (
            "subtopic",
            "ALTER TABLE memories ADD COLUMN subtopic TEXT NOT NULL DEFAULT '一般'",
        ),
        (
            "confidence",
            "ALTER TABLE memories ADD COLUMN confidence REAL NOT NULL DEFAULT 0.8",
        ),
        (
            "embedding",
            "ALTER TABLE memories ADD COLUMN embedding BLOB",
        ),
        (
            "embedding_model",
            "ALTER TABLE memories ADD COLUMN embedding_model TEXT",
        ),
        (
            "embedding_dimension",
            "ALTER TABLE memories ADD COLUMN embedding_dimension INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "last_recalled_at",
            "ALTER TABLE memories ADD COLUMN last_recalled_at INTEGER",
        ),
        (
            "recall_count",
            "ALTER TABLE memories ADD COLUMN recall_count INTEGER NOT NULL DEFAULT 0",
        ),
    ] {
        if !memory_columns.iter().any(|column| column == name) {
            conn.execute(sql, [])?;
        }
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_memories_active ON memories(status, updated_at DESC);
         CREATE INDEX IF NOT EXISTS idx_memories_topic ON memories(topic, subtopic, status);",
    )?;
    let columns: Vec<String> = conn
        .prepare("PRAGMA table_info(app_settings)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    if !columns.iter().any(|name| name == "microphone_device_name") {
        conn.execute(
            "ALTER TABLE app_settings ADD COLUMN microphone_device_name TEXT",
            [],
        )?;
    }
    for (name, sql) in [
        (
            "asr_app_id",
            "ALTER TABLE app_settings ADD COLUMN asr_app_id TEXT NOT NULL DEFAULT ''",
        ),
        (
            "asr_api_key",
            "ALTER TABLE app_settings ADD COLUMN asr_api_key TEXT NOT NULL DEFAULT ''",
        ),
        (
            "asr_api_secret",
            "ALTER TABLE app_settings ADD COLUMN asr_api_secret TEXT NOT NULL DEFAULT ''",
        ),
        ("proactive_enabled", "ALTER TABLE app_settings ADD COLUMN proactive_enabled INTEGER NOT NULL DEFAULT 0"),
        ("proactive_min_minutes", "ALTER TABLE app_settings ADD COLUMN proactive_min_minutes INTEGER NOT NULL DEFAULT 45"),
        ("proactive_max_minutes", "ALTER TABLE app_settings ADD COLUMN proactive_max_minutes INTEGER NOT NULL DEFAULT 120"),
        ("proactive_daily_limit", "ALTER TABLE app_settings ADD COLUMN proactive_daily_limit INTEGER NOT NULL DEFAULT 6"),
        ("qdrant_url", "ALTER TABLE app_settings ADD COLUMN qdrant_url TEXT NOT NULL DEFAULT 'http://127.0.0.1:6333'"),
        ("embedding_base_url", "ALTER TABLE app_settings ADD COLUMN embedding_base_url TEXT NOT NULL DEFAULT ''"),
        ("embedding_model", "ALTER TABLE app_settings ADD COLUMN embedding_model TEXT NOT NULL DEFAULT ''"),
        ("embedding_api_key", "ALTER TABLE app_settings ADD COLUMN embedding_api_key TEXT NOT NULL DEFAULT ''"),
        ("embedding_dimension", "ALTER TABLE app_settings ADD COLUMN embedding_dimension INTEGER NOT NULL DEFAULT 0"),
        ("memory_observer_enabled", "ALTER TABLE app_settings ADD COLUMN memory_observer_enabled INTEGER NOT NULL DEFAULT 1"),
        ("memory_observer_interval", "ALTER TABLE app_settings ADD COLUMN memory_observer_interval INTEGER NOT NULL DEFAULT 30"),
        ("voice_output_mode", "ALTER TABLE app_settings ADD COLUMN voice_output_mode TEXT NOT NULL DEFAULT 'disabled'"),
        ("tts_api_protocol", "ALTER TABLE app_settings ADD COLUMN tts_api_protocol TEXT NOT NULL DEFAULT 'dashscope'"),
        ("tts_api_base_url", "ALTER TABLE app_settings ADD COLUMN tts_api_base_url TEXT NOT NULL DEFAULT 'https://dashscope.aliyuncs.com/api/v1'"),
        ("tts_api_model", "ALTER TABLE app_settings ADD COLUMN tts_api_model TEXT NOT NULL DEFAULT 'qwen3-tts-flash'"),
        ("tts_api_key", "ALTER TABLE app_settings ADD COLUMN tts_api_key TEXT NOT NULL DEFAULT ''"),
        ("tts_api_voice", "ALTER TABLE app_settings ADD COLUMN tts_api_voice TEXT NOT NULL DEFAULT 'Cherry'"),
        ("tts_api_language", "ALTER TABLE app_settings ADD COLUMN tts_api_language TEXT NOT NULL DEFAULT 'Chinese'"),
        ("vits_model_name", "ALTER TABLE app_settings ADD COLUMN vits_model_name TEXT NOT NULL DEFAULT ''"),
        ("vits_model_path", "ALTER TABLE app_settings ADD COLUMN vits_model_path TEXT NOT NULL DEFAULT ''"),
        ("vits_speaker_id", "ALTER TABLE app_settings ADD COLUMN vits_speaker_id TEXT"),
        ("vits_target_language", "ALTER TABLE app_settings ADD COLUMN vits_target_language TEXT NOT NULL DEFAULT 'ja'"),
        ("vits_speed", "ALTER TABLE app_settings ADD COLUMN vits_speed REAL NOT NULL DEFAULT 1.0"),
        ("vits_emotion_params", "ALTER TABLE app_settings ADD COLUMN vits_emotion_params TEXT NOT NULL DEFAULT ''"),
        ("vits_translate_enabled", "ALTER TABLE app_settings ADD COLUMN vits_translate_enabled INTEGER NOT NULL DEFAULT 1"),
        ("tool_hook_enabled", "ALTER TABLE app_settings ADD COLUMN tool_hook_enabled INTEGER NOT NULL DEFAULT 0"),
        ("tool_hook_mode", "ALTER TABLE app_settings ADD COLUMN tool_hook_mode TEXT NOT NULL DEFAULT 'fixed'"),
        ("tool_hook_port", "ALTER TABLE app_settings ADD COLUMN tool_hook_port INTEGER NOT NULL DEFAULT 34125"),
        ("tool_hook_token", "ALTER TABLE app_settings ADD COLUMN tool_hook_token TEXT NOT NULL DEFAULT ''"),
        ("tool_hook_token_enabled", "ALTER TABLE app_settings ADD COLUMN tool_hook_token_enabled INTEGER NOT NULL DEFAULT 1"),
        ("tool_hook_fixed_text", "ALTER TABLE app_settings ADD COLUMN tool_hook_fixed_text TEXT NOT NULL DEFAULT ''"),
        ("tool_hook_fixed_voice_text", "ALTER TABLE app_settings ADD COLUMN tool_hook_fixed_voice_text TEXT NOT NULL DEFAULT ''"),
        ("tool_hook_tool_texts", "ALTER TABLE app_settings ADD COLUMN tool_hook_tool_texts TEXT NOT NULL DEFAULT ''"),
        ("tool_hook_include_last_message", "ALTER TABLE app_settings ADD COLUMN tool_hook_include_last_message INTEGER NOT NULL DEFAULT 1"),
        ("tool_hook_min_interval_minutes", "ALTER TABLE app_settings ADD COLUMN tool_hook_min_interval_minutes INTEGER NOT NULL DEFAULT 10"),
        ("tool_hook_daily_limit", "ALTER TABLE app_settings ADD COLUMN tool_hook_daily_limit INTEGER NOT NULL DEFAULT 20"),
        ("tool_hook_debounce_seconds", "ALTER TABLE app_settings ADD COLUMN tool_hook_debounce_seconds INTEGER NOT NULL DEFAULT 0"),
        ("tool_hook_voice_enabled", "ALTER TABLE app_settings ADD COLUMN tool_hook_voice_enabled INTEGER NOT NULL DEFAULT 1"),
        ("system_status_enabled", "ALTER TABLE app_settings ADD COLUMN system_status_enabled INTEGER NOT NULL DEFAULT 0"),
        ("taskbar_apps_enabled", "ALTER TABLE app_settings ADD COLUMN taskbar_apps_enabled INTEGER NOT NULL DEFAULT 0"),
        ("now_playing_enabled", "ALTER TABLE app_settings ADD COLUMN now_playing_enabled INTEGER NOT NULL DEFAULT 0"),
        ("voice_input_mode", "ALTER TABLE app_settings ADD COLUMN voice_input_mode TEXT NOT NULL DEFAULT 'disabled'"),
        ("push_to_talk_shortcut", "ALTER TABLE app_settings ADD COLUMN push_to_talk_shortcut TEXT NOT NULL DEFAULT ''"),
        ("pet_show_on_fullscreen", "ALTER TABLE app_settings ADD COLUMN pet_show_on_fullscreen INTEGER NOT NULL DEFAULT 1"),
        ("pet_outfit", "ALTER TABLE app_settings ADD COLUMN pet_outfit TEXT NOT NULL DEFAULT 'default'"),
        ("search_provider", "ALTER TABLE app_settings ADD COLUMN search_provider TEXT NOT NULL DEFAULT ''"),
        ("search_api_key", "ALTER TABLE app_settings ADD COLUMN search_api_key TEXT NOT NULL DEFAULT ''"),
        ("search_base_url", "ALTER TABLE app_settings ADD COLUMN search_base_url TEXT NOT NULL DEFAULT ''"),
        ("agent_max_tool_rounds", "ALTER TABLE app_settings ADD COLUMN agent_max_tool_rounds INTEGER NOT NULL DEFAULT 30"),
        ("autostart_enabled", "ALTER TABLE app_settings ADD COLUMN autostart_enabled INTEGER NOT NULL DEFAULT 0"),
        ("study_enabled", "ALTER TABLE app_settings ADD COLUMN study_enabled INTEGER NOT NULL DEFAULT 0"),
        ("study_isolated", "ALTER TABLE app_settings ADD COLUMN study_isolated INTEGER NOT NULL DEFAULT 1"),
        ("study_review_enabled", "ALTER TABLE app_settings ADD COLUMN study_review_enabled INTEGER NOT NULL DEFAULT 1"),
        ("study_review_min_minutes", "ALTER TABLE app_settings ADD COLUMN study_review_min_minutes INTEGER NOT NULL DEFAULT 30"),
        ("study_review_max_minutes", "ALTER TABLE app_settings ADD COLUMN study_review_max_minutes INTEGER NOT NULL DEFAULT 90"),
        ("study_review_daily_limit", "ALTER TABLE app_settings ADD COLUMN study_review_daily_limit INTEGER NOT NULL DEFAULT 8"),
        ("study_quiz_types", "ALTER TABLE app_settings ADD COLUMN study_quiz_types TEXT NOT NULL DEFAULT 'meaning,spelling,reading'"),
        ("study_continuous_enabled", "ALTER TABLE app_settings ADD COLUMN study_continuous_enabled INTEGER NOT NULL DEFAULT 0"),
        ("study_quiz_ttl_seconds", "ALTER TABLE app_settings ADD COLUMN study_quiz_ttl_seconds INTEGER NOT NULL DEFAULT 60"),
    ] {
        if !columns.iter().any(|column| column == name) {
            conn.execute(sql, [])?;
        }
    }
    let todo_columns: Vec<String> = conn
        .prepare("PRAGMA table_info(todos)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    if !todo_columns.iter().any(|name| name == "repeat_interval_minutes") {
        conn.execute(
            "ALTER TABLE todos ADD COLUMN repeat_interval_minutes INTEGER",
            [],
        )?;
    }
    // mcp_servers 的增量列迁移（老库补 headers / timeout_seconds 列）。
    let mcp_columns: Vec<String> = conn
        .prepare("PRAGMA table_info(mcp_servers)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for (name, sql) in [
        (
            "headers",
            "ALTER TABLE mcp_servers ADD COLUMN headers TEXT NOT NULL DEFAULT ''",
        ),
        (
            "timeout_seconds",
            "ALTER TABLE mcp_servers ADD COLUMN timeout_seconds INTEGER NOT NULL DEFAULT 120",
        ),
    ] {
        if !mcp_columns.iter().any(|column| column == name) {
            conn.execute(sql, [])?;
        }
    }
    let d = AppSettings::default();
    conn.execute(
        "INSERT OR IGNORE INTO app_settings(id, pet_name, persona, user_name, api_base_url, api_model, voice_output_enabled) VALUES(1, ?, ?, ?, ?, ?, ?)",
        params![d.pet_name, d.persona, d.user_name, d.api_base_url, d.api_model, d.voice_output_enabled as i32],
    )?;
    seed_tool_hook_tool_texts(&conn)?;
    Ok(conn)
}

/// 一次性迁移：旧的全局固定文本拆分为各工具的独立配置。
/// 仅当用户自定义过旧文本（非空且不等于旧默认模板）或填写过语音文本时才复制，
/// 否则让各工具直接使用注册表中新的默认语句。
fn seed_tool_hook_tool_texts(conn: &Connection) -> rusqlite::Result<()> {
    let current: String = conn
        .query_row("SELECT tool_hook_tool_texts FROM app_settings WHERE id=1", [], |r| {
            r.get(0)
        })
        .unwrap_or_default();
    if !current.trim().is_empty() {
        return Ok(());
    }
    let legacy_text: String = conn
        .query_row("SELECT tool_hook_fixed_text FROM app_settings WHERE id=1", [], |r| r.get(0))
        .unwrap_or_default();
    let legacy_voice: String = conn
        .query_row(
            "SELECT tool_hook_fixed_voice_text FROM app_settings WHERE id=1",
            [],
            |r| r.get(0),
        )
        .unwrap_or_default();
    const LEGACY_DEFAULT: &str = "你在 {tool} 里 {project} 的任务已经完成了。";
    let customized = !legacy_text.trim().is_empty() && legacy_text.trim() != LEGACY_DEFAULT;
    if !customized && legacy_voice.trim().is_empty() {
        return Ok(());
    }
    let mut map = HashMap::new();
    for tool in crate::tool_hook_config::TOOLS {
        map.insert(
            tool.id.to_string(),
            ToolHookToolText {
                fixed_text: if customized {
                    legacy_text.trim().to_string()
                } else {
                    String::new()
                },
                fixed_voice_text: legacy_voice.trim().to_string(),
            },
        );
    }
    let json = serde_json::to_string(&map).unwrap_or_default();
    conn.execute(
        "UPDATE app_settings SET tool_hook_tool_texts=? WHERE id=1",
        params![json],
    )?;
    Ok(())
}

pub fn get_settings(conn: &Connection, key_configured: bool) -> rusqlite::Result<AppSettings> {
    conn.query_row(
        "SELECT pet_name, persona, user_name, api_base_url, api_model, voice_output_enabled, microphone_device_name, asr_app_id, asr_api_key, asr_api_secret, proactive_enabled, proactive_min_minutes, proactive_max_minutes, proactive_daily_limit, qdrant_url, embedding_base_url, embedding_model, embedding_api_key, embedding_dimension, memory_observer_enabled, memory_observer_interval, voice_output_mode, tts_api_protocol, tts_api_base_url, tts_api_model, tts_api_key, tts_api_voice, tts_api_language, vits_model_name, vits_model_path, vits_speaker_id, vits_target_language, vits_speed, vits_emotion_params, vits_translate_enabled, tool_hook_enabled, tool_hook_mode, tool_hook_port, tool_hook_tool_texts, tool_hook_include_last_message, tool_hook_min_interval_minutes, tool_hook_daily_limit, tool_hook_debounce_seconds, tool_hook_voice_enabled, system_status_enabled, taskbar_apps_enabled, now_playing_enabled, voice_input_mode, push_to_talk_shortcut, pet_show_on_fullscreen, pet_outfit, search_provider, search_api_key, search_base_url, agent_max_tool_rounds, autostart_enabled, study_enabled, study_isolated, study_review_enabled, study_review_min_minutes, study_review_max_minutes, study_review_daily_limit, study_quiz_types, study_continuous_enabled, study_quiz_ttl_seconds FROM app_settings WHERE id=1",
        [],
        |r| {
            let asr_app_id: String = r.get(7)?;
            let asr_api_key: String = r.get(8)?;
            let asr_api_secret: String = r.get(9)?;
            let embedding_api_key:String=r.get(17)?; let embedding_base_url:String=r.get(15)?; let embedding_model:String=r.get(16)?; let embedding_dimension:u32=r.get(18)?;
            let tts_api_key:String=r.get(25)?;
            let tool_hook_tool_texts_raw:String=r.get(38)?;
            let tool_hook_tool_texts=serde_json::from_str(&tool_hook_tool_texts_raw).unwrap_or_default();
            Ok(AppSettings { pet_name: r.get(0)?, persona: r.get(1)?, user_name: r.get(2)?, api_base_url: r.get(3)?, api_model: r.get(4)?, api_key_configured: key_configured, voice_output_enabled: r.get::<_, i32>(5)? != 0, microphone_device_name: r.get(6)?, asr_configured: !asr_app_id.is_empty() && !asr_api_key.is_empty() && !asr_api_secret.is_empty(), asr_app_id, asr_api_key, asr_api_secret, proactive_enabled: r.get::<_, i32>(10)? != 0, proactive_min_minutes: r.get(11)?, proactive_max_minutes: r.get(12)?, proactive_daily_limit: r.get(13)?, qdrant_url:r.get(14)?, memory_configured:!embedding_base_url.is_empty()&&!embedding_model.is_empty()&&!embedding_api_key.is_empty()&&embedding_dimension>0, embedding_base_url,embedding_model,embedding_api_key,embedding_dimension, memory_observer_enabled:r.get::<_,i32>(19)? != 0, memory_observer_interval:r.get(20)?, voice_output_mode:r.get(21)?, tts_api_protocol:r.get(22)?, tts_api_base_url:r.get(23)?, tts_api_model:r.get(24)?, tts_api_configured:!tts_api_key.is_empty(), tts_api_key, tts_api_voice:r.get(26)?, tts_api_language:r.get(27)?, vits_model_name:r.get(28)?, vits_model_path:r.get(29)?, vits_speaker_id:r.get(30)?, vits_target_language:r.get(31)?, vits_speed:r.get(32)?, vits_emotion_params:r.get(33)?, vits_translate_enabled:r.get::<_,i32>(34)? != 0, tool_hook_enabled:r.get::<_,i32>(35)? != 0, tool_hook_mode:r.get(36)?, tool_hook_port:r.get(37)?, tool_hook_tool_texts, tool_hook_include_last_message:r.get::<_,i32>(39)? != 0, tool_hook_min_interval_minutes:r.get(40)?, tool_hook_daily_limit:r.get(41)?, tool_hook_debounce_seconds:r.get(42)?, tool_hook_voice_enabled:r.get::<_,i32>(43)? != 0, system_status_enabled:r.get::<_,i32>(44)? != 0, taskbar_apps_enabled:r.get::<_,i32>(45)? != 0, now_playing_enabled:r.get::<_,i32>(46)? != 0, voice_input_mode:r.get(47)?, push_to_talk_shortcut:r.get(48)?, pet_show_on_fullscreen:r.get::<_,i32>(49)? != 0, pet_outfit:r.get(50)?, search_provider:r.get(51)?, search_api_key:r.get(52)?, search_base_url:r.get(53)?, search_api_configured:!r.get::<_,String>(52)?.is_empty(), agent_max_tool_rounds:r.get(54)?, autostart_enabled:r.get::<_,i32>(55)? != 0, study_enabled:r.get::<_,i32>(56)? != 0, study_isolated:r.get::<_,i32>(57)? != 0, study_review_enabled:r.get::<_,i32>(58)? != 0, study_review_min_minutes:r.get(59)?, study_review_max_minutes:r.get(60)?, study_review_daily_limit:r.get(61)?, study_quiz_types:r.get(62)?, study_continuous_enabled:r.get::<_,i32>(63)? != 0, study_quiz_ttl_seconds:r.get(64)? })
        }
    )
}

pub fn toggle_proactive_enabled(conn: &Connection) -> rusqlite::Result<bool> {
    let enabled = conn.query_row(
        "SELECT proactive_enabled FROM app_settings WHERE id=1",
        [],
        |row| Ok(row.get::<_, i32>(0)? == 0),
    )?;
    conn.execute(
        "UPDATE app_settings SET proactive_enabled=? WHERE id=1",
        params![enabled as i32],
    )?;
    let settings = get_settings(conn, true)?;
    reset_proactive_schedule(conn, &settings, chrono::Utc::now().timestamp_millis())?;
    Ok(enabled)
}

/// 右键菜单切换日语学习模式：翻转 study_enabled 并重排复习计划（开启时立即排第一次，关闭时清空）。
pub fn toggle_study_enabled(conn: &Connection) -> rusqlite::Result<bool> {
    let enabled = conn.query_row(
        "SELECT study_enabled FROM app_settings WHERE id=1",
        [],
        |row| Ok(row.get::<_, i32>(0)? == 0),
    )?;
    conn.execute(
        "UPDATE app_settings SET study_enabled=? WHERE id=1",
        params![enabled as i32],
    )?;
    let settings = get_settings(conn, true)?;
    reset_study_schedule(conn, &settings, chrono::Utc::now().timestamp_millis())?;
    Ok(enabled)
}

/// 单独更新桌宠当前服装，供右键菜单即时切换使用（避免整份设置回写互相覆盖）。
pub fn set_pet_outfit(conn: &Connection, outfit: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE app_settings SET pet_outfit=? WHERE id=1",
        params![outfit],
    )?;
    Ok(())
}

pub fn save_settings(conn: &Connection, s: &AppSettings) -> rusqlite::Result<()> {
    let tool_hook_tool_texts = serde_json::to_string(&s.tool_hook_tool_texts).unwrap_or_default();
    conn.execute("UPDATE app_settings SET pet_name=?, persona=?, user_name=?, api_base_url=?, api_model=?, voice_output_enabled=?, microphone_device_name=?, asr_app_id=?, asr_api_key=?, asr_api_secret=?, proactive_enabled=?, proactive_min_minutes=?, proactive_max_minutes=?, proactive_daily_limit=?, qdrant_url=?, embedding_base_url=?, embedding_model=?, embedding_api_key=?, embedding_dimension=?, memory_observer_enabled=?, memory_observer_interval=?, voice_output_mode=?, tts_api_protocol=?, tts_api_base_url=?, tts_api_model=?, tts_api_key=?, tts_api_voice=?, tts_api_language=?, vits_model_name=?, vits_model_path=?, vits_speaker_id=?, vits_target_language=?, vits_speed=?, vits_emotion_params=?, vits_translate_enabled=?, tool_hook_enabled=?, tool_hook_mode=?, tool_hook_port=?, tool_hook_tool_texts=?, tool_hook_include_last_message=?, tool_hook_min_interval_minutes=?, tool_hook_daily_limit=?, tool_hook_debounce_seconds=?, tool_hook_voice_enabled=?, system_status_enabled=?, taskbar_apps_enabled=?, now_playing_enabled=?, voice_input_mode=?, push_to_talk_shortcut=?, pet_show_on_fullscreen=?, pet_outfit=?, search_provider=?, search_api_key=?, search_base_url=?, agent_max_tool_rounds=?, autostart_enabled=?, study_enabled=?, study_isolated=?, study_review_enabled=?, study_review_min_minutes=?, study_review_max_minutes=?, study_review_daily_limit=?, study_quiz_types=?, study_continuous_enabled=?, study_quiz_ttl_seconds=? WHERE id=1",
        params![s.pet_name, s.persona, s.user_name, s.api_base_url, s.api_model, s.voice_output_enabled as i32, s.microphone_device_name, s.asr_app_id, s.asr_api_key, s.asr_api_secret, s.proactive_enabled as i32, s.proactive_min_minutes, s.proactive_max_minutes, s.proactive_daily_limit,s.qdrant_url,s.embedding_base_url,s.embedding_model,s.embedding_api_key,s.embedding_dimension,s.memory_observer_enabled as i32,s.memory_observer_interval,s.voice_output_mode,s.tts_api_protocol,s.tts_api_base_url,s.tts_api_model,s.tts_api_key,s.tts_api_voice,s.tts_api_language,s.vits_model_name,s.vits_model_path,s.vits_speaker_id,s.vits_target_language,s.vits_speed,s.vits_emotion_params,s.vits_translate_enabled as i32, s.tool_hook_enabled as i32, s.tool_hook_mode, s.tool_hook_port, tool_hook_tool_texts, s.tool_hook_include_last_message as i32, s.tool_hook_min_interval_minutes, s.tool_hook_daily_limit, s.tool_hook_debounce_seconds, s.tool_hook_voice_enabled as i32, s.system_status_enabled as i32, s.taskbar_apps_enabled as i32, s.now_playing_enabled as i32, s.voice_input_mode, s.push_to_talk_shortcut, s.pet_show_on_fullscreen as i32, s.pet_outfit, s.search_provider, s.search_api_key, s.search_base_url, s.agent_max_tool_rounds, s.autostart_enabled as i32, s.study_enabled as i32, s.study_isolated as i32, s.study_review_enabled as i32, s.study_review_min_minutes, s.study_review_max_minutes, s.study_review_daily_limit, s.study_quiz_types, s.study_continuous_enabled as i32, s.study_quiz_ttl_seconds])?;
    Ok(())
}

const MESSAGE_COLUMNS: &str =
    "id, role, content, japanese_text, emotion, trigger_type, created_at, session, quiz_json";

/// 统一的消息行映射：quiz_json 解析失败时静默丢弃（不影响消息本体）。
fn row_to_message(r: &rusqlite::Row) -> rusqlite::Result<Message> {
    let quiz_json: Option<String> = r.get(8)?;
    Ok(Message {
        id: r.get(0)?,
        role: r.get(1)?,
        content: r.get(2)?,
        japanese_text: r.get(3)?,
        emotion: r.get(4)?,
        trigger_type: r.get(5)?,
        created_at: r.get(6)?,
        session: r.get(7)?,
        quiz: quiz_json.and_then(|json| serde_json::from_str(&json).ok()),
    })
}

pub fn insert_message(conn: &Connection, m: &Message) -> rusqlite::Result<()> {
    let quiz_json = m
        .quiz
        .as_ref()
        .map(|quiz| serde_json::to_string(quiz).unwrap_or_default());
    let session = if m.session.is_empty() {
        "main"
    } else {
        m.session.as_str()
    };
    conn.execute(
        "INSERT INTO messages(id, role, content, japanese_text, emotion, trigger_type, created_at, session, quiz_json) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![m.id, m.role, m.content, m.japanese_text, m.emotion, m.trigger_type, m.created_at, session, quiz_json],
    )?;
    Ok(())
}

pub fn list_messages(conn: &Connection, limit: u32) -> rusqlite::Result<Vec<Message>> {
    let mut stmt = conn.prepare(&format!("SELECT {MESSAGE_COLUMNS} FROM (SELECT * FROM messages ORDER BY created_at DESC LIMIT ?) ORDER BY created_at"))?;
    let messages = stmt
        .query_map([limit.min(500)], row_to_message)?
        .collect();
    messages
}

/// 按会话读取最近消息（正序返回），供主循环按隔离设置分别加载主会话/学习会话历史。
pub fn list_messages_by_session(
    conn: &Connection,
    session: &str,
    limit: u32,
) -> rusqlite::Result<Vec<Message>> {
    let mut stmt = conn.prepare(&format!("SELECT {MESSAGE_COLUMNS} FROM (SELECT * FROM messages WHERE session=? ORDER BY created_at DESC LIMIT ?) ORDER BY created_at"))?;
    let messages = stmt
        .query_map(params![session, limit.min(500)], row_to_message)?
        .collect();
    messages
}

/// 学习历史页：学习会话消息 + 落在主会话里的复习出题/答题反馈（非隔离模式），按时间正序返回。
pub fn list_study_messages(conn: &Connection, limit: u32) -> rusqlite::Result<Vec<Message>> {
    let mut stmt = conn.prepare(&format!("SELECT {MESSAGE_COLUMNS} FROM (SELECT * FROM messages WHERE session='study' OR trigger_type IN ('study_review','study_feedback') ORDER BY created_at DESC LIMIT ?) ORDER BY created_at"))?;
    let messages = stmt
        .query_map([limit.min(500)], row_to_message)?
        .collect();
    messages
}

/// 主会话历史分页查询：包含用户输入、主会话回复（user_text/user_voice）
/// 以及 AI 主动发起的消息（提醒/主动搭话/工具提醒/任务转告），
/// 按 created_at 游标向前翻页（before 缺省表示从最新开始），返回按时间正序。
pub fn list_main_session_messages(
    conn: &Connection,
    before: Option<i64>,
    limit: u32,
) -> rusqlite::Result<Vec<Message>> {
    let before = before.unwrap_or(i64::MAX);
    let mut stmt = conn.prepare(
        // 注意：SQLite 的 SELECT * 不含 rowid 伪列，子查询需显式取出供外层排序。
        // 只取主会话消息：学习会话（session='study'）不混入历史页主会话标签。
        &format!("SELECT {MESSAGE_COLUMNS} FROM (\
         SELECT rowid AS _rid, * FROM messages \
         WHERE session='main' AND trigger_type IN ('user_text','user_voice','reminder_due','companion_tick','tool_hook','agent_followup','study_review','study_feedback') \
         AND created_at < ? \
         ORDER BY created_at DESC, rowid DESC LIMIT ?) ORDER BY created_at, _rid"),
    )?;
    let messages = stmt
        .query_map(params![before, limit.min(500)], row_to_message)?
        .collect();
    messages
}

/// 读取观察者维护的"用户偏好"滚动总结；缺行/空值时返回空字符串。
pub fn get_user_preference_summary(conn: &Connection) -> rusqlite::Result<String> {
    conn.query_row(
        "SELECT text FROM user_preference_summary WHERE id=1",
        [],
        |r| r.get(0),
    )
}

pub fn set_user_preference_summary(conn: &Connection, text: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE user_preference_summary SET text=?, updated_at=? WHERE id=1",
        params![text, chrono::Utc::now().timestamp_millis()],
    )?;
    Ok(())
}

pub fn observer_message_batch(
    conn: &Connection,
    limit: u32,
) -> rusqlite::Result<Vec<(i64, Message)>> {
    let last: i64 = conn.query_row(
        "SELECT last_message_rowid FROM memory_observer_state WHERE id=1",
        [],
        |r| r.get(0),
    )?;
    // 学习会话不进长期记忆观察者：只消费主会话的用户消息。
    let mut stmt = conn.prepare("SELECT rowid,id,role,content,trigger_type,created_at FROM messages WHERE rowid>? AND session='main' AND trigger_type IN ('user_text','user_voice') ORDER BY rowid LIMIT ?")?;
    let rows = stmt
        .query_map(params![last, limit], |r| {
            Ok((
                r.get(0)?,
                Message {
                    id: r.get(1)?,
                    role: r.get(2)?,
                    content: r.get(3)?,
                    japanese_text: None,
                    emotion: None,
                    trigger_type: r.get(4)?,
                    created_at: r.get(5)?,
                    session: "main".into(),
                    quiz: None,
                },
            ))
        })?
        .collect();
    rows
}

pub fn advance_memory_observer(conn: &Connection, rowid: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE memory_observer_state SET last_message_rowid=?,updated_at=? WHERE id=1",
        params![rowid, chrono::Utc::now().timestamp_millis()],
    )?;
    Ok(())
}

pub fn create_todo(
    conn: &mut Connection,
    title: &str,
    due_at_utc: i64,
    timezone: &str,
    source_message_id: Option<&str>,
    repeat_interval_minutes: Option<i64>,
) -> rusqlite::Result<Todo> {
    let now = chrono::Utc::now().timestamp_millis();
    let todo = Todo {
        id: uuid::Uuid::new_v4().to_string(),
        title: title.to_string(),
        due_at_utc,
        timezone: timezone.to_string(),
        status: "pending".into(),
        repeat_interval_minutes,
        created_at: now,
    };
    let occurrence_id = uuid::Uuid::new_v4().to_string();
    let tx = conn.transaction()?;
    tx.execute("INSERT INTO todos(id,title,due_at_utc,timezone,status,source_message_id,created_at,updated_at,repeat_interval_minutes) VALUES(?,?,?,?,?,?,?,?,?)", params![todo.id, todo.title, todo.due_at_utc, todo.timezone, todo.status, source_message_id, now, now, todo.repeat_interval_minutes])?;
    tx.execute("INSERT INTO reminder_occurrences(id,todo_id,scheduled_at_utc,status) VALUES(?,?,?,'pending')", params![occurrence_id, todo.id, due_at_utc])?;
    tx.commit()?;
    Ok(todo)
}

/// 删除待办：标记为 deleted，并清掉未交付的提醒 occurrence。
pub fn delete_todo(conn: &Connection, id: &str) -> rusqlite::Result<bool> {
    let now = chrono::Utc::now().timestamp_millis();
    let changed = conn.execute(
        "UPDATE todos SET status='deleted',updated_at=? WHERE id=? AND status='pending'",
        params![now, id],
    )?;
    if changed == 0 {
        return Ok(false);
    }
    conn.execute(
        "DELETE FROM reminder_occurrences WHERE todo_id=? AND status IN ('pending','claimed')",
        params![id],
    )?;
    Ok(true)
}

pub fn list_pending_todos(conn: &Connection) -> rusqlite::Result<Vec<Todo>> {
    let mut stmt = conn.prepare("SELECT id,title,due_at_utc,timezone,status,repeat_interval_minutes,created_at FROM todos WHERE status='pending' ORDER BY due_at_utc LIMIT 100")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Todo {
                id: r.get(0)?,
                title: r.get(1)?,
                due_at_utc: r.get(2)?,
                timezone: r.get(3)?,
                status: r.get(4)?,
                repeat_interval_minutes: r.get(5)?,
                created_at: r.get(6)?,
            })
        })?
        .collect();
    rows
}

pub fn claim_due_reminder(
    conn: &mut Connection,
    now: i64,
) -> rusqlite::Result<Option<DueReminder>> {
    let tx = conn.transaction()?;
    let found = tx.query_row("SELECT o.id,t.id,t.title,o.scheduled_at_utc FROM reminder_occurrences o JOIN todos t ON t.id=o.todo_id WHERE t.status='pending' AND o.scheduled_at_utc<=? AND (o.status='pending' OR (o.status='claimed' AND o.lease_until<?)) ORDER BY o.scheduled_at_utc LIMIT 1", params![now, now], |r| Ok(DueReminder { occurrence_id:r.get(0)?, todo_id:r.get(1)?, title:r.get(2)?, scheduled_at_utc:r.get(3)? })).optional()?;
    if let Some(item) = &found {
        tx.execute("UPDATE reminder_occurrences SET status='claimed',attempt_count=attempt_count+1,lease_until=? WHERE id=?", params![now + 120_000, item.occurrence_id])?;
    }
    tx.commit()?;
    Ok(found)
}

pub fn deliver_reminder(
    conn: &Connection,
    item: &DueReminder,
    error: Option<&str>,
) -> rusqlite::Result<()> {
    let now = chrono::Utc::now().timestamp_millis();
    conn.execute("UPDATE reminder_occurrences SET status='delivered',delivered_at=?,lease_until=NULL,last_error=? WHERE id=?", params![now,error,item.occurrence_id])?;
    let repeat: Option<i64> = conn.query_row(
        "SELECT repeat_interval_minutes FROM todos WHERE id=?",
        params![item.todo_id],
        |r| r.get(0),
    )?;
    if let Some(interval_minutes) = repeat.filter(|v| *v > 0) {
        // 循环待办：从原计划时间起按间隔推进到下一个未来时间点，避免漂移。
        let interval_ms = interval_minutes * 60_000;
        let mut next_due = item.scheduled_at_utc + interval_ms;
        while next_due <= now {
            next_due += interval_ms;
        }
        conn.execute(
            "UPDATE todos SET due_at_utc=?,updated_at=? WHERE id=?",
            params![next_due, now, item.todo_id],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO reminder_occurrences(id,todo_id,scheduled_at_utc,status) VALUES(?,?,?,'pending')",
            params![uuid::Uuid::new_v4().to_string(), item.todo_id, next_due],
        )?;
    } else {
        conn.execute(
            "UPDATE todos SET status='completed',updated_at=? WHERE id=?",
            params![now, item.todo_id],
        )?;
    }
    Ok(())
}

fn local_day(now: i64) -> String {
    chrono::DateTime::from_timestamp_millis(now)
        .unwrap_or_default()
        .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
        .format("%Y-%m-%d")
        .to_string()
}

pub fn reset_proactive_schedule(
    conn: &Connection,
    settings: &AppSettings,
    now: i64,
) -> rusqlite::Result<()> {
    if !settings.proactive_enabled {
        conn.execute(
            "UPDATE scheduler_state SET next_proactive_at=NULL WHERE id=1",
            [],
        )?;
        return Ok(());
    }
    let min = settings.proactive_min_minutes.max(1) as i64;
    let max = settings
        .proactive_max_minutes
        .max(settings.proactive_min_minutes)
        .max(1) as i64;
    let minutes = rand::random_range(min..=max);
    conn.execute(
        "UPDATE scheduler_state SET next_proactive_at=? WHERE id=1",
        [now + minutes * 60_000],
    )?;
    Ok(())
}

pub fn claim_proactive_due(
    conn: &Connection,
    settings: &AppSettings,
    now: i64,
) -> rusqlite::Result<bool> {
    if !settings.proactive_enabled {
        return Ok(false);
    }
    let day = local_day(now);
    let (next, saved_day, count): (Option<i64>, Option<String>, u32) = conn.query_row(
        "SELECT next_proactive_at,proactive_day,proactive_count FROM scheduler_state WHERE id=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let count = if saved_day.as_deref() == Some(&day) {
        count
    } else {
        0
    };
    if saved_day.as_deref() != Some(&day) {
        conn.execute(
            "UPDATE scheduler_state SET proactive_day=?,proactive_count=0 WHERE id=1",
            [&day],
        )?;
    }
    if next.is_none() {
        reset_proactive_schedule(conn, settings, now)?;
        return Ok(false);
    }
    if next.unwrap() > now || count >= settings.proactive_daily_limit {
        return Ok(false);
    }
    conn.execute(
        "UPDATE scheduler_state SET proactive_count=proactive_count+1 WHERE id=1",
        [],
    )?;
    reset_proactive_schedule(conn, settings, now)?;
    Ok(true)
}

fn row_to_mcp_server(r: &rusqlite::Row) -> rusqlite::Result<McpServer> {
    Ok(McpServer {
        id: r.get(0)?,
        name: r.get(1)?,
        transport: r.get(2)?,
        command: r.get(3)?,
        args: r.get(4)?,
        env: r.get(5)?,
        url: r.get(6)?,
        headers: r.get(7)?,
        enabled: r.get::<_, i32>(8)? != 0,
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
        timeout_seconds: r.get(11)?,
    })
}

const MCP_SERVER_COLUMNS: &str =
    "id,name,transport,command,args,env,url,headers,enabled,created_at,updated_at,timeout_seconds";

pub fn list_mcp_servers(conn: &Connection) -> rusqlite::Result<Vec<McpServer>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MCP_SERVER_COLUMNS} FROM mcp_servers ORDER BY id"
    ))?;
    let rows = stmt.query_map([], row_to_mcp_server)?.collect();
    rows
}

pub fn get_mcp_server(conn: &Connection, id: i64) -> rusqlite::Result<Option<McpServer>> {
    conn.query_row(
        &format!("SELECT {MCP_SERVER_COLUMNS} FROM mcp_servers WHERE id=?"),
        [id],
        row_to_mcp_server,
    )
    .optional()
}

pub fn insert_mcp_server(conn: &Connection, input: &McpServerInput) -> rusqlite::Result<McpServer> {
    let now = chrono::Utc::now().timestamp_millis();
    conn.execute(
        "INSERT INTO mcp_servers(name,transport,command,args,env,url,headers,enabled,created_at,updated_at,timeout_seconds) VALUES(?,?,?,?,?,?,?,1,?,?,?)",
        params![input.name, input.transport, input.command, input.args, input.env, input.url, input.headers, now, now, input.timeout_seconds],
    )?;
    let id = conn.last_insert_rowid();
    get_mcp_server(conn, id)?.ok_or_else(|| {
        rusqlite::Error::QueryReturnedNoRows
    })
}

pub fn update_mcp_server(conn: &Connection, id: i64, input: &McpServerInput) -> rusqlite::Result<bool> {
    let now = chrono::Utc::now().timestamp_millis();
    let changed = conn.execute(
        "UPDATE mcp_servers SET name=?,transport=?,command=?,args=?,env=?,url=?,headers=?,updated_at=?,timeout_seconds=? WHERE id=?",
        params![input.name, input.transport, input.command, input.args, input.env, input.url, input.headers, now, input.timeout_seconds, id],
    )?;
    Ok(changed > 0)
}

pub fn delete_mcp_server(conn: &Connection, id: i64) -> rusqlite::Result<bool> {
    Ok(conn.execute("DELETE FROM mcp_servers WHERE id=?", [id])? > 0)
}

pub fn set_mcp_server_enabled(conn: &Connection, id: i64, enabled: bool) -> rusqlite::Result<bool> {
    let now = chrono::Utc::now().timestamp_millis();
    let changed = conn.execute(
        "UPDATE mcp_servers SET enabled=?,updated_at=? WHERE id=?",
        params![enabled as i32, now, id],
    )?;
    Ok(changed > 0)
}

pub fn insert_agent_run(conn: &Connection, task: &str) -> rusqlite::Result<AgentRun> {
    let now = chrono::Utc::now().timestamp_millis();
    conn.execute(
        "INSERT INTO agent_runs(task,status,summary,created_at) VALUES(?,'running','',?)",
        params![task, now],
    )?;
    Ok(AgentRun {
        id: conn.last_insert_rowid(),
        task: task.into(),
        status: "running".into(),
        summary: String::new(),
        created_at: now,
        finished_at: None,
    })
}

pub fn finish_agent_run(
    conn: &Connection,
    id: i64,
    status: &str,
    summary: &str,
) -> rusqlite::Result<()> {
    let now = chrono::Utc::now().timestamp_millis();
    conn.execute(
        "UPDATE agent_runs SET status=?,summary=?,finished_at=? WHERE id=?",
        params![status, summary, now, id],
    )?;
    Ok(())
}

pub fn list_recent_agent_runs(conn: &Connection, limit: u32) -> rusqlite::Result<Vec<AgentRun>> {
    let mut stmt = conn.prepare(
        "SELECT id,task,status,summary,created_at,finished_at FROM agent_runs ORDER BY id DESC LIMIT ?",
    )?;
    let rows = stmt
        .query_map([limit.min(50)], |r| {
            Ok(AgentRun {
                id: r.get(0)?,
                task: r.get(1)?,
                status: r.get(2)?,
                summary: r.get(3)?,
                created_at: r.get(4)?,
                finished_at: r.get(5)?,
            })
        })?
        .collect();
    rows
}

/// 记录一次 API 调用消耗（ASR 等无 token 的调用传 0）。
pub fn insert_usage_event(
    conn: &Connection,
    category: &str,
    prompt_tokens: i64,
    completion_tokens: i64,
) -> rusqlite::Result<()> {
    let now = chrono::Utc::now().timestamp_millis();
    conn.execute(
        "INSERT INTO usage_events(created_at,category,prompt_tokens,completion_tokens) VALUES(?,?,?,?)",
        params![now, category, prompt_tokens, completion_tokens],
    )?;
    Ok(())
}

/// 按分类汇总消耗：today_start_ms 为当地当日 0 点的 UTC 毫秒时间戳。
pub fn summarize_usage(conn: &Connection, today_start_ms: i64) -> rusqlite::Result<Vec<UsageSummary>> {
    let mut stmt = conn.prepare(
        "SELECT category,
                COUNT(*),
                COALESCE(SUM(prompt_tokens),0),
                COALESCE(SUM(completion_tokens),0),
                COALESCE(SUM(CASE WHEN created_at >= ? THEN 1 ELSE 0 END),0),
                COALESCE(SUM(CASE WHEN created_at >= ? THEN prompt_tokens ELSE 0 END),0),
                COALESCE(SUM(CASE WHEN created_at >= ? THEN completion_tokens ELSE 0 END),0)
         FROM usage_events GROUP BY category",
    )?;
    let rows = stmt
        .query_map(
            params![today_start_ms, today_start_ms, today_start_ms],
            |r| {
                Ok(UsageSummary {
                    category: r.get(0)?,
                    calls_total: r.get(1)?,
                    prompt_total: r.get(2)?,
                    completion_total: r.get(3)?,
                    calls_today: r.get(4)?,
                    prompt_today: r.get(5)?,
                    completion_today: r.get(6)?,
                })
            },
        )?
        .collect();
    rows
}

// ---------- 日语学习模式：单词组 / 单词 / 选题记录 / 复习调度 ----------

/// 重导入一个单词组（id 即组名）：按（单词+假名）自然键合并——已有词保留学习统计并更新中文意思，
/// 新词插入，文件里已删除的词移除（其历史答题记录 quiz_records 自带单词快照，不受影响）。
/// 组不存在则新建（默认未启用）。绝不能整表删词重插，否则统计与题目关联会随 id 变化全部丢失。
pub fn upsert_word_group(
    conn: &mut Connection,
    name: &str,
    file_name: &str,
    words: &[(String, String, String)],
) -> rusqlite::Result<()> {
    let now = chrono::Utc::now().timestamp_millis();
    let tx = conn.transaction()?;
    let existed: bool = tx
        .query_row(
            "SELECT COUNT(*) FROM word_groups WHERE id=?",
            [name],
            |r| r.get::<_, i64>(0),
        )
        .map(|count| count > 0)?;
    if !existed {
        tx.execute(
            "INSERT INTO word_groups(id, name, file_name, enabled, word_count, imported_at, file_missing) VALUES(?,?,?,0,0,?,0)",
            params![name, name, file_name, now],
        )?;
    }
    // 文件内自然键去重（保留首次出现）。
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let file_words: Vec<&(String, String, String)> = words
        .iter()
        .filter(|(word, kana, _)| seen.insert((word.clone(), kana.clone())))
        .collect();
    for (word, kana, meaning) in &file_words {
        // 已存在（同单词+假名）→ 只更新中文意思，保留统计；不存在 → 插入新词。
        tx.execute(
            "INSERT INTO words(group_id, word, kana, meaning) VALUES(?,?,?,?) \
             ON CONFLICT(group_id, word, kana) DO UPDATE SET meaning=excluded.meaning",
            params![name, word, kana, meaning],
        )?;
    }
    // 删除文件中已不存在的词。
    let existing: Vec<(i64, String, String)> = tx
        .prepare("SELECT id, word, kana FROM words WHERE group_id=?")?
        .query_map([name], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (id, word, kana) in existing {
        if !seen.contains(&(word, kana)) {
            tx.execute("DELETE FROM words WHERE id=?", [id])?;
        }
    }
    // word_count 以实际入库（去重后）的数量为准。
    tx.execute(
        "UPDATE word_groups SET file_name=?, word_count=(SELECT COUNT(*) FROM words WHERE group_id=?), imported_at=?, file_missing=0 WHERE id=?",
        params![file_name, name, now, name],
    )?;
    tx.commit()?;
    Ok(())
}

/// 标记组文件是否已从 wordgroups 目录消失（保留 DB 记录与统计数据）。
pub fn mark_word_group_missing(
    conn: &Connection,
    group_id: &str,
    missing: bool,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE word_groups SET file_missing=? WHERE id=?",
        params![missing as i32, group_id],
    )?;
    Ok(())
}

fn row_to_word_item(r: &rusqlite::Row) -> rusqlite::Result<WordItem> {
    let mut item = WordItem {
        id: r.get(0)?,
        group_id: r.get(1)?,
        word: r.get(2)?,
        kana: r.get(3)?,
        meaning: r.get(4)?,
        correct_count: r.get(5)?,
        wrong_count: r.get(6)?,
        streak: r.get(7)?,
        last_outcome: r.get(8)?,
        last_reviewed_at: r.get(9)?,
        mastery: String::new(),
    };
    item.mastery = crate::study::mastery_key(&item).to_string();
    Ok(item)
}

const WORD_COLUMNS: &str = "id, group_id, word, kana, meaning, correct_count, wrong_count, streak, last_outcome, last_reviewed_at";

/// 单词组列表（含各掌握度计数与文件缺失标记），供设置页展示。
pub fn list_word_groups(conn: &Connection) -> rusqlite::Result<Vec<WordGroup>> {
    let mut stmt = conn.prepare(
        "SELECT g.id, g.name, g.file_name, g.enabled, g.word_count, g.file_missing,
                COALESCE(SUM(CASE WHEN w.streak>=2 THEN 1 ELSE 0 END),0),
                COALESCE(SUM(CASE WHEN w.streak<2 AND w.last_outcome='wrong' THEN 1 ELSE 0 END),0),
                COALESCE(SUM(CASE WHEN w.streak<2 AND (w.last_outcome IS NULL OR w.last_outcome!='wrong') AND (w.correct_count+w.wrong_count)>0 THEN 1 ELSE 0 END),0),
                COALESCE(SUM(CASE WHEN w.streak<2 AND (w.last_outcome IS NULL OR w.last_outcome!='wrong') AND (w.correct_count+w.wrong_count)=0 THEN 1 ELSE 0 END),0)
         FROM word_groups g LEFT JOIN words w ON w.group_id=g.id
         GROUP BY g.id ORDER BY g.name",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(WordGroup {
                id: r.get(0)?,
                name: r.get(1)?,
                file_name: r.get(2)?,
                enabled: r.get::<_, i32>(3)? != 0,
                word_count: r.get(4)?,
                file_missing: r.get::<_, i32>(5)? != 0,
                mastered: r.get(6)?,
                forgotten: r.get(7)?,
                shaky: r.get(8)?,
                unlearned: r.get(9)?,
            })
        })?
        .collect();
    rows
}

/// 组内全部单词（按导入顺序），掌握度标签查询时派生。
pub fn list_words(conn: &Connection, group_id: &str) -> rusqlite::Result<Vec<WordItem>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {WORD_COLUMNS} FROM words WHERE group_id=? ORDER BY id"
    ))?;
    let rows = stmt.query_map([group_id], row_to_word_item)?.collect();
    rows
}

/// 单选启用单词组：事务内先全部取消再启用目标组；组不存在时报错。
pub fn set_enabled_group(conn: &mut Connection, group_id: Option<&str>) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute("UPDATE word_groups SET enabled=0", [])?;
    if let Some(id) = group_id {
        let changed = tx.execute("UPDATE word_groups SET enabled=1 WHERE id=?", [id])?;
        if changed == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
    }
    tx.commit()?;
    Ok(())
}

/// 当前启用组的全部单词（未启用任何组时为空）。
pub fn enabled_group_words(conn: &Connection) -> rusqlite::Result<Vec<WordItem>> {
    // JOIN 后两表都有 id 列，必须显式加表前缀避免歧义。
    let mut stmt = conn.prepare(
        "SELECT w.id, w.group_id, w.word, w.kana, w.meaning, w.correct_count, w.wrong_count, w.streak, w.last_outcome, w.last_reviewed_at
         FROM words w JOIN word_groups g ON g.id=w.group_id WHERE g.enabled=1 ORDER BY w.id"
    )?;
    let rows = stmt.query_map([], row_to_word_item)?.collect();
    rows
}

/// 作答后更新单词统计：streak 连续答对累计、答错清零；掌握度由这些字段派生。
pub fn record_quiz_result(conn: &Connection, word_id: i64, correct: bool) -> rusqlite::Result<()> {
    let now = chrono::Utc::now().timestamp_millis();
    conn.execute(
        "UPDATE words SET correct_count=correct_count+?, wrong_count=wrong_count+?,
                streak=CASE WHEN ?=1 THEN streak+1 ELSE 0 END,
                last_outcome=?, last_reviewed_at=? WHERE id=?",
        params![
            correct as i32,
            !correct as i32,
            correct as i32,
            if correct { "correct" } else { "wrong" },
            now,
            word_id
        ],
    )?;
    Ok(())
}

fn row_to_quiz_record(r: &rusqlite::Row) -> rusqlite::Result<QuizRecord> {
    let options_json: String = r.get(5)?;
    Ok(QuizRecord {
        id: r.get(0)?,
        word_id: r.get(1)?,
        group_id: r.get(2)?,
        quiz_type: r.get(3)?,
        question: r.get(4)?,
        options: serde_json::from_str(&options_json).unwrap_or_default(),
        correct_index: r.get(6)?,
        selected_index: r.get(7)?,
        is_correct: r.get::<_, Option<i32>>(8)?.map(|v| v != 0),
        source: r.get(9)?,
        created_at: r.get(10)?,
        answered_at: r.get(11)?,
        word: r.get(12)?,
        kana: r.get(13)?,
        meaning: r.get(14)?,
    })
}

/// 写入列（word/kana/meaning 不入库，由查询时 JOIN words 带出）。
const QUIZ_COLUMNS: &str = "id, word_id, group_id, quiz_type, question, options_json, correct_index, selected_index, is_correct, source, created_at, answered_at";

/// 查询前缀：LEFT JOIN words 带出目标词本体，词被删（重导入）时用空串兜底。
const QUIZ_SELECT: &str = "SELECT q.id, q.word_id, q.group_id, q.quiz_type, q.question, q.options_json, q.correct_index, q.selected_index, q.is_correct, q.source, q.created_at, q.answered_at, COALESCE(w.word,''), COALESCE(w.kana,''), COALESCE(w.meaning,'') FROM quiz_records q LEFT JOIN words w ON w.id=q.word_id";

/// 出题即落库（selected_index 为空），重启后未答的题依然可答。
pub fn insert_quiz_record(conn: &Connection, record: &QuizRecord) -> rusqlite::Result<()> {
    let options_json = serde_json::to_string(&record.options).unwrap_or_default();
    conn.execute(
        &format!("INSERT INTO quiz_records({QUIZ_COLUMNS}) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)"),
        params![
            record.id,
            record.word_id,
            record.group_id,
            record.quiz_type,
            record.question,
            options_json,
            record.correct_index,
            record.selected_index,
            record.is_correct.map(|v| v as i32),
            record.source,
            record.created_at,
            record.answered_at
        ],
    )?;
    Ok(())
}

fn quiz_record_by_id(conn: &Connection, quiz_id: &str) -> rusqlite::Result<Option<QuizRecord>> {
    conn.query_row(
        &format!("{QUIZ_SELECT} WHERE q.id=?"),
        [quiz_id],
        row_to_quiz_record,
    )
    .optional()
}

/// 重启后找回未作答的题（selected_index 为空才返回）。
/// 目前答题入口只靠消息携带的 quiz_json，本函数为后续恢复入口预留。
#[allow(dead_code)]
pub fn pending_quiz(conn: &Connection, quiz_id: &str) -> rusqlite::Result<Option<QuizRecord>> {
    Ok(quiz_record_by_id(conn, quiz_id)?.filter(|record| record.selected_index.is_none()))
}

/// 作答落库：事务内回填选择并联动单词统计；重复作答直接返回已落库结果，题目不存在返回 None。
pub fn answer_quiz_record(
    conn: &mut Connection,
    quiz_id: &str,
    selected_index: i64,
) -> rusqlite::Result<Option<QuizAnswerResult>> {
    let tx = conn.transaction()?;
    let Some(mut record) = quiz_record_by_id(&tx, quiz_id)? else {
        return Ok(None);
    };
    if record.selected_index.is_some() {
        return Ok(Some(QuizAnswerResult {
            record,
            already_answered: true,
        }));
    }
    let is_correct = selected_index == record.correct_index;
    let now = chrono::Utc::now().timestamp_millis();
    tx.execute(
        "UPDATE quiz_records SET selected_index=?, is_correct=?, answered_at=? WHERE id=?",
        params![selected_index, is_correct as i32, now, quiz_id],
    )?;
    record_quiz_result(&tx, record.word_id, is_correct)?;
    record.selected_index = Some(selected_index);
    record.is_correct = Some(is_correct);
    record.answered_at = Some(now);
    tx.commit()?;
    Ok(Some(QuizAnswerResult {
        record,
        already_answered: false,
    }))
}

/// 是否存在未作答且仍在答题有效期内的选择题。
/// 有效期 = 答题气泡显示时间（study_quiz_ttl_seconds）：气泡消失即视为放弃，不再阻塞后续出题
///（历史页仍显示为「未作答」）。应用启动前出的旧题一律视为已过期。
pub fn has_pending_quiz(
    conn: &Connection,
    now: i64,
    ttl_seconds: i64,
    app_started_at: i64,
) -> rusqlite::Result<bool> {
    let threshold = (now - ttl_seconds.max(1) * 1000).max(app_started_at);
    conn.query_row(
        "SELECT COUNT(*) FROM quiz_records WHERE selected_index IS NULL AND created_at > ?",
        [threshold],
        |r| r.get::<_, i64>(0),
    )
    .map(|count| count > 0)
}

/// 最近一次出题时间（持续模式限流用；无记录返回 None）。
pub fn latest_quiz_created_at(conn: &Connection) -> rusqlite::Result<Option<i64>> {
    conn.query_row("SELECT MAX(created_at) FROM quiz_records", [], |r| r.get(0))
}

/// 当前启用组是否全部单词都已掌握（streak>=2）；无启用组或组内无词时返回 false。
pub fn enabled_group_all_mastered(conn: &Connection) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT COUNT(*) > 0 AND COUNT(*) = SUM(CASE WHEN streak>=2 THEN 1 ELSE 0 END) \
         FROM words w JOIN word_groups g ON g.id=w.group_id WHERE g.enabled=1",
        [],
        |r| r.get::<_, bool>(0),
    )
}

/// 用户手动标记单词掌握度：mastered/shaky/forgotten/unlearned 四档，映射到统计字段（派生规则不变）。
pub fn set_word_mastery(
    conn: &Connection,
    word_id: i64,
    mastery: &str,
) -> rusqlite::Result<WordItem> {
    match mastery {
        // streak>=2 → 已掌握；有记录且最后答错 → 不记得；streak=1 → 勉强记得；清零 → 未学。
        "mastered" => conn.execute(
            "UPDATE words SET streak=2, last_outcome='correct', correct_count=CASE WHEN correct_count+wrong_count=0 THEN 2 ELSE correct_count END WHERE id=?",
            [word_id],
        )?,
        "shaky" => conn.execute(
            "UPDATE words SET streak=1, last_outcome='correct', correct_count=CASE WHEN correct_count+wrong_count=0 THEN 1 ELSE correct_count END WHERE id=?",
            [word_id],
        )?,
        "forgotten" => conn.execute(
            "UPDATE words SET streak=0, last_outcome='wrong', wrong_count=CASE WHEN correct_count+wrong_count=0 THEN 1 ELSE wrong_count END WHERE id=?",
            [word_id],
        )?,
        "unlearned" => conn.execute(
            "UPDATE words SET correct_count=0, wrong_count=0, streak=0, last_outcome=NULL, last_reviewed_at=NULL WHERE id=?",
            [word_id],
        )?,
        _ => {
            return Err(rusqlite::Error::InvalidParameterName(format!(
                "未知掌握度：{mastery}"
            )))
        }
    };
    let mut stmt = conn.prepare(
        "SELECT id, group_id, word, kana, meaning, correct_count, wrong_count, streak, last_outcome, last_reviewed_at FROM words WHERE id=?",
    )?;
    stmt.query_row([word_id], row_to_word_item)
}

/// 学习历史页的完整题目记录（含正确选项与用户选择），按时间倒序。
pub fn list_quiz_records(conn: &Connection, limit: u32) -> rusqlite::Result<Vec<QuizRecord>> {
    let mut stmt = conn.prepare(&format!(
        "{QUIZ_SELECT} ORDER BY q.created_at DESC LIMIT ?"
    ))?;
    let rows = stmt
        .query_map([limit.min(500)], row_to_quiz_record)?
        .collect();
    rows
}

/// 重排下一次主动复习时间；学习模式或复习开关关闭时清空（不再调度）。
pub fn reset_study_schedule(
    conn: &Connection,
    settings: &AppSettings,
    now: i64,
) -> rusqlite::Result<()> {
    if !settings.study_enabled || !settings.study_review_enabled {
        conn.execute(
            "UPDATE study_scheduler_state SET next_review_at=NULL WHERE id=1",
            [],
        )?;
        return Ok(());
    }
    let min = settings.study_review_min_minutes.max(1);
    let max = settings
        .study_review_max_minutes
        .max(settings.study_review_min_minutes)
        .max(1);
    let minutes = rand::random_range(min..=max);
    conn.execute(
        "UPDATE study_scheduler_state SET next_review_at=? WHERE id=1",
        [now + minutes * 60_000],
    )?;
    Ok(())
}

/// 到点且未超每日上限时认领一次主动复习：计数 +1 并重排下一次，漏发不补。
pub fn claim_study_review_due(
    conn: &Connection,
    settings: &AppSettings,
    now: i64,
) -> rusqlite::Result<bool> {
    if !settings.study_enabled || !settings.study_review_enabled {
        return Ok(false);
    }
    let day = local_day(now);
    let (next, saved_day, count): (Option<i64>, Option<String>, i64) = conn.query_row(
        "SELECT next_review_at, review_day, review_count FROM study_scheduler_state WHERE id=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let count = if saved_day.as_deref() == Some(&day) {
        count
    } else {
        0
    };
    if saved_day.as_deref() != Some(&day) {
        conn.execute(
            "UPDATE study_scheduler_state SET review_day=?, review_count=0 WHERE id=1",
            [&day],
        )?;
    }
    if next.is_none() {
        reset_study_schedule(conn, settings, now)?;
        return Ok(false);
    }
    if next.unwrap() > now || count >= settings.study_review_daily_limit {
        return Ok(false);
    }
    conn.execute(
        "UPDATE study_scheduler_state SET review_count=review_count+1 WHERE id=1",
        [],
    )?;
    reset_study_schedule(conn, settings, now)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_events_summary_splits_today() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("test.db")).unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        // 直接写入一条“昨天”的记录，验证今日/累计口径分离
        conn.execute(
            "INSERT INTO usage_events(created_at,category,prompt_tokens,completion_tokens) VALUES(?,?,?,?)",
            params![now - 86_460_000, "chat", 100i64, 10i64],
        )
        .unwrap();
        insert_usage_event(&conn, "chat", 200, 20).unwrap();
        insert_usage_event(&conn, "observer", 50, 5).unwrap();
        insert_usage_event(&conn, "asr", 0, 0).unwrap();
        let summaries = summarize_usage(&conn, now - 60_000).unwrap();
        let chat = summaries.iter().find(|s| s.category == "chat").unwrap();
        assert_eq!(chat.calls_total, 2);
        assert_eq!(chat.prompt_total, 300);
        assert_eq!(chat.completion_total, 30);
        assert_eq!(chat.calls_today, 1);
        assert_eq!(chat.prompt_today, 200);
        assert_eq!(chat.completion_today, 20);
        let observer = summaries.iter().find(|s| s.category == "observer").unwrap();
        assert_eq!(observer.calls_total, 1);
        assert_eq!(observer.calls_today, 1);
        let asr = summaries.iter().find(|s| s.category == "asr").unwrap();
        assert_eq!(asr.calls_total, 1);
        assert_eq!(asr.prompt_total, 0);
        assert_eq!(asr.completion_total, 0);
    }
    #[test]
    fn settings_and_messages_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("test.db")).unwrap();
        let mut settings = get_settings(&conn, false).unwrap();
        assert_eq!(settings.pet_name, "小栞");
        assert!(!settings.autostart_enabled);
        // 学习模式字段默认值与回读校验（get_settings 是位置索引 SELECT，防错位）。
        assert!(!settings.study_enabled);
        assert!(settings.study_isolated);
        assert!(settings.study_review_enabled);
        assert_eq!(settings.study_review_min_minutes, 30);
        assert_eq!(settings.study_review_max_minutes, 90);
        assert_eq!(settings.study_review_daily_limit, 8);
        settings.autostart_enabled = true;
        settings.study_enabled = true;
        settings.study_isolated = false;
        settings.study_review_min_minutes = 15;
        settings.study_review_max_minutes = 45;
        settings.study_review_daily_limit = 12;
        save_settings(&conn, &settings).unwrap();
        let loaded = get_settings(&conn, false).unwrap();
        assert!(loaded.autostart_enabled);
        assert!(loaded.study_enabled);
        assert!(!loaded.study_isolated);
        assert!(loaded.study_review_enabled);
        assert_eq!(loaded.study_review_min_minutes, 15);
        assert_eq!(loaded.study_review_max_minutes, 45);
        assert_eq!(loaded.study_review_daily_limit, 12);
        let m = Message {
            id: "1".into(),
            role: "user".into(),
            content: "hi".into(),
            japanese_text: None,
            emotion: None,
            trigger_type: "user_text".into(),
            created_at: 1,
            session: "main".into(),
            quiz: None,
        };
        insert_message(&conn, &m).unwrap();
        assert_eq!(list_messages(&conn, 10).unwrap().len(), 1);
    }

    /// 测试辅助：快速构造一条消息。
    fn msg(id: &str, role: &str, content: &str, trigger: &str, session: &str, at: i64) -> Message {
        Message {
            id: id.into(),
            role: role.into(),
            content: content.into(),
            japanese_text: None,
            emotion: None,
            trigger_type: trigger.into(),
            created_at: at,
            session: session.into(),
            quiz: None,
        }
    }

    /// 测试辅助：导入一个三词小组。
    fn seed_group(conn: &mut Connection, name: &str) -> Vec<WordItem> {
        upsert_word_group(
            conn,
            name,
            &format!("{name}.xlsx"),
            &[
                ("食べる".into(), "たべる".into(), "吃".into()),
                ("飲む".into(), "のむ".into(), "喝".into()),
                ("行く".into(), "いく".into(), "去".into()),
            ],
        )
        .unwrap();
        list_words(conn, name).unwrap()
    }

    #[test]
    fn word_group_reimport_preserves_stats_and_merges() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = open(&dir.path().join("test.db")).unwrap();
        let words = seed_group(&mut conn, "N5-动词");
        // 制造学习记录：食べる 答对两次（已掌握），飲む 答错一次（不记得）。
        record_quiz_result(&conn, words[0].id, true).unwrap();
        record_quiz_result(&conn, words[0].id, true).unwrap();
        record_quiz_result(&conn, words[1].id, false).unwrap();
        let mastered_id = words[0].id;
        // 重导入：食べる 意思改了、行く 被文件删除、新增 見る。
        upsert_word_group(
            &mut conn,
            "N5-动词",
            "N5-动词.xlsx",
            &[
                ("食べる".into(), "たべる".into(), "吃；进食".into()),
                ("飲む".into(), "のむ".into(), "喝".into()),
                ("見る".into(), "みる".into(), "看".into()),
            ],
        )
        .unwrap();
        let merged = list_words(&conn, "N5-动词").unwrap();
        assert_eq!(merged.len(), 3);
        let taberu = merged.iter().find(|w| w.word == "食べる").unwrap();
        // id 稳定、统计保留、意思更新。
        assert_eq!(taberu.id, mastered_id);
        assert_eq!(taberu.mastery, "mastered");
        assert_eq!(taberu.meaning, "吃；进食");
        assert_eq!(merged.iter().find(|w| w.word == "飲む").unwrap().mastery, "forgotten");
        // 删除的词没了，新词进来了。
        assert!(merged.iter().all(|w| w.word != "行く"));
        assert_eq!(merged.iter().find(|w| w.word == "見る").unwrap().mastery, "unlearned");
    }

    #[test]
    fn word_group_reimport_keeps_enabled_flag() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = open(&dir.path().join("test.db")).unwrap();
        seed_group(&mut conn, "N5-动词");
        set_enabled_group(&mut conn, Some("N5-动词")).unwrap();
        // 重导入（模拟文件内容变化）：词条替换但启用状态保留。
        upsert_word_group(
            &mut conn,
            "N5-动词",
            "N5-动词.xlsx",
            &[("見る".into(), "みる".into(), "看".into())],
        )
        .unwrap();
        let groups = list_word_groups(&conn).unwrap();
        assert_eq!(groups.len(), 1);
        assert!(groups[0].enabled);
        assert_eq!(groups[0].word_count, 1);
        assert_eq!(groups[0].unlearned, 1);
        let words = list_words(&conn, "N5-动词").unwrap();
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].word, "見る");
    }

    #[test]
    fn enabled_group_is_exclusive() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = open(&dir.path().join("test.db")).unwrap();
        seed_group(&mut conn, "A组");
        seed_group(&mut conn, "B组");
        set_enabled_group(&mut conn, Some("A组")).unwrap();
        set_enabled_group(&mut conn, Some("B组")).unwrap();
        let groups = list_word_groups(&conn).unwrap();
        assert_eq!(groups.iter().filter(|g| g.enabled).count(), 1);
        assert!(groups.iter().find(|g| g.id == "B组").unwrap().enabled);
        // 取消启用。
        set_enabled_group(&mut conn, None).unwrap();
        assert!(enabled_group_words(&conn).unwrap().is_empty());
        // 不存在的组报错。
        assert!(set_enabled_group(&mut conn, Some("不存在")).is_err());
    }

    #[test]
    fn quiz_result_updates_word_stats_and_mastery() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = open(&dir.path().join("test.db")).unwrap();
        let words = seed_group(&mut conn, "N5");
        let word_id = words[0].id;
        assert_eq!(list_words(&conn, "N5").unwrap()[0].mastery, "unlearned");
        record_quiz_result(&conn, word_id, true).unwrap();
        assert_eq!(list_words(&conn, "N5").unwrap()[0].mastery, "shaky");
        record_quiz_result(&conn, word_id, true).unwrap();
        // streak>=2 → 已掌握
        assert_eq!(list_words(&conn, "N5").unwrap()[0].mastery, "mastered");
        record_quiz_result(&conn, word_id, false).unwrap();
        // 最近一次答错 → 不记得
        let word = &list_words(&conn, "N5").unwrap()[0];
        assert_eq!(word.mastery, "forgotten");
        assert_eq!(word.streak, 0);
        assert_eq!(word.correct_count, 2);
        assert_eq!(word.wrong_count, 1);
        assert!(word.last_reviewed_at.is_some());
        // 组级掌握度计数联动。
        let group = &list_word_groups(&conn).unwrap()[0];
        assert_eq!(group.forgotten, 1);
        assert_eq!(group.unlearned, 2);
    }

    #[test]
    fn pending_quiz_respects_ttl_and_restart() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = open(&dir.path().join("test.db")).unwrap();
        let words = seed_group(&mut conn, "N5");
        let now = 1_000_000_000i64;
        let mut record = QuizRecord {
            id: "q1".into(),
            word_id: words[0].id,
            group_id: "N5".into(),
            quiz_type: "meaning".into(),
            question: "q".into(),
            options: vec!["吃".into(), "喝".into(), "去".into()],
            correct_index: 0,
            selected_index: None,
            is_correct: None,
            source: "study_review".into(),
            created_at: now - 10_000, // 10 秒前，在 60 秒答题期内
            answered_at: None,
            word: words[0].word.clone(),
            kana: words[0].kana.clone(),
            meaning: words[0].meaning.clone(),
        };
        insert_quiz_record(&conn, &record).unwrap();
        // 有效期内、启动之后 → 算未答。
        assert!(has_pending_quiz(&conn, now, 60, 0).unwrap());
        // 超过答题气泡时间 → 视为放弃。
        assert!(!has_pending_quiz(&conn, now, 5, 0).unwrap());
        // 应用重启（启动时间晚于出题时间）→ 旧题一律过期。
        assert!(!has_pending_quiz(&conn, now, 60, now - 5_000).unwrap());
        // 已作答的题不算未答：答掉两题后无未答题。
        record.created_at = now - 1_000;
        record.id = "q2".into();
        insert_quiz_record(&conn, &record).unwrap();
        answer_quiz_record(&mut conn, "q2", 0).unwrap();
        answer_quiz_record(&mut conn, "q1", 0).unwrap();
        assert!(!has_pending_quiz(&conn, now, 60, 0).unwrap());
    }

    #[test]
    fn quiz_record_insert_answer_and_reanswer() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = open(&dir.path().join("test.db")).unwrap();
        let words = seed_group(&mut conn, "N5");
        let record = QuizRecord {
            id: "q1".into(),
            word_id: words[0].id,
            group_id: "N5".into(),
            quiz_type: "meaning".into(),
            question: "「食べる」的中文意思是？".into(),
            options: vec!["吃".into(), "喝".into(), "去".into()],
            correct_index: 0,
            selected_index: None,
            is_correct: None,
            source: "study_review".into(),
            created_at: 1,
            answered_at: None,
            word: words[0].word.clone(),
            kana: words[0].kana.clone(),
            meaning: words[0].meaning.clone(),
        };
        insert_quiz_record(&conn, &record).unwrap();
        assert!(pending_quiz(&conn, "q1").unwrap().is_some());
        // 答错。
        let result = answer_quiz_record(&mut conn, "q1", 1).unwrap().unwrap();
        assert!(!result.already_answered);
        assert_eq!(result.record.is_correct, Some(false));
        assert!(pending_quiz(&conn, "q1").unwrap().is_none());
        // 单词统计已联动。
        assert_eq!(list_words(&conn, "N5").unwrap()[0].mastery, "forgotten");
        // 重复作答：返回已有结果，不再改统计。
        let again = answer_quiz_record(&mut conn, "q1", 0).unwrap().unwrap();
        assert!(again.already_answered);
        assert_eq!(again.record.selected_index, Some(1));
        assert_eq!(list_words(&conn, "N5").unwrap()[0].wrong_count, 1);
        // 不存在的题。
        assert!(answer_quiz_record(&mut conn, "不存在", 0).unwrap().is_none());
        // 查询时 JOIN words 带出目标词本体。
        let records = list_quiz_records(&conn, 10).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].word, "食べる");
        assert_eq!(records[0].kana, "たべる");
        assert_eq!(records[0].meaning, "吃");
        // 重导入删掉旧词后，历史题目以空串兜底，不丢记录。
        upsert_word_group(
            &mut conn,
            "N5",
            "N5.xlsx",
            &[("見る".into(), "みる".into(), "看".into())],
        )
        .unwrap();
        let records = list_quiz_records(&conn, 10).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].word, "");
        assert_eq!(records[0].kana, "");
        assert_eq!(records[0].meaning, "");
        // 已答记录仍可回显。
        assert_eq!(records[0].selected_index, Some(1));
    }

    #[test]
    fn study_scheduler_claim_and_day_rollover() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("test.db")).unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        let mut settings = get_settings(&conn, false).unwrap();
        // 默认关闭：不可认领，重排后仍无计划。
        assert!(!claim_study_review_due(&conn, &settings, now).unwrap());
        let next: Option<i64> = conn
            .query_row("SELECT next_review_at FROM study_scheduler_state WHERE id=1", [], |r| r.get(0))
            .unwrap();
        assert!(next.is_none());
        // 开启后：首次 claim 只重排不出题，到点后才认领。
        settings.study_enabled = true;
        settings.study_review_min_minutes = 1;
        settings.study_review_max_minutes = 1;
        assert!(!claim_study_review_due(&conn, &settings, now).unwrap());
        assert!(claim_study_review_due(&conn, &settings, now + 61_000).unwrap());
        // 认领后立即重排了下一次（1 分钟后），同一时刻不能连续认领。
        assert!(!claim_study_review_due(&conn, &settings, now + 61_000).unwrap());
        // 每日上限：上限为 1 时，到点也不再认领。
        settings.study_review_daily_limit = 1;
        assert!(!claim_study_review_due(&conn, &settings, now + 122_000).unwrap());
        // 日切换后计数清零恢复认领。
        assert!(claim_study_review_due(&conn, &settings, now + 86_400_000 + 61_000).unwrap());
        // 关闭复习开关：重排清空计划。
        settings.study_review_enabled = false;
        reset_study_schedule(&conn, &settings, now).unwrap();
        let next: Option<i64> = conn
            .query_row("SELECT next_review_at FROM study_scheduler_state WHERE id=1", [], |r| r.get(0))
            .unwrap();
        assert!(next.is_none());
    }

    #[test]
    fn session_isolation_for_history_and_observer() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("test.db")).unwrap();
        insert_message(&conn, &msg("u1", "user", "主会话", "user_text", "main", 1)).unwrap();
        insert_message(&conn, &msg("s1", "user", "学习会话", "user_text", "study", 2)).unwrap();
        insert_message(&conn, &msg("r1", "assistant", "复习出题", "study_review", "study", 3)).unwrap();
        // 主会话历史只含 main。
        let main = list_main_session_messages(&conn, None, 10).unwrap();
        assert_eq!(main.len(), 1);
        assert_eq!(main[0].content, "主会话");
        // 观察者只消费 main。
        let batch = observer_message_batch(&conn, 10).unwrap();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].1.content, "主会话");
        // 学习历史含 study 会话与复习出题。
        let study = list_study_messages(&conn, 10).unwrap();
        assert_eq!(study.len(), 2);
        // 按 session 拉取。
        assert_eq!(list_messages_by_session(&conn, "study", 10).unwrap().len(), 2);
        assert_eq!(list_messages_by_session(&conn, "main", 10).unwrap().len(), 1);
    }

    #[test]
    fn message_quiz_json_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("test.db")).unwrap();
        let mut m = msg("q1", "assistant", "来答一题", "study_review", "study", 1);
        m.quiz = Some(QuizView {
            quiz_id: "quiz-1".into(),
            question: "「食べる」的意思是？".into(),
            options: vec!["吃".into(), "喝".into(), "去".into()],
        });
        insert_message(&conn, &m).unwrap();
        let loaded = list_messages_by_session(&conn, "study", 1).unwrap();
        let quiz = loaded[0].quiz.as_ref().unwrap();
        assert_eq!(quiz.quiz_id, "quiz-1");
        assert_eq!(quiz.options.len(), 3);
        // 无题消息不携带 quiz 字段。
        insert_message(&conn, &msg("q2", "assistant", "普通消息", "companion_tick", "main", 2)).unwrap();
        assert!(list_messages_by_session(&conn, "main", 1).unwrap()[0].quiz.is_none());
    }

    #[test]
    fn todo_creates_and_claims_one_occurrence() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = open(&dir.path().join("test.db")).unwrap();
        let due = chrono::Utc::now().timestamp_millis() - 1;
        let todo = create_todo(&mut conn, "喝水", due, "Asia/Shanghai", None, None).unwrap();
        assert_eq!(list_pending_todos(&conn).unwrap().len(), 1);
        let claimed = claim_due_reminder(&mut conn, chrono::Utc::now().timestamp_millis())
            .unwrap()
            .unwrap();
        assert_eq!(claimed.todo_id, todo.id);
        assert!(
            claim_due_reminder(&mut conn, chrono::Utc::now().timestamp_millis())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn recurring_todo_reschedules_after_delivery() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = open(&dir.path().join("test.db")).unwrap();
        let due = chrono::Utc::now().timestamp_millis() - 1;
        let _todo =
            create_todo(&mut conn, "站起来活动", due, "Asia/Shanghai", None, Some(30)).unwrap();
        let claimed = claim_due_reminder(&mut conn, chrono::Utc::now().timestamp_millis())
            .unwrap()
            .unwrap();
        deliver_reminder(&conn, &claimed, None).unwrap();
        // 循环待办保持 pending，且排好了下一次未来时间的 occurrence。
        let todos = list_pending_todos(&conn).unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].repeat_interval_minutes, Some(30));
        assert!(todos[0].due_at_utc > chrono::Utc::now().timestamp_millis());
        assert_eq!(todos[0].due_at_utc - due, 30 * 60_000);
        // 下一次还未到点，不能被 claim。
        assert!(
            claim_due_reminder(&mut conn, chrono::Utc::now().timestamp_millis())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn delete_todo_blocks_future_claims() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = open(&dir.path().join("test.db")).unwrap();
        let due = chrono::Utc::now().timestamp_millis() - 1;
        let todo = create_todo(&mut conn, "喝水", due, "Asia/Shanghai", None, Some(30)).unwrap();
        assert!(delete_todo(&conn, &todo.id).unwrap());
        assert!(list_pending_todos(&conn).unwrap().is_empty());
        assert!(
            claim_due_reminder(&mut conn, chrono::Utc::now().timestamp_millis())
                .unwrap()
                .is_none()
        );
        // 重复删除返回 false。
        assert!(!delete_todo(&conn, &todo.id).unwrap());
    }

    #[test]
    fn mcp_server_crud_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("test.db")).unwrap();
        let input = McpServerInput {
            name: "music".into(),
            transport: "remote".into(),
            command: String::new(),
            args: String::new(),
            env: String::new(),
            url: "http://127.0.0.1:9000/mcp".into(),
            headers: "X-Api-Key=abc".into(),
            timeout_seconds: 45,
        };
        let server = insert_mcp_server(&conn, &input).unwrap();
        assert!(server.enabled);
        assert_eq!(server.timeout_seconds, 45);
        assert_eq!(list_mcp_servers(&conn).unwrap().len(), 1);
        // 名称唯一约束
        assert!(insert_mcp_server(&conn, &input).is_err());
        set_mcp_server_enabled(&conn, server.id, false).unwrap();
        assert!(!get_mcp_server(&conn, server.id).unwrap().unwrap().enabled);
        let renamed = McpServerInput { name: "music2".into(), ..input };
        assert!(update_mcp_server(&conn, server.id, &renamed).unwrap());
        assert_eq!(get_mcp_server(&conn, server.id).unwrap().unwrap().name, "music2");
        assert!(delete_mcp_server(&conn, server.id).unwrap());
        assert!(list_mcp_servers(&conn).unwrap().is_empty());
    }

    #[test]
    fn agent_run_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("test.db")).unwrap();
        let run = insert_agent_run(&conn, "查天气").unwrap();
        assert_eq!(run.status, "running");
        finish_agent_run(&conn, run.id, "done", "晴 25°C").unwrap();
        let runs = list_recent_agent_runs(&conn, 3).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "done");
        assert_eq!(runs[0].summary, "晴 25°C");
        assert!(runs[0].finished_at.is_some());
    }

    #[test]
    fn observer_batch_resumes_after_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("test.db")).unwrap();
        for index in 0..4 {
            insert_message(
                &conn,
                &Message {
                    id: index.to_string(),
                    role: if index % 2 == 0 { "user" } else { "assistant" }.into(),
                    content: format!("message {index}"),
                    japanese_text: None,
                    emotion: None,
                    trigger_type: "user_text".into(),
                    created_at: index,
                    session: "main".into(),
                    quiz: None,
                },
            )
            .unwrap();
        }
        let first = observer_message_batch(&conn, 2).unwrap();
        assert_eq!(first.len(), 2);
        advance_memory_observer(&conn, first.last().unwrap().0).unwrap();
        let second = observer_message_batch(&conn, 10).unwrap();
        assert_eq!(second.len(), 2);
        assert_eq!(second[0].1.content, "message 2");
    }

    #[test]
    fn main_session_history_filters_and_paginates() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("test.db")).unwrap();
        for index in 0..5 {
            insert_message(
                &conn,
                &Message {
                    id: format!("m{index}"),
                    role: if index % 2 == 0 { "user" } else { "assistant" }.into(),
                    content: format!("main {index}"),
                    japanese_text: None,
                    emotion: None,
                    trigger_type: "user_text".into(),
                    created_at: index * 10,
                    session: "main".into(),
                    quiz: None,
                },
            )
            .unwrap();
        }
        // 主动搭话消息（companion_tick）应被包含。
        insert_message(
            &conn,
            &Message {
                id: "p1".into(),
                role: "assistant".into(),
                content: "proactive".into(),
                japanese_text: None,
                emotion: None,
                trigger_type: "companion_tick".into(),
                created_at: 100,
                session: "main".into(),
                quiz: None,
            },
        )
        .unwrap();
        // 语音测试消息（voice_test）不属于会话历史，应被过滤。
        insert_message(
            &conn,
            &Message {
                id: "v1".into(),
                role: "assistant".into(),
                content: "voice test".into(),
                japanese_text: None,
                emotion: None,
                trigger_type: "voice_test".into(),
                created_at: 110,
                session: "main".into(),
                quiz: None,
            },
        )
        .unwrap();
        // 最新一页：2 条，返回正序（含主动搭话，不含语音测试）。
        let latest = list_main_session_messages(&conn, None, 2).unwrap();
        assert_eq!(latest.len(), 2);
        assert_eq!(latest[0].created_at, 40);
        assert_eq!(latest[1].created_at, 100);
        // 以页首 created_at 为游标向前翻页。
        let earlier = list_main_session_messages(&conn, Some(latest[0].created_at), 10).unwrap();
        assert_eq!(earlier.len(), 4);
        assert_eq!(earlier[0].created_at, 0);
        assert_eq!(earlier[3].created_at, 30);
    }
}
