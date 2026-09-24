//! bili2text — B 站视频转文字 CLI。
//!
//! 传入视频链接自动提取字幕（官方优先、AI 次之），无字幕视频可加
//! `--transcribe` 用本地 SenseVoice 模型转写；结果存为 Markdown 文档，
//! `--srt` 可同时导出字幕文件。数据（登录态/历史/模型/日志）在
//! ~/.local/share/bili2text/。
//!
//! 退出码见 [`error::AppError`] 文档（按错误类别区分）。

mod error;
mod extract;
mod log;
mod login;
mod output;
mod tasks;
mod transcribe;

use std::path::PathBuf;
use std::time::Instant;

use clap::{Parser, Subcommand, ValueEnum};
use downloader::AudioDownloader;

use crate::error::AppError;
use crate::extract::ExtractReq;
use crate::tasks::{TaskRecord, TaskStore};
use crate::transcribe::AsrSlot;

#[derive(Parser)]
#[command(
    name = "bili2text",
    version,
    about = "B 站视频转文字：提取字幕或本地转写，结果存为本地文档"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Command>,

    /// BV 号、视频页链接或 b23.tv 短链
    input: Option<String>,

    /// 指定字幕语言（lan，如 zh-Hans / ai-zh）；缺省自动挑选
    #[arg(short, long)]
    lang: Option<String>,

    /// 跳过字幕，直接用本地 SenseVoice 模型转写（耗时较长）
    #[arg(short, long)]
    transcribe: bool,

    /// 同时导出 .srt 字幕文件（与文档同目录同名）
    #[arg(long)]
    srt: bool,

    /// 输出文档路径（缺省：当前目录/<标题>.md）
    #[arg(short, long)]
    output: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Command {
    /// 扫码登录 B 站（AI 字幕与高音质音频需要）
    Login,
    /// 清除本机登录态
    Logout,
    /// 查看登录态
    Status,
    /// 历史记录（list / show <id> / rm <id>；id 支持前缀）
    History {
        #[arg(default_value = "list")]
        action: HistoryAction,
        /// 记录 id
        id: Option<String>,
    },
}

#[derive(ValueEnum, Clone, Copy)]
enum HistoryAction {
    List,
    Show,
    Rm,
}

#[tokio::main]
async fn main() {
    let start = Instant::now();
    let cli = Cli::parse();

    // 日志先行：数据目录不可用时降级为仅 stderr（run 内会给出明确错误）
    if let Ok(dir) = data_dir() {
        log::init(&dir);
    }
    log_info!(
        "start argv={:?} cwd={}",
        std::env::args().collect::<Vec<_>>(),
        std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default()
    );

    if let Err(e) = run(cli).await {
        let code = e.exit_code();
        log_error!("exit {code}: {e}");
        eprintln!("错误[{code}]：{e}");
        std::process::exit(code);
    }
    log_info!("done in {:.1}s", start.elapsed().as_secs_f32());
}

async fn run(cli: Cli) -> Result<(), AppError> {
    let data_dir = data_dir()?;
    let client = bili_client::BiliClient::new()
        .map_err(AppError::from_bili)?
        .with_store(data_dir.join("credential.json"));
    let store =
        TaskStore::open(data_dir.join("tasks.db")).map_err(|e| AppError::Storage(e.to_string()))?;

    match cli.cmd {
        Some(Command::Login) => login::login(&client).await,
        Some(Command::Logout) => login::logout(&client).await,
        Some(Command::Status) => login::status(&client).await,
        Some(Command::History { action, id }) => history(&store, action, id.as_deref()),
        None => {
            let input = cli.input.as_deref().ok_or_else(|| {
                AppError::Usage(
                    "缺少视频输入。用法：bili2text <BV号|链接|短链>，或 bili2text <子命令>".into(),
                )
            })?;
            convert(&client, &store, &data_dir, input, &cli).await
        }
    }
}

