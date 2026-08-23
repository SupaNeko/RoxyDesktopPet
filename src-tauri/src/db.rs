use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::sync::Mutex;

pub struct DbState(pub Mutex<Connection>);

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
    pub tool_hook_token: String,
    pub tool_hook_token_enabled: bool,
    pub tool_hook_fixed_text: String,
    pub tool_hook_fixed_voice_text: String,
    pub tool_hook_include_last_message: bool,
    pub tool_hook_min_interval_minutes: u32,
    pub tool_hook_daily_limit: u32,
    pub tool_hook_debounce_seconds: u32,
    pub tool_hook_voice_enabled: bool,
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
            tool_hook_token: String::new(),
            tool_hook_token_enabled: true,
            tool_hook_fixed_text: "你在 {tool} 里 {project} 的任务已经完成了。".into(),
            tool_hook_fixed_voice_text: String::new(),
            tool_hook_include_last_message: true,
            tool_hook_min_interval_minutes: 10,
            tool_hook_daily_limit: 20,
            tool_hook_debounce_seconds: 0,
            tool_hook_voice_enabled: true,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id: String,
    pub title: String,
    pub due_at_utc: i64,
    pub timezone: String,
    pub status: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct DueReminder {
    pub occurrence_id: String,
    pub todo_id: String,
    pub title: String,
    pub scheduled_at_utc: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub text: String,
    pub memory_type: String,
    pub importance: f64,
    pub status: String,
    pub embedding_status: String,
    pub created_at: i64,
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
         CREATE TABLE IF NOT EXISTS memories (id TEXT PRIMARY KEY, text TEXT NOT NULL, memory_type TEXT NOT NULL, importance REAL NOT NULL, status TEXT NOT NULL, embedding_collection TEXT, embedding_status TEXT NOT NULL, source_message_id TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS memory_observer_state (id INTEGER PRIMARY KEY CHECK (id = 1), last_message_rowid INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL DEFAULT 0);
         INSERT OR IGNORE INTO scheduler_state(id, proactive_count) VALUES(1, 0);
         INSERT OR IGNORE INTO memory_observer_state(id) VALUES(1);
         CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(created_at);
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
        ("tool_hook_include_last_message", "ALTER TABLE app_settings ADD COLUMN tool_hook_include_last_message INTEGER NOT NULL DEFAULT 1"),
        ("tool_hook_min_interval_minutes", "ALTER TABLE app_settings ADD COLUMN tool_hook_min_interval_minutes INTEGER NOT NULL DEFAULT 10"),
        ("tool_hook_daily_limit", "ALTER TABLE app_settings ADD COLUMN tool_hook_daily_limit INTEGER NOT NULL DEFAULT 20"),
        ("tool_hook_debounce_seconds", "ALTER TABLE app_settings ADD COLUMN tool_hook_debounce_seconds INTEGER NOT NULL DEFAULT 0"),
        ("tool_hook_voice_enabled", "ALTER TABLE app_settings ADD COLUMN tool_hook_voice_enabled INTEGER NOT NULL DEFAULT 1"),
    ] {
        if !columns.iter().any(|column| column == name) {
            conn.execute(sql, [])?;
        }
    }
    let d = AppSettings::default();
    conn.execute(
        "INSERT OR IGNORE INTO app_settings(id, pet_name, persona, user_name, api_base_url, api_model, voice_output_enabled) VALUES(1, ?, ?, ?, ?, ?, ?)",
        params![d.pet_name, d.persona, d.user_name, d.api_base_url, d.api_model, d.voice_output_enabled as i32],
    )?;
    Ok(conn)
}

