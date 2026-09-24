//! 日志：详细日志追加写入全局数据目录 `logs/bili2text.log`；
//! stderr 只输出人类可读的进度（`log_step!`），详细事件（参数、结果、错误）
//! 只进文件。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static LOG_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

/// 初始化日志文件（数据目录下 logs/bili2text.log，追加写）。
/// 目录不可创建时降级为仅 stderr（不写文件）。
pub fn init(data_dir: &Path) {
    let dir = data_dir.join("logs");
    let path = dir.join("bili2text.log");
    let enabled = std::fs::create_dir_all(&dir).is_ok();
    let _ = LOG_PATH.set(enabled.then_some(path));
}

/// 追加一条日志到文件（不影响 stdout/stderr）。
pub fn append(level: &str, msg: &str) {
    let Some(Some(path)) = LOG_PATH.get() else {
        return;
    };
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{ts} [{level}] {msg}");
    }
}

/// 进度信息：stderr + 文件。
#[macro_export]
macro_rules! log_step {
    ($($arg:tt)*) => {{
        eprintln!($($arg)*);
        $crate::log::append("STEP", &format!($($arg)*));
    }};
}

/// 详细事件：仅文件。
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => { $crate::log::append("INFO", &format!($($arg)*)) };
}

/// 警告：stderr + 文件（非致命，如历史写入失败）。
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {{
        eprintln!("警告：{}", format!($($arg)*));
        $crate::log::append("WARN", &format!($($arg)*));
    }};
}

/// 错误：仅文件（终端输出由 main 统一处理，含退出码）。
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => { $crate::log::append("ERROR", &format!($($arg)*)) };
}
