//! 远程转写引擎：对接转写服务 HTTP API（tailnet 内，如 http://100.64.0.5:8765）。
//!
//! 契约（v0.1.0）：
//! - `POST /v1/transcripts`（multipart 字段 `file`）→ 202 `{task_id, state, queue_position}`
//! - `GET /v1/transcripts/{id}` → `{state: queued|processing|completed|failed, progress, result?, error?}`
//! - result：`{lang, text, segments[{start,end,text}], srt}`，秒级 float
//! - 鉴权：`Authorization: Bearer <token>`；healthz 免鉴权
//!
//! 配置来源（优先级）：环境变量 `BILI2TEXT_REMOTE_URL` + `BILI2TEXT_REMOTE_TOKEN`
//! → 数据目录 `remote.json`（`{"url": "...", "token": "..."}`）。token 值不落代码与文档。

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::Deserialize;

use crate::error::AppError;
use crate::transcriber::{Segment, Transcript};

/// 轮询间隔（秒）。
const POLL_INTERVAL: u64 = 2;
/// 总超时（秒）：服务端上限 2h 音频，留足余量。
const TOTAL_TIMEOUT: u64 = 7_200;

/// 远程服务配置。
#[derive(Debug, Clone)]
pub struct RemoteConfig {
    pub url: String,
    pub token: String,
}

/// 配置加载：环境变量优先，其次数据目录 `remote.json`；都没有 → None。
pub fn load_config(data_dir: &Path) -> Option<RemoteConfig> {
    if let (Ok(url), Ok(token)) = (
        std::env::var("BILI2TEXT_REMOTE_URL"),
        std::env::var("BILI2TEXT_REMOTE_TOKEN"),
    ) {
        if !url.is_empty() && !token.is_empty() {
            return Some(RemoteConfig { url: url.trim_end_matches('/').into(), token });
        }
    }
    let path: PathBuf = data_dir.join("remote.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return None;
    };
    #[derive(Deserialize)]
    struct File {
        url: String,
        token: String,
    }
    let Ok(f) = serde_json::from_str::<File>(&text) else {
        return None;
    };
    (!f.url.is_empty() || !f.token.is_empty()).then(|| RemoteConfig {
        url: f.url.trim_end_matches('/').into(),
        token: f.token,
    })
}

/// healthz 响应（免鉴权）。
#[derive(Debug, Deserialize)]
pub struct Health {
    pub status: String,
    #[serde(default)]
    pub engine: Option<HealthEngine>,
    #[serde(rename = "model_ready", default)]
    pub model_ready: bool,
}

#[derive(Debug, Deserialize)]
pub struct HealthEngine {
    pub name: String,
}

fn remote_err(msg: impl std::fmt::Display) -> AppError {
    AppError::Asr(format!("远程转写失败：{msg}"))
}

/// 远程转写引擎。
pub struct RemoteTranscriber {
    cfg: RemoteConfig,
    http: reqwest::Client,
    /// 引擎名缓存（healthz 时更新），文档「文本来源」用。
    engine_label: std::sync::Mutex<Option<String>>,
}

impl RemoteTranscriber {
    pub fn new(cfg: RemoteConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("reqwest client");
        Self { cfg, http, engine_label: Mutex::new(None) }
    }