pub fn get_settings(conn: &Connection, key_configured: bool) -> rusqlite::Result<AppSettings> {
    conn.query_row(
        "SELECT pet_name, persona, user_name, api_base_url, api_model, voice_output_enabled, microphone_device_name, asr_app_id, asr_api_key, asr_api_secret, proactive_enabled, proactive_min_minutes, proactive_max_minutes, proactive_daily_limit, qdrant_url, embedding_base_url, embedding_model, embedding_api_key, embedding_dimension, memory_observer_enabled, memory_observer_interval, voice_output_mode, tts_api_protocol, tts_api_base_url, tts_api_model, tts_api_key, tts_api_voice, tts_api_language, vits_model_name, vits_model_path, vits_speaker_id, vits_target_language, vits_speed, vits_emotion_params, vits_translate_enabled, tool_hook_enabled, tool_hook_mode, tool_hook_port, tool_hook_token, tool_hook_token_enabled, tool_hook_fixed_text, tool_hook_fixed_voice_text, tool_hook_include_last_message, tool_hook_min_interval_minutes, tool_hook_daily_limit, tool_hook_debounce_seconds, tool_hook_voice_enabled FROM app_settings WHERE id=1",
        [],
        |r| {
            let asr_app_id: String = r.get(7)?;
            let asr_api_key: String = r.get(8)?;
            let asr_api_secret: String = r.get(9)?;
            let embedding_api_key:String=r.get(17)?; let embedding_base_url:String=r.get(15)?; let embedding_model:String=r.get(16)?; let embedding_dimension:u32=r.get(18)?;
            let tts_api_key:String=r.get(25)?;
            Ok(AppSettings { pet_name: r.get(0)?, persona: r.get(1)?, user_name: r.get(2)?, api_base_url: r.get(3)?, api_model: r.get(4)?, api_key_configured: key_configured, voice_output_enabled: r.get::<_, i32>(5)? != 0, microphone_device_name: r.get(6)?, asr_configured: !asr_app_id.is_empty() && !asr_api_key.is_empty() && !asr_api_secret.is_empty(), asr_app_id, asr_api_key, asr_api_secret, proactive_enabled: r.get::<_, i32>(10)? != 0, proactive_min_minutes: r.get(11)?, proactive_max_minutes: r.get(12)?, proactive_daily_limit: r.get(13)?, qdrant_url:r.get(14)?, memory_configured:!embedding_base_url.is_empty()&&!embedding_model.is_empty()&&!embedding_api_key.is_empty()&&embedding_dimension>0, embedding_base_url,embedding_model,embedding_api_key,embedding_dimension, memory_observer_enabled:r.get::<_,i32>(19)? != 0, memory_observer_interval:r.get(20)?, voice_output_mode:r.get(21)?, tts_api_protocol:r.get(22)?, tts_api_base_url:r.get(23)?, tts_api_model:r.get(24)?, tts_api_configured:!tts_api_key.is_empty(), tts_api_key, tts_api_voice:r.get(26)?, tts_api_language:r.get(27)?, vits_model_name:r.get(28)?, vits_model_path:r.get(29)?, vits_speaker_id:r.get(30)?, vits_target_language:r.get(31)?, vits_speed:r.get(32)?, vits_emotion_params:r.get(33)?, vits_translate_enabled:r.get::<_,i32>(34)? != 0, tool_hook_enabled:r.get::<_,i32>(35)? != 0, tool_hook_mode:r.get(36)?, tool_hook_port:r.get(37)?, tool_hook_token:r.get(38)?, tool_hook_token_enabled:r.get::<_,i32>(39)? != 0, tool_hook_fixed_text:r.get(40)?, tool_hook_fixed_voice_text:r.get(41)?, tool_hook_include_last_message:r.get::<_,i32>(42)? != 0, tool_hook_min_interval_minutes:r.get(43)?, tool_hook_daily_limit:r.get(44)?, tool_hook_debounce_seconds:r.get(45)?, tool_hook_voice_enabled:r.get::<_,i32>(46)? != 0 })
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

pub fn save_settings(conn: &Connection, s: &AppSettings) -> rusqlite::Result<()> {
    conn.execute("UPDATE app_settings SET pet_name=?, persona=?, user_name=?, api_base_url=?, api_model=?, voice_output_enabled=?, microphone_device_name=?, asr_app_id=?, asr_api_key=?, asr_api_secret=?, proactive_enabled=?, proactive_min_minutes=?, proactive_max_minutes=?, proactive_daily_limit=?, qdrant_url=?, embedding_base_url=?, embedding_model=?, embedding_api_key=?, embedding_dimension=?, memory_observer_enabled=?, memory_observer_interval=?, voice_output_mode=?, tts_api_protocol=?, tts_api_base_url=?, tts_api_model=?, tts_api_key=?, tts_api_voice=?, tts_api_language=?, vits_model_name=?, vits_model_path=?, vits_speaker_id=?, vits_target_language=?, vits_speed=?, vits_emotion_params=?, vits_translate_enabled=?, tool_hook_enabled=?, tool_hook_mode=?, tool_hook_port=?, tool_hook_token=?, tool_hook_token_enabled=?, tool_hook_fixed_text=?, tool_hook_fixed_voice_text=?, tool_hook_include_last_message=?, tool_hook_min_interval_minutes=?, tool_hook_daily_limit=?, tool_hook_debounce_seconds=?, tool_hook_voice_enabled=? WHERE id=1",
        params![s.pet_name, s.persona, s.user_name, s.api_base_url, s.api_model, s.voice_output_enabled as i32, s.microphone_device_name, s.asr_app_id, s.asr_api_key, s.asr_api_secret, s.proactive_enabled as i32, s.proactive_min_minutes, s.proactive_max_minutes, s.proactive_daily_limit,s.qdrant_url,s.embedding_base_url,s.embedding_model,s.embedding_api_key,s.embedding_dimension,s.memory_observer_enabled as i32,s.memory_observer_interval,s.voice_output_mode,s.tts_api_protocol,s.tts_api_base_url,s.tts_api_model,s.tts_api_key,s.tts_api_voice,s.tts_api_language,s.vits_model_name,s.vits_model_path,s.vits_speaker_id,s.vits_target_language,s.vits_speed,s.vits_emotion_params,s.vits_translate_enabled as i32, s.tool_hook_enabled as i32, s.tool_hook_mode, s.tool_hook_port, s.tool_hook_token, s.tool_hook_token_enabled as i32, s.tool_hook_fixed_text, s.tool_hook_fixed_voice_text, s.tool_hook_include_last_message as i32, s.tool_hook_min_interval_minutes, s.tool_hook_daily_limit, s.tool_hook_debounce_seconds, s.tool_hook_voice_enabled as i32])?;
    Ok(())
}

pub fn insert_message(conn: &Connection, m: &Message) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO messages(id, role, content, japanese_text, emotion, trigger_type, created_at) VALUES(?, ?, ?, ?, ?, ?, ?)",
        params![m.id, m.role, m.content, m.japanese_text, m.emotion, m.trigger_type, m.created_at],
    )?;
    Ok(())
}

pub fn list_messages(conn: &Connection, limit: u32) -> rusqlite::Result<Vec<Message>> {
    let mut stmt = conn.prepare("SELECT id, role, content, japanese_text, emotion, trigger_type, created_at FROM (SELECT * FROM messages ORDER BY created_at DESC LIMIT ?) ORDER BY created_at")?;
    let messages = stmt
        .query_map([limit.min(500)], |r| {
            Ok(Message {
                id: r.get(0)?,
                role: r.get(1)?,
                content: r.get(2)?,
                japanese_text: r.get(3)?,
                emotion: r.get(4)?,
                trigger_type: r.get(5)?,
                created_at: r.get(6)?,
            })
        })?
        .collect();
    messages
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
    let mut stmt = conn.prepare("SELECT rowid,id,role,content,trigger_type,created_at FROM messages WHERE rowid>? AND trigger_type IN ('user_text','user_voice') ORDER BY rowid LIMIT ?")?;
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

pub fn memory_text_exists(conn: &Connection, text: &str) -> rusqlite::Result<bool> {
    conn.query_row("SELECT EXISTS(SELECT 1 FROM memories WHERE status='active' AND lower(trim(text))=lower(trim(?)))", [text], |r| r.get(0))
}

pub fn create_todo(
    conn: &mut Connection,
    title: &str,
    due_at_utc: i64,
    timezone: &str,
    source_message_id: Option<&str>,
) -> rusqlite::Result<Todo> {
    let now = chrono::Utc::now().timestamp_millis();
    let todo = Todo {
        id: uuid::Uuid::new_v4().to_string(),
        title: title.to_string(),
        due_at_utc,
        timezone: timezone.to_string(),
        status: "pending".into(),
        created_at: now,
    };
    let occurrence_id = uuid::Uuid::new_v4().to_string();
    let tx = conn.transaction()?;
    tx.execute("INSERT INTO todos(id,title,due_at_utc,timezone,status,source_message_id,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)", params![todo.id, todo.title, todo.due_at_utc, todo.timezone, todo.status, source_message_id, now, now])?;
    tx.execute("INSERT INTO reminder_occurrences(id,todo_id,scheduled_at_utc,status) VALUES(?,?,?,'pending')", params![occurrence_id, todo.id, due_at_utc])?;
    tx.commit()?;
    Ok(todo)
}

pub fn list_pending_todos(conn: &Connection) -> rusqlite::Result<Vec<Todo>> {
    let mut stmt = conn.prepare("SELECT id,title,due_at_utc,timezone,status,created_at FROM todos WHERE status='pending' ORDER BY due_at_utc LIMIT 100")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Todo {
                id: r.get(0)?,
                title: r.get(1)?,
                due_at_utc: r.get(2)?,
                timezone: r.get(3)?,
                status: r.get(4)?,
                created_at: r.get(5)?,
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
    conn.execute(
        "UPDATE todos SET status='completed',updated_at=? WHERE id=?",
        params![now, item.todo_id],
    )?;
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

pub fn insert_memory(
    conn: &Connection,
    text: &str,
    memory_type: &str,
    importance: f64,
    source_message_id: Option<&str>,
) -> rusqlite::Result<Memory> {
    let now = chrono::Utc::now().timestamp_millis();
    let memory = Memory {
        id: uuid::Uuid::new_v4().to_string(),
        text: text.into(),
        memory_type: memory_type.into(),
        importance,
        status: "active".into(),
        embedding_status: "pending".into(),
        created_at: now,
    };
    conn.execute("INSERT INTO memories(id,text,memory_type,importance,status,embedding_status,source_message_id,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)",params![memory.id,memory.text,memory.memory_type,memory.importance,memory.status,memory.embedding_status,source_message_id,now,now])?;
    Ok(memory)
}
pub fn set_memory_embedding_status(
    conn: &Connection,
    id: &str,
    status: &str,
    collection: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE memories SET embedding_status=?,embedding_collection=?,updated_at=? WHERE id=?",
        params![
            status,
            collection,
            chrono::Utc::now().timestamp_millis(),
            id
        ],
    )?;
    Ok(())
}
pub fn list_memories(conn: &Connection) -> rusqlite::Result<Vec<Memory>> {
    let mut stmt=conn.prepare("SELECT id,text,memory_type,importance,status,embedding_status,created_at FROM memories WHERE status='active' ORDER BY created_at DESC LIMIT 200")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Memory {
                id: r.get(0)?,
                text: r.get(1)?,
                memory_type: r.get(2)?,
                importance: r.get(3)?,
                status: r.get(4)?,
                embedding_status: r.get(5)?,
                created_at: r.get(6)?,
            })
        })?
        .collect();
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_and_messages_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("test.db")).unwrap();
        assert_eq!(get_settings(&conn, false).unwrap().pet_name, "小栞");
        let m = Message {
            id: "1".into(),
            role: "user".into(),
            content: "hi".into(),
            japanese_text: None,
            emotion: None,
            trigger_type: "user_text".into(),
            created_at: 1,
        };
        insert_message(&conn, &m).unwrap();
        assert_eq!(list_messages(&conn, 10).unwrap().len(), 1);
    }

    #[test]
    fn todo_creates_and_claims_one_occurrence() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = open(&dir.path().join("test.db")).unwrap();
        let due = chrono::Utc::now().timestamp_millis() - 1;
        let todo = create_todo(&mut conn, "喝水", due, "Asia/Shanghai", None).unwrap();
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
}
