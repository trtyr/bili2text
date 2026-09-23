//! 平台任务存储：工具执行的持久化历史（SQLite）。
//!
//! 这是平台能力：所有工具的执行记录（提取/转写/未来的长任务）统一落在这里，
//! 供前端历史回看与未来的任务队列/进度推送复用。
//!
//! 当前是「完成后落一条记录」的同步模型；长任务（如 ASR）接入时在同一张表上
//! 扩展 pending/running 状态机与进度字段。

use rusqlite::Connection;
use serde::Serialize;

/// 一条任务记录。
#[derive(Debug, Clone, Serialize)]
pub struct TaskRecord {
    pub id: String,
    pub tool_id: String,
    /// 任务类型，如 subtitle_extract / asr_transcribe。
    pub kind: String,
    /// 用户原始输入（链接/BV 号等）。
    pub input: String,
    /// 输出摘要（视频标题等）。
    pub title: Option<String>,
    pub bvid: Option<String>,
    /// succeeded / failed（长任务扩展 pending / running）。
    pub status: String,
    pub error: Option<String>,
    pub lang: Option<String>,
    pub lines_count: Option<i64>,
    pub duration_secs: Option<i64>,
    /// 结果全文（仅详情接口返回）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_srt: Option<String>,
    /// Unix 秒级时间戳。
    pub created_at: i64,
    pub finished_at: Option<i64>,
}

/// 列表项（不含结果全文，保持轻量）。
#[derive(Debug, Serialize)]
pub struct TaskListItem {
    pub id: String,
    pub tool_id: String,
    pub kind: String,
    pub input: String,
    pub title: Option<String>,
    pub bvid: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub lang: Option<String>,
    pub lines_count: Option<i64>,
    pub duration_secs: Option<i64>,
    pub created_at: i64,
}

/// SQLite 任务存储。线程安全：内部持连接互斥。
pub struct TaskStore {
    conn: std::sync::Mutex<Connection>,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    tool_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    input TEXT NOT NULL,
    title TEXT,
    bvid TEXT,
    status TEXT NOT NULL,
    error TEXT,
    lang TEXT,
    lines_count INTEGER,
    duration_secs INTEGER,
    result_text TEXT,
    result_srt TEXT,
    created_at INTEGER NOT NULL,
    finished_at INTEGER
);
CREATE INDEX IF NOT EXISTS idx_tasks_tool_created ON tasks(tool_id, created_at DESC);
";

fn row_to_record(row: &rusqlite::Row<'_>, with_result: bool) -> rusqlite::Result<TaskRecord> {
    Ok(TaskRecord {
        id: row.get("id")?,
        tool_id: row.get("tool_id")?,
        kind: row.get("kind")?,
        input: row.get("input")?,
        title: row.get("title")?,
        bvid: row.get("bvid")?,
        status: row.get("status")?,
        error: row.get("error")?,
        lang: row.get("lang")?,
        lines_count: row.get("lines_count")?,
        duration_secs: row.get("duration_secs")?,
        result_text: if with_result { row.get("result_text")? } else { None },
        result_srt: if with_result { row.get("result_srt")? } else { None },
        created_at: row.get("created_at")?,
        finished_at: row.get("finished_at")?,
    })
}

