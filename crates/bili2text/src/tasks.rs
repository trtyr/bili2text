//! 任务历史存储：提取/转写记录的 SQLite 持久化。
//!
//! 短任务（字幕提取）「完成后落一条记录」；长任务（本地转写）先写
//! running 记录，终态回写结果。历史用于 `bili2text history` 回看。

use rusqlite::Connection;
use serde::Serialize;

/// 历史记录归属的应用标识（schema 保留 tool_id 列以兼容旧库）。
const TOOL_ID: &str = "bili2text";

/// 一条任务记录。
#[derive(Debug, Clone, Serialize)]
pub struct TaskRecord {
    pub id: String,
    /// 任务类型，如 subtitle_extract / asr_transcribe。
    pub kind: String,
    /// 用户原始输入（链接/BV 号等）。
    pub input: String,
    /// 输出摘要（视频标题等）。
    pub title: Option<String>,
    pub bvid: Option<String>,
    /// pending / running / succeeded / failed。
    pub status: String,
    /// 运行阶段（长任务），如 downloading / transcribing。
    pub stage: Option<String>,
    /// 进度 0-100。
    pub progress: Option<i64>,
    pub error: Option<String>,
    pub lang: Option<String>,
    pub lines_count: Option<i64>,
    pub duration_secs: Option<i64>,
    /// 结果全文（仅详情返回）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_srt: Option<String>,
    /// Unix 秒级时间戳。
    pub created_at: i64,
    pub finished_at: Option<i64>,
}

/// 列表项（不含结果全文）。
#[derive(Debug, Serialize)]
pub struct TaskListItem {
    pub id: String,
    pub kind: String,
    pub input: String,
    pub title: Option<String>,
    pub bvid: Option<String>,
    pub status: String,
    pub stage: Option<String>,
    pub progress: Option<i64>,
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

/// 旧库迁移：补 progress / stage 列（已存在则忽略错误）。
fn migrate(conn: &Connection) {
    let _ = conn.execute("ALTER TABLE tasks ADD COLUMN progress INTEGER", []);
    let _ = conn.execute("ALTER TABLE tasks ADD COLUMN stage TEXT", []);
}

fn row_to_record(row: &rusqlite::Row<'_>, with_result: bool) -> rusqlite::Result<TaskRecord> {
    Ok(TaskRecord {
        id: row.get("id")?,
        kind: row.get("kind")?,
        input: row.get("input")?,
        title: row.get("title")?,
        bvid: row.get("bvid")?,
        status: row.get("status")?,
        stage: row.get("stage").ok().flatten(),
        progress: row.get("progress").ok().flatten(),
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
    /// 打开（或创建）存储；自动建表并迁移。目录不存在会自动创建。
    pub fn open(path: impl AsRef<std::path::Path>) -> rusqlite::Result<Self> {
        if let Some(dir) = path.as_ref().parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn);
        Ok(Self { conn: std::sync::Mutex::new(conn) })
    }

    /// 写入一条任务记录。status=running/pending 时不设 finished_at（长任务入口）。
    pub fn record(&self, mut task: TaskRecord) -> rusqlite::Result<String> {
        if task.id.is_empty() {
            task.id = uuid::Uuid::new_v4().to_string();
        }
        if task.created_at == 0 {
            task.created_at = now_secs();
        }
        let is_terminal = matches!(task.status.as_str(), "succeeded" | "failed");
        if task.finished_at.is_none() && is_terminal {
            task.finished_at = Some(now_secs());
        }
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO tasks (id, tool_id, kind, input, title, bvid, status, error, lang, \
             lines_count, duration_secs, result_text, result_srt, created_at, finished_at, \
             stage, progress) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
            rusqlite::params![
                task.id,
                TOOL_ID,
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
                task.stage,
                task.progress,
            ],
        )?;
        Ok(task.id)
    }

    /// 更新运行中任务的阶段与进度（长任务心跳）。
    #[cfg(feature = "transcribe")]
    pub fn update_progress(
        &self,
        id: &str,
        stage: &str,
        progress: i64,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE tasks SET stage = ?2, progress = ?3, status = 'running' WHERE id = ?1",
            rusqlite::params![id, stage, progress],
        )?;
        Ok(())
    }

    /// 完成长任务：写入终态与结果。
    #[cfg(feature = "transcribe")]
    #[allow(clippy::too_many_arguments)]
    pub fn finish(
        &self,
        id: &str,
        status: &str,
        error: Option<&str>,
        lang: Option<&str>,
        lines_count: Option<i64>,
        duration_secs: Option<i64>,
        result_text: Option<&str>,
        result_srt: Option<&str>,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE tasks SET status = ?2, error = ?3, lang = ?4, lines_count = ?5, \
             duration_secs = ?6, result_text = ?7, result_srt = ?8, progress = 100, \
             finished_at = ?9 WHERE id = ?1",
            rusqlite::params![
                id,
                status,
                error,
                lang,
                lines_count,
                duration_secs,
                result_text,
                result_srt,
                now_secs(),
            ],
        )?;
        Ok(())
    }

    /// 任务列表（按时间倒序；不含结果全文）。
    pub fn list(&self, limit: u32) -> rusqlite::Result<Vec<TaskListItem>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, kind, input, title, bvid, status, stage, progress, error, lang, \
             lines_count, duration_secs, created_at \
             FROM tasks ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![limit], |row| {
            Ok(TaskListItem {
                id: row.get("id")?,
                kind: row.get("kind")?,
                input: row.get("input")?,
                title: row.get("title")?,
                bvid: row.get("bvid")?,
                status: row.get("status")?,
                stage: row.get("stage").ok().flatten(),
                progress: row.get("progress").ok().flatten(),
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
            kind: "subtitle_extract".into(),
            input: "BV1J7hE6aEDQ".into(),
            title: Some("测试视频".into()),
            bvid: Some("BV1J7hE6aEDQ".into()),
            status: "succeeded".into(),
            stage: None,
            progress: Some(100),
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

        let list = store.list(10).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title.as_deref(), Some("测试视频"));

        let detail = store.get(&id).unwrap().unwrap();
        assert_eq!(detail.result_text.as_deref(), Some("全文"));
        assert!(detail.finished_at.is_some());

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
