use crate::db::{self, DbState, Message};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        log_info!("scheduler started");
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        let mut ticks: u64 = 0;
        loop {
            interval.tick().await;
            ticks += 1;
            if let Err(error) = tick(&app).await {
                log_error!("scheduler tick failed: {error}");
            }
            if ticks % 30 == 0 {
                log_info!("scheduler heartbeat (ticks={ticks}, ~{}s)", ticks * 2);
            }
        }
    });
}

async fn tick(app: &AppHandle) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp_millis();
    let db_state = app.state::<DbState>();
    let reminder = {
        let mut conn = db_state.0.lock().await;
        db::claim_due_reminder(&mut conn, now).map_err(|e| e.to_string())?
    };
    if let Some(item) = reminder {
        log_info!("reminder due, claiming todo: {} ({})", item.title, item.todo_id);
        let due = chrono::DateTime::from_timestamp_millis(item.scheduled_at_utc)
            .unwrap_or_default()
            .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
            .format("%Y-%m-%d %H:%M:%S");
        let prompt = format!(
            "现在必须提醒用户：{}。原定时间（Asia/Shanghai）：{}。直接提醒，不要创建新的待办。",
            item.title, due
        );
        let generated =
            crate::commands::generate_scheduled_message(app, "reminder_due", &prompt).await;
        let (message, error) = match generated {
            Ok(message) => {
                log_info!("reminder message generated: {} chars", message.content.chars().count());
                (message, None)
            }
            Err(ref e) => {
                log_warn!("reminder message generation failed, using fallback: {e}");
                (
                    Message {
                        id: uuid::Uuid::new_v4().to_string(),
                        role: "assistant".into(),
                        content: format!("提醒：{}", item.title),
                        japanese_text: None,
                        emotion: Some("calm".into()),
                        trigger_type: "reminder_due".into(),
                        created_at: chrono::Utc::now().timestamp_millis(),
                    },
                    Some(e.clone()),
                )
            }
        };
        {
            let conn = db_state.0.lock().await;
            if error.is_some() {
                db::insert_message(&conn, &message).map_err(|e| e.to_string())?;
            }
            db::deliver_reminder(&conn, &item, error.as_deref()).map_err(|e| e.to_string())?;
        }
        let _ = app.emit("assistant-message", message.clone());
        crate::voice_output::schedule(app.clone(), message);
        log_info!("reminder delivered (fallback={})", error.is_some());
    }

    let proactive_due = {
        let conn = db_state.0.lock().await;
        let settings = db::get_settings(&conn, true).map_err(|e| e.to_string())?;
        db::claim_proactive_due(&conn, &settings, now).map_err(|e| e.to_string())?
    };
    if proactive_due {
        log_info!("proactive message due");
        match crate::commands::generate_scheduled_message(app, "companion_tick", "主动和用户说一句自然、低打扰、有陪伴感的话。可以结合最近对话，但不要声称观察到了未提供的信息。").await {
            Ok(message) => {
                log_info!("proactive message generated: {} chars", message.content.chars().count());
                let _ = app.emit("assistant-message", message.clone());
                crate::voice_output::schedule(app.clone(), message);
            }
            Err(error) => {
                log_error!("proactive message generation failed: {error}");
            }
        }
    }
    Ok(())
}