/// 主流程：字幕提取或本地转写 → 落历史 → 写文档。
async fn convert(
    client: &bili_client::BiliClient,
    store: &TaskStore,
    data_dir: &std::path::Path,
    input: &str,
    cli: &Cli,
) -> Result<(), AppError> {
    log_info!(
        "convert input={input} mode={} lang={:?} out={:?} srt={}",
        if cli.transcribe { "transcribe" } else { "subtitle" },
        cli.lang,
        cli.output,
        cli.srt,
    );

    let doc = if cli.transcribe {
        transcribe_flow(client, store, data_dir, input).await?
    } else {
        match extract::run(client, &ExtractReq { input, lang: cli.lang.as_deref() }).await {
            Ok(doc) => {
                // 成功也落历史（尽力而为，失败不影响结果）
                if let Err(err) = store.record(TaskRecord {
                    id: String::new(),
                    kind: "subtitle_extract".into(),
                    input: input.into(),
                    title: Some(doc.title.clone()),
                    bvid: Some(doc.bvid.clone()),
                    status: "succeeded".into(),
                    stage: None,
                    progress: Some(100),
                    error: None,
                    lang: Some(doc.lang.clone()),
                    lines_count: Some(doc.lines_count as i64),
                    duration_secs: Some(doc.duration_secs as i64),
                    result_text: Some(doc.text.clone()),
                    result_srt: Some(doc.srt.clone()),
                    created_at: 0,
                    finished_at: None,
                }) {
                    log_warn!("历史写入失败（不影响本次结果）：{err}");
                }
                doc
            }
            // 无字幕是业务性失败：落历史后原样返回（退出码 31）
            Err(e @ AppError::NoSubtitle { .. }) => {
                let (bvid, title) = match &e {
                    AppError::NoSubtitle { bvid, title } => (bvid.clone(), title.clone()),
                    _ => unreachable!(),
                };
                if let Err(err) = store.record(TaskRecord {
                    id: String::new(),
                    kind: "subtitle_extract".into(),
                    input: input.into(),
                    title: Some(title),
                    bvid: Some(bvid),
                    status: "failed".into(),
                    stage: None,
                    progress: None,
                    error: Some("no_subtitle".into()),
                    lang: cli.lang.clone(),
                    lines_count: None,
                    duration_secs: None,
                    result_text: None,
                    result_srt: None,
                    created_at: 0,
                    finished_at: None,
                }) {
                    log_warn!("历史写入失败（不影响本次结果）：{err}");
                }
                return Err(e);
            }
            Err(e) => return Err(e),
        }
    };

    let path = output::write_doc(&doc, cli.output.as_deref(), cli.srt)
        .map_err(|e| AppError::Storage(format!("写出结果文档失败：{e}")))?;
    println!("✓ {}（{} · {} 段）", doc.title, doc.source, doc.lines_count);
    println!("已保存：{}", path.display());
    if cli.srt {
        println!("已保存：{}", path.with_extension("srt").display());
    }
    Ok(())
}

/// 本地转写流程：先落 running 历史，完成后回写终态。
async fn transcribe_flow(
    client: &bili_client::BiliClient,
    store: &TaskStore,
    data_dir: &std::path::Path,
    input: &str,
) -> Result<output::Doc, AppError> {
    let bvid = client.resolve_bvid(input).await.map_err(AppError::from_bili)?;
    let info = client.video_view(&bvid).await.map_err(AppError::from_bili)?;

    let task_id = store
        .record(TaskRecord {
            id: String::new(),
            kind: "asr_transcribe".into(),
            input: input.into(),
            title: Some(info.title.clone()),
            bvid: Some(bvid.clone()),
            status: "running".into(),
            stage: Some("downloading".into()),
            progress: Some(10),
            error: None,
            lang: None,
            lines_count: None,
            duration_secs: Some(info.duration_secs as i64),
            result_text: None,
            result_srt: None,
            created_at: 0,
            finished_at: None,
        })
        .map_err(|e| AppError::Storage(e.to_string()))?;

    let downloader = AudioDownloader::new(data_dir.join("audio"));
    let asr_slot = std::sync::Arc::new(AsrSlot::with_model_dir(
        data_dir.join("models").join(transcribe::MODEL_SUBDIR),
    ));
    let result = transcribe::run(
        client,
        &downloader,
        &asr_slot,
        store,
        &task_id,
        &transcribe::TranscribeReq {
            bvid: &bvid,
            title: &info.title,
            duration_secs: info.duration_secs,
            data_dir,
        },
    )
    .await;

    match result {
        Ok(doc) => {
            if let Err(e) = store.finish(
                &task_id,
                "succeeded",
                None,
                None,
                Some(doc.lines_count as i64),
                Some(doc.duration_secs as i64),
                Some(&doc.text),
                Some(&doc.srt),
            ) {
                log_warn!("历史回写失败（不影响本次结果）：{e}");
            }
            Ok(doc)
        }
        Err(e) => {
            if let Err(err) = store.finish(
                &task_id,
                "failed",
                Some(&format!("{e}")),
                None,
                None,
                None,
                None,
                None,
            ) {
                log_warn!("历史回写失败：{err}");
            }
            Err(e)
        }
    }
}