impl TaskStore {
    /// 打开（或创建）存储；自动建表。目录不存在会自动创建。
    pub fn open(path: impl AsRef<std::path::Path>) -> rusqlite::Result<Self> {
        if let Some(dir) = path.as_ref().parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn: std::sync::Mutex::new(conn) })
    }

    /// 写入一条已完成的任务记录（当前同步模型：record 即终态）。
    pub fn record(&self, mut task: TaskRecord) -> rusqlite::Result<String> {
        if task.id.is_empty() {
            task.id = uuid::Uuid::new_v4().to_string();
        }
        if task.created_at == 0 {
            task.created_at = now_secs();
        }
        if task.finished_at.is_none() {
            task.finished_at = Some(now_secs());
        }
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO tasks (id, tool_id, kind, input, title, bvid, status, error, lang, \
             lines_count, duration_secs, result_text, result_srt, created_at, finished_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            rusqlite::params![
                task.id,
                task.tool_id,
                task.kind,
                task.input,
                task.title,
                task.bvid,
                task.status,
                task.error,
                task.lang,
                task.lines_count,
                task.duration_secs,
                task.result_text,
                task.result_srt,
                task.created_at,
                task.finished_at,
            ],
        )?;
        Ok(task.id)
    }

    /// 任务列表（按时间倒序；可按工具过滤；不含结果全文）。
    pub fn list(&self, tool_id: Option<&str>, limit: u32) -> rusqlite::Result<Vec<TaskListItem>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, tool_id, kind, input, title, bvid, status, error, lang, \
             lines_count, duration_secs, created_at \
             FROM tasks WHERE (?1 IS NULL OR tool_id = ?1) \
             ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![tool_id, limit], |row| {
            Ok(TaskListItem {
                id: row.get("id")?,
                tool_id: row.get("tool_id")?,
                kind: row.get("kind")?,
                input: row.get("input")?,
                title: row.get("title")?,
                bvid: row.get("bvid")?,
                status: row.get("status")?,
                error: row.get("error")?,
                lang: row.get("lang")?,
                lines_count: row.get("lines_count")?,
                duration_secs: row.get("duration_secs")?,
                created_at: row.get("created_at")?,
            })
        })?;
        rows.collect()
    }

    /// 任务详情（含结果全文）。
    pub fn get(&self, id: &str) -> rusqlite::Result<Option<TaskRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT * FROM tasks WHERE id = ?1")?;
        let mut rows = stmt.query_map([id], |r| row_to_record(r, true))?;
        match rows.next() {
            Some(Ok(rec)) => Ok(Some(rec)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    /// 删除单条。
    pub fn delete(&self, id: &str) -> rusqlite::Result<bool> {
        let conn = self.conn.lock().unwrap();
        let n = conn.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
        Ok(n > 0)
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_store() -> TaskStore {
        TaskStore::open(":memory:").unwrap()
    }

    fn sample() -> TaskRecord {
        TaskRecord {
            id: String::new(),
            tool_id: "bili2text".into(),
            kind: "subtitle_extract".into(),
            input: "BV1J7hE6aEDQ".into(),
            title: Some("测试视频".into()),
            bvid: Some("BV1J7hE6aEDQ".into()),
            status: "succeeded".into(),
            error: None,
            lang: Some("ai-zh".into()),
            lines_count: Some(353),
            duration_secs: Some(785),
            result_text: Some("全文".into()),
            result_srt: Some("1\n00:00:00,000 --> 00:00:01,000\n全文".into()),
            created_at: 0,
            finished_at: None,
        }
    }

    #[test]
    fn record_list_get_delete_roundtrip() {
        let store = mem_store();
        let id = store.record(sample()).unwrap();
        assert!(!id.is_empty());

        // 列表：有内容、不含全文
        let list = store.list(Some("bili2text"), 10).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title.as_deref(), Some("测试视频"));

        // 其他工具过滤为空
        assert!(store.list(Some("other"), 10).unwrap().is_empty());

        // 详情：含全文
        let detail = store.get(&id).unwrap().unwrap();
        assert_eq!(detail.result_text.as_deref(), Some("全文"));
        assert!(detail.finished_at.is_some());

        // 删除
        assert!(store.delete(&id).unwrap());
        assert!(store.get(&id).unwrap().is_none());
        assert!(!store.delete(&id).unwrap());
    }

    #[test]
    fn failed_task_is_recorded() {
        let store = mem_store();
        let mut t = sample();
        t.status = "failed".into();
        t.error = Some("no_subtitle".into());
        t.result_text = None;
        t.result_srt = None;
        let id = store.record(t).unwrap();
        let got = store.get(&id).unwrap().unwrap();
        assert_eq!(got.status, "failed");
        assert_eq!(got.error.as_deref(), Some("no_subtitle"));
        assert!(got.result_text.is_none());
    }
}
