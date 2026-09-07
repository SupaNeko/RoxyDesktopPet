//! 日语学习模式：单词表导入（calamine 解析 xlsx）、掌握度派生、
//! 学习上下文构建、选择题校验与模板兜底出题。

use crate::db::{self, AppSettings, WordItem};
use calamine::{open_workbook_auto, Data, Reader};
use rand::seq::{IndexedRandom, SliceRandom};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// reply_to_user 工具中可选的 quiz 载荷（模型输出；word_id 容忍数字/字符串两种 JSON 形式）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizPayload {
    pub question: String,
    pub options: Vec<String>,
    pub correct_index: usize,
    /// 题型：meaning（释义）| spelling（拼写）| reading（读音）。
    pub quiz_type: String,
    #[serde(deserialize_with = "deserialize_word_id")]
    pub word_id: String,
}

fn deserialize_word_id<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(s) => Ok(s),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        _ => Err(serde::de::Error::custom("word_id 必须是字符串或数字")),
    }
}

/// validate_quiz 的输出：含目标词完整信息的内部结构（correct_index 不离开 Rust 端）。
#[derive(Debug, Clone)]
pub struct ValidatedQuiz {
    pub word_id: i64,
    pub quiz_type: String,
    pub question: String,
    pub options: Vec<String>,
    pub correct_index: usize,
    pub word: WordItem,
}

/// 掌握度派生（统计驱动，AI 不做标记）：
/// streak>=2 → 已掌握；最后一次答错 → 不记得；有答题记录 → 勉强记得；无记录 → 未学。
pub fn mastery_label(word: &WordItem) -> &'static str {
    if word.streak >= 2 {
        "已掌握"
    } else if word.last_outcome.as_deref() == Some("wrong") {
        "不记得"
    } else if word.correct_count + word.wrong_count > 0 {
        "勉强记得"
    } else {
        "未学"
    }
}

/// 掌握度的英文键（前端 WordItem.mastery / CSS 类名契约）：mastered/shaky/forgotten/unlearned。
pub fn mastery_key(word: &WordItem) -> &'static str {
    match mastery_label(word) {
        "已掌握" => "mastered",
        "不记得" => "forgotten",
        "勉强记得" => "shaky",
        _ => "unlearned",
    }
}

/// 掌握度升序排序权重：数值越小越优先出题（不记得最优先，已掌握最后）。
fn mastery_rank(word: &WordItem) -> u8 {
    match mastery_label(word) {
        "不记得" => 0,
        "勉强记得" => 1,
        "未学" => 2,
        _ => 3,
    }
}

/// 扫描 data_dir/wordgroups/*.xlsx 并导入（文件名即组名，表头：单词 / 平假名 / 中文意思）。
/// 组文件消失时保留 DB 记录并标记 file_missing。返回本次成功导入的组数。
pub fn scan_and_import(conn: &mut Connection, data_dir: &Path) -> Result<usize, String> {
    let dir = data_dir.join("wordgroups");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建单词组目录失败：{e}"))?;
    let mut seen: Vec<String> = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("读取单词组目录失败：{e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let is_xlsx = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("xlsx"))
            .unwrap_or(false);
        if !is_xlsx {
            continue;
        }
        let name = match path.file_stem().and_then(|s| s.to_str()) {
            Some(n) if !n.trim().is_empty() => n.trim().to_string(),
            _ => continue,
        };
        let file_name = entry.file_name().to_string_lossy().to_string();
        match read_xlsx_words(&path) {
            Ok(words) => {
                if words.is_empty() {
                    log_warn!("单词组「{name}」未解析到有效词条，跳过导入");
                    continue;
                }
                db::upsert_word_group(conn, &name, &file_name, &words)
                    .map_err(|e| format!("导入单词组「{name}」失败：{e}"))?;
                seen.push(name);
            }
            Err(error) => {
                log_warn!("解析单词组文件失败（{}）：{error}", path.display());
            }
        }
    }
    // 文件已删除的组保留记录与统计，仅标记缺失。
    let groups = db::list_word_groups(conn).map_err(|e| e.to_string())?;
    for group in groups {
        let missing = !seen.contains(&group.id);
        if missing != group.file_missing {
            db::mark_word_group_missing(conn, &group.id, missing).map_err(|e| e.to_string())?;
        }
    }
    Ok(seen.len())
}