    pub fn label(&self) -> String {
        self.engine_label
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| "远程引擎".into())
    }

    /// 服务可达性探测（auto 模式 fallback 判定用）。3 秒超时，失败 = 不可达。
    pub async fn healthz(&self) -> Result<Health, AppError> {
        let resp = self
            .http
            .get(format!("{}/healthz", self.cfg.url))
            .timeout(Duration::from_secs(3))
            .send()
            .await
            .map_err(|_| AppError::Other("远程服务不可达".into()))?;
        let health: Health = resp
            .json()
            .await
            .map_err(|e| remote_err(format!("healthz 响应解析失败：{e}")))?;
        if health.status == "ok" {
            if let Some(e) = &health.engine {
                *self.engine_label.lock().unwrap() =
                    Some(e.name.split_whitespace().next().unwrap_or("远程引擎").to_string());
            }
        }
        Ok(health)
    }

    /// 提交音频，返回 task_id。
    async fn submit(&self, wav: &Path) -> Result<String, AppError> {
        let file_bytes = tokio::fs::read(wav)
            .await
            .map_err(|e| remote_err(format!("读取音频失败：{e}")))?;
        let part = reqwest::multipart::Part::bytes(file_bytes)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| remote_err(format!("组装上传失败：{e}")))?;
        let form = reqwest::multipart::Form::new().part("file", part);

        let resp = self
            .http
            .post(format!("{}/v1/transcripts", self.cfg.url))
            .bearer_auth(&self.cfg.token)
            .multipart(form)
            .send()
            .await
            .map_err(|e| remote_err(format!("提交失败：{e}")))?;

        #[derive(Deserialize)]
        struct Body {
            task_id: String,
        }
        match resp.status() {
            reqwest::StatusCode::OK | reqwest::StatusCode::ACCEPTED => {
                let body: Body = resp.json().await.map_err(|e| remote_err(format!("提交响应解析失败：{e}")))?;
                Ok(body.task_id)
            }
            s if s.as_u16() == 401 => Err(remote_err("鉴权失败（token 无效，检查 remote.json/service.env）")),
            s if s.as_u16() == 413 => Err(remote_err("音频超过服务端大小限制（100MB）")),
            s if s.as_u16() == 415 => Err(remote_err("不支持的音频格式")),
            s if s.as_u16() == 422 => Err(remote_err("音频内容不合法")),
            s if s.as_u16() == 503 => Err(remote_err("模型未就绪（服务端加载中或缺失）")),
            s => Err(remote_err(format!("提交失败：HTTP {s}"))),
        }
    }
}

/// 任务状态响应（轮询体）。
#[derive(Debug, Deserialize)]
struct TaskBody {
    state: String,
    #[serde(default)]
    progress: Option<u32>,
    #[serde(default)]
    result: Option<ResultBody>,
    #[serde(default)]
    error: Option<ErrorBody>,
}

#[derive(Debug, Deserialize)]
struct ResultBody {
    lang: String,
    text: String,
    #[serde(default)]
    segments: Vec<ResultSegment>,
    srt: String,
}

#[derive(Debug, Deserialize)]
struct ResultSegment {
    start: f32,
    end: f32,
    text: String,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    #[serde(default)]
    code: String,
    #[serde(default)]
    message: String,
}

/// 解析轮询响应体 → 引擎无关结果。completed 返回 Some；进行中返回 None；
/// failed 返回错误。契约字段语义在此锁定（单测覆盖）。
fn parse_task_body(text: &str) -> Result<Option<Transcript>, AppError> {
    let body: TaskBody =
        serde_json::from_str(text).map_err(|e| remote_err(format!("任务响应解析失败：{e}")))?;
    match body.state.as_str() {
        "completed" => {
            let r = body.result.ok_or_else(|| remote_err("completed 但缺少 result"))?;
            Ok(Some(Transcript {
                lang: r.lang,
                text: r.text,
                segments: r
                    .segments
                    .into_iter()
                    .map(|s| Segment { start: s.start, end: s.end, text: s.text })
                    .collect(),
                srt: r.srt,
            }))
        }
        "failed" => {
            let e = body.error.unwrap_or(ErrorBody { code: "unknown".into(), message: String::new() });
            Err(remote_err(format!("{}（{}）", e.message, e.code)))
        }
        _ => Ok(None), // queued / processing：继续轮询
    }
}

impl RemoteTranscriber {
    /// 提交 → 轮询 → 取结果（断连不取消：中断后重试 GET 同 task_id 可恢复）。
    pub async fn transcribe(&self, wav: &Path) -> Result<Transcript, AppError> {
        let task_id = self.submit(wav).await?;
        crate::log_step!("[远程] 已提交（task {task_id}），等待转写…");

        let poll_url = format!("{}/v1/transcripts/{task_id}", self.cfg.url);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(TOTAL_TIMEOUT);
        let mut last_progress: u32 = 0;

        loop {
            if tokio::time::Instant::now() > deadline {
                return Err(remote_err(format!(
                    "总超时（{TOTAL_TIMEOUT}s）。任务仍在服务端（task {task_id}），稍后可重新查询"
                )));
            }
            tokio::time::sleep(Duration::from_secs(POLL_INTERVAL)).await;

            let resp = self
                .http
                .get(&poll_url)
                .bearer_auth(&self.cfg.token)
                .send()
                .await
                .map_err(|e| remote_err(format!("轮询失败：{e}（任务不取消，稍后重试）")))?;
            let status = resp.status();
            let text = resp
                .text()
                .await
                .map_err(|e| remote_err(format!("轮询响应读取失败：{e}")))?;
            if status == reqwest::StatusCode::NOT_FOUND {
                return Err(remote_err(format!("任务不存在（task {task_id}，可能已过期）")));
            }

            match parse_task_body(&text)? {
                Some(t) => {
                    crate::log_step!("[远程] 转写完成（{} 段）", t.segments.len());
                    return Ok(t);
                }
                None => {
                    // 进度变化才打印，避免刷屏
                    if let Some(p) = extract_progress(&text) {
                        if p / 10 != last_progress / 10 {
                            crate::log_step!("[远程] 进度 {p}%");
                        }
                        last_progress = p;
                    }
                }
            }
        }
    }
}