/// 历史记录子命令。
fn history(store: &TaskStore, action: HistoryAction, id: Option<&str>) -> Result<(), AppError> {
    let storage = |e: rusqlite::Error| AppError::Storage(e.to_string());
    match action {
        HistoryAction::List => {
            let items = store.list(200).map_err(storage)?;
            if items.is_empty() {
                println!("还没有历史记录。");
                return Ok(());
            }
            println!("{:<10}  {:<16}  {:<3}  {}", "id", "时间", "状态", "标题 / 说明");
            for t in items {
                let time = chrono::DateTime::from_timestamp(t.created_at, 0)
                    .map(|d| {
                        d.with_timezone(&chrono::Local)
                            .format("%m-%d %H:%M")
                            .to_string()
                    })
                    .unwrap_or_default();
                let mark = match t.status.as_str() {
                    "succeeded" => "✓",
                    "failed" => "✗",
                    _ => "…",
                };
                let note = match (&t.error, t.lines_count) {
                    (Some(e), _) => format!("失败：{e}"),
                    (None, Some(n)) => format!("{n} 段"),
                    _ => String::new(),
                };
                println!(
                    "{:<10}  {:<16}  {:<3}  {}{}",
                    &t.id[..8.min(t.id.len())],
                    time,
                    mark,
                    t.title.as_deref().unwrap_or(&t.input),
                    if note.is_empty() {
                        String::new()
                    } else {
                        format!("（{note}）")
                    },
                );
            }
        }
        HistoryAction::Show { .. } => {
            let task = resolve_task(store, id)?;
            let time = chrono::DateTime::from_timestamp(task.created_at, 0)
                .map(|d| {
                    d.with_timezone(&chrono::Local)
                        .format("%Y-%m-%d %H:%M")
                        .to_string()
                })
                .unwrap_or_default();
            println!("标题：{}", task.title.as_deref().unwrap_or("(无)"));
            println!("BV：{}", task.bvid.as_deref().unwrap_or("-"));
            println!(
                "来源：{} · {}",
                task.kind,
                task.lang.as_deref().unwrap_or("-")
            );
            println!("时间：{time} · 状态：{}", task.status);
            println!();
            match &task.result_text {
                Some(text) => println!("{text}"),
                None => println!(
                    "（该记录没有正文，可能当时失败了：{}）",
                    task.error.as_deref().unwrap_or("-")
                ),
            }
        }
        HistoryAction::Rm => {
            let task = resolve_task(store, id)?;
            store.delete(&task.id).map_err(storage)?;
            println!("✓ 已删除：{}", task.title.as_deref().unwrap_or(&task.id));
        }
    }
    Ok(())
}

/// 按 id（支持前缀）定位任务记录。
fn resolve_task(store: &TaskStore, id: Option<&str>) -> Result<TaskRecord, AppError> {
    let id = id.ok_or_else(|| {
        AppError::Usage("需要提供记录 id（bili2text history 查看列表）".into())
    })?;
    let storage = |e: rusqlite::Error| AppError::Storage(e.to_string());
    if let Some(t) = store.get(id).map_err(storage)? {
        return Ok(t);
    }
    // 前缀匹配
    let items = store.list(200).map_err(storage)?;
    let hits: Vec<_> = items.iter().filter(|t| t.id.starts_with(id)).collect();
    match hits.len() {
        0 => Err(AppError::Usage(format!("找不到记录 {id}"))),
        1 => store
            .get(&hits[0].id)
            .map_err(storage)?
            .ok_or_else(|| AppError::Usage(format!("找不到记录 {id}"))),
        n => Err(AppError::Usage(format!(
            "id 前缀「{id}」匹配到 {n} 条记录，请给更长前缀"
        ))),
    }
}

/// 数据目录：~/.local/share/bili2text（登录态 / 历史库 / 模型 / 日志 / 临时音频）。
fn data_dir() -> Result<PathBuf, AppError> {
    let home = std::env::var("HOME")
        .map_err(|_| AppError::Storage("无法确定 HOME 目录".into()))?;
    Ok(PathBuf::from(home).join(".local/share/bili2text"))
}
