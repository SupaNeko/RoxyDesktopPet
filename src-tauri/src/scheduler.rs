use crate::db::{self, DbState, Message};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub fn start(app: AppHandle) {
    // 应用启动时间：此前出的旧题一律视为已过期（不再阻塞出题）。
    let started_at = chrono::Utc::now().timestamp_millis();
    tauri::async_runtime::spawn(async move {
        log_info!("scheduler started");
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        let mut ticks: u64 = 0;
        loop {
            interval.tick().await;
            ticks += 1;
            if let Err(error) = tick(&app, started_at).await {
                log_error!("scheduler tick failed: {error}");
            }
            if ticks % 30 == 0 {
                log_info!("scheduler heartbeat (ticks={ticks}, ~{}s)", ticks * 2);
            }
        }
    });
}

async fn tick(app: &AppHandle, started_at: i64) -> Result<(), String> {
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
                        session: "main".into(),
                        quiz: None,
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

    // 日语学习：持续模式（答完即出下一题）与间隔复习（主动巩固）互斥。
    let (study_due, study_settings) = {
        let conn = db_state.0.lock().await;
        let settings = db::get_settings(&conn, true).map_err(|e| e.to_string())?;
        if settings.study_enabled && settings.study_continuous_enabled {
            // 持续模式：有未答完的题不出新题；距上一题 30 秒限流（覆盖生成中窗口）；
            // 全部掌握时停止自动出题（休息/换组建议由答题链路发出，避免每 tick 重复提醒）。
            let pending = db::has_pending_quiz(&conn, now, settings.study_quiz_ttl_seconds, started_at)
                .map_err(|e| e.to_string())?;
            let recent = db::latest_quiz_created_at(&conn)
                .map_err(|e| e.to_string())?
                .map(|t| now - t < 30_000)
                .unwrap_or(false);
            let all_mastered = db::enabled_group_all_mastered(&conn).unwrap_or(false);
            (!pending && !recent && !all_mastered, settings)
        } else {
            let due = db::claim_study_review_due(&conn, &settings, now).map_err(|e| e.to_string())?;
            // 上一题未答完时跳过本次（claim 已重排到下一随机时间点，届时再试）。
            let pending = if due {
                db::has_pending_quiz(&conn, now, settings.study_quiz_ttl_seconds, started_at)
                    .map_err(|e| e.to_string())?
            } else {
                false
            };
            if due && pending {
                log_info!("study review skipped: 上一道复习题还未作答");
            }
            (due && !pending, settings)
        }
    };
    if study_due {
        log_info!("study review due");
        match crate::commands::generate_study_quiz_message(app).await {
            Ok(message) => {
                log_info!(
                    "study review quiz message generated: {} chars",
                    message.content.chars().count()
                );
                let _ = app.emit("assistant-message", message.clone());
                crate::voice_output::schedule(app.clone(), message);
            }
            Err(error) => {
                // 生成失败：记日志并重排下一次复习（顺延，不补发）。
                log_warn!("study review quiz generation failed: {error}");
                let conn = db_state.0.lock().await;
                if let Err(e) = db::reset_study_schedule(&conn, &study_settings, now) {
                    log_error!("顺延复习计划失败：{e}");
                }
            }
        }
    }

    // 学习模式开启且与主会话隔离时：主会话的主动搭话不再触发（仅跳过本次生成，
    // 不修改用户设置；claim 已重排下一次时间，退出学习模式后恢复正常节奏）。
    let (proactive_due, study_isolated_active) = {
        let conn = db_state.0.lock().await;
        let settings = db::get_settings(&conn, true).map_err(|e| e.to_string())?;
        let isolated = settings.study_enabled && settings.study_isolated;
        (
            db::claim_proactive_due(&conn, &settings, now).map_err(|e| e.to_string())?,
            isolated,
        )
    };
    if proactive_due && study_isolated_active {
        log_info!("proactive message skipped: 日语学习模式（会话隔离）进行中");
    }
    if proactive_due && !study_isolated_active {
        log_info!("proactive message due");
        match crate::commands::generate_scheduled_message(app, "companion_tick", "主动和用户说一句自然、低打扰、有陪伴感的话。优先利用系统提示中提供的用户电脑实时状态（如果有）作为话题切入点：比如用户正在听的歌、在玩的游戏、在用的应用，都可以自然地聊起，也可以顺势以不确定的口吻推测并询问用户的音乐、游戏等偏好；没有可用线索时再结合最近对话找话题。先检查最近对话记录，自己刚说过的关心、提醒或话题（比如催睡觉、催休息）不要重复。不要声称观察到了未提供的信息。").await {
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