/// 从轮询响应里取 progress（独立小函数，便于复用）。
fn extract_progress(text: &str) -> Option<u32> {
    serde_json::from_str::<TaskBody>(text).ok().and_then(|b| b.progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMPLETED: &str = r#"{
        "state": "completed", "progress": 100,
        "result": {
            "lang": "zh",
            "text": "各位网友，大家好。",
            "segments": [{"start": 0.0, "end": 2.5, "text": "各位网友，大家好。"}],
            "srt": "1\n00:00:00,000 --> 00:00:02,500\n各位网友，大家好。\n"
        },
        "timings": {"infer_s": 3.2}
    }"#;

    #[test]
    fn completed_maps_full_contract() {
        let t = parse_task_body(COMPLETED).unwrap().expect("should be done");
        assert_eq!(t.lang, "zh");
        assert_eq!(t.text, "各位网友，大家好。");
        assert_eq!(t.segments.len(), 1);
        assert_eq!(t.segments[0].start, 0.0);
        assert_eq!(t.segments[0].end, 2.5);
        assert!(t.srt.starts_with("1\n00:00:00,000 --> 00:00:02,500"));
    }

    #[test]
    fn queued_and_processing_are_pending() {
        for state in ["queued", "processing"] {
            let body = format!(r#"{{"state":"{state}","progress":42}}"#);
            assert!(parse_task_body(&body).unwrap().is_none(), "{state} should be pending");
        }
    }

    #[test]
    fn failed_maps_error_code() {
        let body = r#"{"state":"failed","error":{"code":"inference_failed","message":"boom"}}"#;
        let err = parse_task_body(body).unwrap_err();
        assert!(err.to_string().contains("boom"), "{err}");
        assert!(err.to_string().contains("inference_failed"), "{err}");
        assert_eq!(err.exit_code(), 50); // 转写引擎错误族
    }

    #[test]
    fn missing_result_on_completed_is_error() {
        let body = r#"{"state":"completed","progress":100}"#;
        assert!(parse_task_body(body).is_err());
    }

    #[test]
    fn load_config_precedence_env_over_file() {
        // env 齐全 → env 优先（不读文件）
        unsafe {
            std::env::set_var("BILI2TEXT_REMOTE_URL", "http://10.0.0.9:1");
            std::env::set_var("BILI2TEXT_REMOTE_TOKEN", "env-token");
        }
        let cfg = load_config(Path::new("/nonexistent-dir-for-test"));
        assert_eq!(cfg.unwrap().url, "http://10.0.0.9:1");
        unsafe {
            std::env::remove_var("BILI2TEXT_REMOTE_URL");
            std::env::remove_var("BILI2TEXT_REMOTE_TOKEN");
        }

        // env 缺失 → 读数据目录 remote.json（尾斜杠修剪）
        let dir = std::env::temp_dir().join(format!("b2t-remote-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("remote.json"),
            r#"{"url": "http://100.64.0.5:8765/", "token": "file-token"}"#,
        )
        .unwrap();
        let cfg = load_config(&dir).unwrap();
        assert_eq!(cfg.url, "http://100.64.0.5:8765");
        assert_eq!(cfg.token, "file-token");
        std::fs::remove_dir_all(dir).ok();

        // env 缺失且文件不存在 → None
        assert!(load_config(Path::new("/nonexistent-dir-for-test")).is_none());
    }
}