/// 读取单个 xlsx：第一个工作表，跳过表头行，三列映射（单词/平假名/中文意思），容忍空行。
fn read_xlsx_words(path: &Path) -> Result<Vec<(String, String, String)>, String> {
    let mut workbook = open_workbook_auto(path).map_err(|e| format!("打开 xlsx 失败：{e}"))?;
    let range = workbook
        .worksheet_range_at(0)
        .ok_or("xlsx 中没有工作表")?
        .map_err(|e| format!("读取工作表失败：{e}"))?;
    let mut words = Vec::new();
    for (index, row) in range.rows().enumerate() {
        if index == 0 {
            continue; // 表头行
        }
        let word = cell_text(row.first());
        if word.is_empty() {
            continue; // 空行
        }
        let kana = cell_text(row.get(1));
        if kana.is_empty() {
            log_warn!("单词「{word}」缺少平假名");
        }
        let meaning = cell_text(row.get(2));
        words.push((word, kana, meaning));
    }
    Ok(words)
}

fn cell_text(cell: Option<&Data>) -> String {
    match cell {
        Some(Data::String(s)) | Some(Data::DateTimeIso(s)) | Some(Data::DurationIso(s)) => {
            s.trim().to_string()
        }
        Some(Data::Float(f)) => f.to_string(),
        Some(Data::Int(i)) => i.to_string(),
        Some(Data::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

/// 全部题型（存储与提示词的规范顺序）：meaning 释义 / spelling 拼写 / reading 读音。
pub const ALL_QUIZ_TYPES: [&str; 3] = ["meaning", "spelling", "reading"];

/// 规范化题型配置串：逗号分隔，只保留合法题型并按规范顺序去重；全不合法/为空时回退为全部题型。
pub fn normalize_quiz_types(raw: &str) -> String {
    let selected: Vec<&str> = ALL_QUIZ_TYPES
        .iter()
        .copied()
        .filter(|t| raw.split(',').any(|v| v.trim() == *t))
        .collect();
    let selected = if selected.is_empty() {
        ALL_QUIZ_TYPES.to_vec()
    } else {
        selected
    };
    selected.join(",")
}

/// 当前启用的题型列表（settings 中已规范化；防御性兜底为全部题型）。
pub fn enabled_quiz_types(settings: &AppSettings) -> Vec<&'static str> {
    let types: Vec<&'static str> = ALL_QUIZ_TYPES
        .iter()
        .copied()
        .filter(|t| {
            settings
                .study_quiz_types
                .split(',')
                .any(|v| v.trim() == *t)
        })
        .collect();
    if types.is_empty() {
        ALL_QUIZ_TYPES.to_vec()
    } else {
        types
    }
}

/// 学习模式开启时注入系统提示词的出题约定（只列出用户启用的题型）。
pub fn quiz_instructions(settings: &AppSettings) -> String {
    let descriptions: Vec<&str> = enabled_quiz_types(settings)
        .iter()
        .map(|t| match *t {
            "meaning" => "meaning（给日语单词选中文意思，正确选项是该词的中文意思）",
            "spelling" => "spelling（给中文意思选日语单词写法，正确选项是单词）",
            _ => "reading（给日语单词选平假名读音，正确选项是平假名）",
        })
        .collect();
    format!(
        "【日语学习出题规则】\n用户要求「考考我」「出个题」等，或系统要求出题时，在 reply_to_user 的 quiz 字段中输出一道三选一选择题：\n- question：题干（中文）；options：恰好 3 个选项；correct_index：正确选项下标（0 起）；word_id：上方单词清单中目标词的 id；quiz_type：题型。\n- 只允许使用以下题型：{}。\n- 目标词与全部选项必须来自上方单词清单，干扰项从同组其他词中挑选；正确选项内容必须与该词在清单中对应字段完全一致。\n- 优先从掌握度低（不记得、未学、勉强记得）的词中选题。不出题时不要填 quiz 字段。",
        descriptions.join("；")
    )
}

/// 学习模式注入的系统提示词段落：掌握度汇总 + 按掌握度升序截断 80 词的清单。
/// 仅在学习模式开启且有启用组、组内有词时返回非空。
pub fn build_study_context(conn: &Connection, settings: &AppSettings) -> String {
    if !settings.study_enabled {
        return String::new();
    }
    let groups = db::list_word_groups(conn).unwrap_or_default();
    let Some(group) = groups.iter().find(|g| g.enabled) else {
        return String::new();
    };
    let words = db::enabled_group_words(conn).unwrap_or_default();
    if words.is_empty() {
        return String::new();
    }
    let mut sorted = words;
    sorted.sort_by_key(|w| (mastery_rank(w), w.last_reviewed_at.unwrap_or(0)));
    let lines = sorted
        .iter()
        .take(80)
        .map(|w| {
            format!(
                "{} | {} | {} | {} | {}",
                w.id,
                w.word,
                w.kana,
                w.meaning,
                mastery_label(w)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "\n【日语学习模式】\n当前启用单词组：{}（共 {} 词；已掌握 {} / 勉强记得 {} / 不记得 {} / 未学 {}）\n组内单词（按掌握度从低到高，最多 80 个；出题优先选靠前的低掌握度词）：\nid | 单词 | 假名 | 意思 | 掌握度\n{lines}",
        group.name,
        group.word_count,
        group.mastered,
        group.shaky,
        group.forgotten,
        group.unlearned
    )
}

/// 校验模型输出的 quiz：题型合法且在用户启用的题型内、恰好 3 个选项、correct_index 合法、
/// word_id 在启用组内、正确选项与该词题型对应字段（释义=中文意思 / 拼写=单词 / 读音=平假名）完全一致。
pub fn validate_quiz(
    quiz: &QuizPayload,
    words: &[WordItem],
    allowed_types: &[&str],
) -> Result<ValidatedQuiz, String> {
    if !matches!(quiz.quiz_type.as_str(), "meaning" | "spelling" | "reading") {
        return Err(format!(
            "quiz_type 无效（{}），必须是 meaning/spelling/reading",
            quiz.quiz_type
        ));
    }
    if !allowed_types.contains(&quiz.quiz_type.as_str()) {
        return Err(format!(
            "题型 {} 未启用，请改用以下题型：{}",
            quiz.quiz_type,
            allowed_types.join("/")
        ));
    }
    if quiz.question.trim().is_empty() {
        return Err("question 不能为空".into());
    }
    if quiz.options.len() != 3 {
        return Err(format!("options 必须恰好 3 个（实际 {} 个）", quiz.options.len()));
    }
    if quiz.correct_index >= quiz.options.len() {
        return Err(format!("correct_index 越界（{}）", quiz.correct_index));
    }
    let word_id: i64 = quiz
        .word_id
        .trim()
        .parse()
        .map_err(|_| format!("word_id 无效（{}）", quiz.word_id))?;
    let word = words
        .iter()
        .find(|w| w.id == word_id)
        .ok_or_else(|| format!("word_id {word_id} 不在当前启用组内"))?;
    let expected = field_for_type(&quiz.quiz_type)(word).trim().to_string();
    if expected.is_empty() {
        return Err(format!("单词「{}」缺少该题型所需的字段", word.word));
    }
    if quiz.options[quiz.correct_index].trim() != expected {
        return Err(format!("correct_index 指向的选项与标准答案「{expected}」不一致"));
    }
    Ok(ValidatedQuiz {
        word_id,
        quiz_type: quiz.quiz_type.clone(),
        question: quiz.question.trim().to_string(),
        options: quiz.options.iter().map(|o| o.trim().to_string()).collect(),
        correct_index: quiz.correct_index,
        word: word.clone(),
    })
}

fn word_meaning(w: &WordItem) -> &str {
    w.meaning.as_str()
}
fn word_spelling(w: &WordItem) -> &str {
    w.word.as_str()
}
fn word_reading(w: &WordItem) -> &str {
    w.kana.as_str()
}

/// 题型 → 取该词作为正确答案的字段。
fn field_for_type(quiz_type: &str) -> fn(&WordItem) -> &str {
    match quiz_type {
        "meaning" => word_meaning,
        "spelling" => word_spelling,
        _ => word_reading,
    }
}

/// Rust 模板兜底出题：随机目标词 + 同组 2 个干扰项，从用户启用的题型中随机
///（目标词缺假名时避开读音题）。组内不足 3 词或干扰项不够时无法出题，返回 None。
pub fn template_quiz_fallback(words: &[WordItem], allowed_types: &[&str]) -> Option<QuizPayload> {
    if words.len() < 3 {
        return None;
    }
    let mut rng = rand::rng();
    let target = words.choose(&mut rng)?;
    let mut quiz_types: Vec<&str> = allowed_types.to_vec();
    if target.kana.trim().is_empty() {
        quiz_types.retain(|t| *t != "reading");
    }
    let quiz_type = *quiz_types.choose(&mut rng)?;
    let field = field_for_type(quiz_type);
    let correct = field(target).trim().to_string();
    if correct.is_empty() {
        return None;
    }
    // 干扰项：同组其他词的同字段取值，去空、去重、排除正确答案。
    let mut pool: Vec<String> = words
        .iter()
        .filter(|w| w.id != target.id)
        .map(|w| field(w).trim().to_string())
        .filter(|v| !v.is_empty() && *v != correct)
        .collect();
    pool.sort();
    pool.dedup();
    if pool.len() < 2 {
        return None;
    }
    pool.shuffle(&mut rng);
    let mut options = vec![correct.clone(), pool.remove(0), pool.remove(0)];
    options.shuffle(&mut rng);
    let correct_index = options.iter().position(|o| *o == correct).unwrap_or(0);
    let question = match quiz_type {
        "meaning" => format!("「{}」的中文意思是？", target.word),
        "spelling" => format!("「{}」对应的日语单词是？", target.meaning),
        _ => format!("「{}」的平假名读音是？", target.word),
    };
    Some(QuizPayload {
        question,
        options,
        correct_index,
        quiz_type: quiz_type.to_string(),
        word_id: target.id.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::AppSettings;

    fn word(id: i64, streak: i64, last_outcome: Option<&str>, correct: i64, wrong: i64) -> WordItem {
        WordItem {
            id,
            group_id: "g1".into(),
            word: format!("単語{id}"),
            kana: format!("たんご{id}"),
            meaning: format!("意思{id}"),
            correct_count: correct,
            wrong_count: wrong,
            streak,
            last_outcome: last_outcome.map(str::to_string),
            last_reviewed_at: None,
            mastery: String::new(),
        }
    }

    const ALL: [&str; 3] = ["meaning", "spelling", "reading"];

    #[test]
    fn quiz_types_normalize_and_enabled() {
        // 保序去重、过滤非法值。
        assert_eq!(normalize_quiz_types("reading,meaning,foo,reading"), "meaning,reading");
        // 全不合法或为空时回退全部题型。
        assert_eq!(normalize_quiz_types("foo"), "meaning,spelling,reading");
        assert_eq!(normalize_quiz_types(""), "meaning,spelling,reading");
        let mut settings = AppSettings::default();
        assert_eq!(enabled_quiz_types(&settings), ALL);
        settings.study_quiz_types = "reading".into();
        assert_eq!(enabled_quiz_types(&settings), ["reading"]);
        // 防御：异常值仍回退全部。
        settings.study_quiz_types = "foo".into();
        assert_eq!(enabled_quiz_types(&settings), ALL);
        // 出题约定只列出启用的题型。
        settings.study_quiz_types = "spelling".into();
        let instructions = quiz_instructions(&settings);
        assert!(instructions.contains("spelling"));
        assert!(!instructions.contains("reading（"));
    }

    #[test]
    fn mastery_label_derivation_boundaries() {
        assert_eq!(mastery_label(&word(1, 0, None, 0, 0)), "未学");
        assert_eq!(mastery_label(&word(2, 2, Some("correct"), 2, 0)), "已掌握");
        assert_eq!(mastery_label(&word(3, 5, Some("correct"), 5, 0)), "已掌握");
        assert_eq!(mastery_label(&word(4, 0, Some("wrong"), 1, 1)), "不记得");
        assert_eq!(mastery_label(&word(5, 1, Some("correct"), 1, 0)), "勉强记得");
        // streak>=2 优先于其他条件。
        assert_eq!(mastery_label(&word(6, 2, None, 2, 0)), "已掌握");
    }

    #[test]
    fn validate_quiz_accepts_valid_payload() {
        let words = vec![word(1, 0, None, 0, 0), word(2, 0, None, 0, 0)];
        let quiz = QuizPayload {
            question: "「単語1」的中文意思是？".into(),
            options: vec!["意思1".into(), "意思2".into(), "其他".into()],
            correct_index: 0,
            quiz_type: "meaning".into(),
            word_id: "1".into(),
        };
        let validated = validate_quiz(&quiz, &words, &ALL).unwrap();
        assert_eq!(validated.word_id, 1);
        assert_eq!(validated.options.len(), 3);
        // word_id 容忍数字形式的 JSON。
        let quiz_number: QuizPayload = serde_json::from_str(
            r#"{"question":"q","options":["意思1","a","b"],"correct_index":0,"quiz_type":"meaning","word_id":1}"#,
        )
        .unwrap();
        assert!(validate_quiz(&quiz_number, &words, &ALL).is_ok());
        // 未启用的题型被拒绝，并提示可用题型。
        let error = validate_quiz(&quiz, &words, &["reading"]).unwrap_err();
        assert!(error.contains("未启用"));
    }

    #[test]
    fn validate_quiz_rejects_each_invalid_branch() {
        let words = vec![word(1, 0, None, 0, 0), word(2, 0, None, 0, 0)];
        let base = QuizPayload {
            question: "q".into(),
            options: vec!["意思1".into(), "x".into(), "y".into()],
            correct_index: 0,
            quiz_type: "meaning".into(),
            word_id: "1".into(),
        };
        // 题型非法
        let bad = QuizPayload { quiz_type: "unknown".into(), ..base.clone() };
        assert!(validate_quiz(&bad, &words, &ALL).is_err());
        // 选项数不为 3
        let bad = QuizPayload { options: vec!["意思1".into(), "x".into()], ..base.clone() };
        assert!(validate_quiz(&bad, &words, &ALL).is_err());
        // correct_index 越界
        let bad = QuizPayload { correct_index: 3, ..base.clone() };
        assert!(validate_quiz(&bad, &words, &ALL).is_err());
        // word_id 不在组内
        let bad = QuizPayload { word_id: "99".into(), ..base.clone() };
        assert!(validate_quiz(&bad, &words, &ALL).is_err());
        // word_id 非数字
        let bad = QuizPayload { word_id: "abc".into(), ..base.clone() };
        assert!(validate_quiz(&bad, &words, &ALL).is_err());
        // 正确选项与标准答案不一致
        let bad = QuizPayload { correct_index: 1, ..base.clone() };
        assert!(validate_quiz(&bad, &words, &ALL).is_err());
        // 读音题但目标词缺假名
        let mut no_kana = word(3, 0, None, 0, 0);
        no_kana.kana.clear();
        let words2 = vec![no_kana];
        let bad = QuizPayload {
            question: "q".into(),
            options: vec!["".into(), "a".into(), "b".into()],
            correct_index: 0,
            quiz_type: "reading".into(),
            word_id: "3".into(),
        };
        assert!(validate_quiz(&bad, &words2, &ALL).is_err());
    }

    #[test]
    fn fallback_quiz_passes_validation() {
        let words: Vec<WordItem> = (1..=5).map(|i| word(i, 0, None, 0, 0)).collect();
        for _ in 0..20 {
            let payload = template_quiz_fallback(&words, &ALL).expect("5 词应能兜底出题");
            let validated = validate_quiz(&payload, &words, &ALL).unwrap();
            assert_eq!(validated.options.len(), 3);
            assert_eq!(validated.word_id.to_string(), payload.word_id);
        }
        // 只启用读音题时，兜底题也必须是读音题。
        for _ in 0..20 {
            let payload = template_quiz_fallback(&words, &["reading"]).unwrap();
            assert_eq!(payload.quiz_type, "reading");
        }
        // 不足 3 词时无法出题。
        assert!(template_quiz_fallback(&words[..2], &ALL).is_none());
    }

    #[test]
    fn study_context_summary_and_truncation() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::db::open(&dir.path().join("test.db")).unwrap();
        let entries: Vec<(String, String, String)> = (1..=100)
            .map(|i| (format!("w{i}"), format!("k{i}"), format!("m{i}")))
            .collect();
        crate::db::upsert_word_group(&mut conn, "N5-样例", "N5-样例.xlsx", &entries).unwrap();
        crate::db::set_enabled_group(&mut conn, Some("N5-样例")).unwrap();
        let mut settings = AppSettings::default();
        // 学习模式关闭时不注入。
        assert!(build_study_context(&conn, &settings).is_empty());
        settings.study_enabled = true;
        let context = build_study_context(&conn, &settings);
        assert!(context.contains("N5-样例"));
        assert!(context.contains("未学 100"));
        assert!(context.contains("id | 单词 | 假名 | 意思 | 掌握度"));
        // 截断到 80 词。
        let line_count = context.lines().filter(|l| l.contains(" | w")).count();
        assert_eq!(line_count, 80);
        // 低掌握度词排在前面：人为把 99、100 号词标成「不记得」「勉强记得」。
        let words = crate::db::list_words(&conn, "N5-样例").unwrap();
        let w99 = words.iter().find(|w| w.word == "w99").unwrap();
        crate::db::record_quiz_result(&conn, w99.id, false).unwrap();
        let context = build_study_context(&conn, &settings);
        let pos99 = context.find(" | w99 | ").unwrap();
        let pos1 = context.find(" | w1 | ").unwrap();
        assert!(pos99 < pos1, "不记得的词应排在未学的词前面");
    }
}
