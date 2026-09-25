//! 环境体检：检查依赖与数据就绪度，`--fix` 自动修复可修复项。
//!
//! 退出码约定：0 = 全部就绪；3 = 存在未就绪项（fix 后仍未就绪 / 未跑 fix）。
//! yt-dlp / ffmpeg / SenseVoice 模型三项仅转写构建（transcribe feature）检查。

use std::path::{Path, PathBuf};

use bili_client::BiliClient;

use crate::error::AppError;
#[cfg(feature = "transcribe")]
use crate::transcribe::MODEL_SUBDIR;

#[cfg(feature = "transcribe")]
const MODEL_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17.tar.bz2";

/// 单项检查结果。
struct Finding {
    name: &'static str,
    ok: bool,
    detail: String,
}

impl Finding {
    fn ok(name: &'static str, detail: impl Into<String>) -> Self {
        Self { name, ok: true, detail: detail.into() }
    }
    fn bad(name: &'static str, detail: impl Into<String>) -> Self {
        Self { name, ok: false, detail: detail.into() }
    }
    fn render(&self) -> String {
        let mark = if self.ok { "✓" } else { "✗" };
        // 中文字符按 2 列宽计，名称列对齐到 10 列
        let w: usize = self
            .name
            .chars()
            .map(|c| if c as u32 > 0x2E80 { 2 } else { 1 })
            .sum();
        let pad = " ".repeat(10usize.saturating_sub(w));
        format!("{mark} {}{pad}  {}", self.name, self.detail)
    }
}

/// 执行体检；`fix = true` 时尝试自动修复（模型下载 / brew 安装，后者询问确认）。
/// 返回进程退出码（0 就绪 / 3 有缺项）。
pub async fn run(client: &BiliClient, data_dir: &Path, fix: bool) -> Result<i32, AppError> {
    let mut f = collect(client, data_dir).await;

    if fix && f.iter().any(|x| !x.ok) {
        println!();
        println!("尝试自动修复：");
        #[cfg(feature = "transcribe")]
        fix_model(data_dir, &mut f).await;
        fix_downloaders(&mut f).await;
        // 修复后重新体检一次（真实状态为准）
        f = collect(client, data_dir).await;
    }

    println!();
    println!("bili2text 环境体检：");
    for x in &f {
        println!("  {}", x.render());
    }

    let pending = f.iter().filter(|x| !x.ok).count();
    if pending == 0 {
        println!();
        println!("✓ 环境就绪，可以开转。");
        Ok(0)
    } else {
        println!();
        println!("✗ {pending} 项未就绪。{}", if fix {
            "自动修复未能全部解决，按上面提示手动处理。".to_string()
        } else {
            "运行 `bili2text doctor --fix` 自动修复（模型下载 / 安装 yt-dlp、ffmpeg）。".to_string()
        });
        Ok(3)
    }
}

/// 全量体检项。
async fn collect(client: &BiliClient, data_dir: &Path) -> Vec<Finding> {
    let mut f = Vec::new();

    // 1. 数据目录（顺手确保存在 + 可写）
    let writable = std::fs::create_dir_all(data_dir).is_ok()
        && std::fs::write(data_dir.join(".doctor-probe"), b"ok").is_ok();
    let _ = std::fs::remove_file(data_dir.join(".doctor-probe"));
    f.push(if writable {
        Finding::ok("数据目录", format!("{}（可写）", data_dir.display()))
    } else {
        Finding::bad("数据目录", format!("{}（不可创建或不可写）", data_dir.display()))
    });

    // 2. 登录态（在线校验有效性）
    let has_cred = client.credential().header_value().is_some();
    f.push(match has_cred {
        false => Finding::bad(
            "登录态",
            "未登录 —— AI 字幕与高音质下载需要（bili2text login）",
        ),
        true if client.is_logged_in().await => Finding::ok("登录态", "已登录（有效）"),
        true => Finding::bad("登录态", "本机有存档但已失效（bili2text login 重新扫码）"),
    });

    // 3/4/5. 外部依赖与模型（仅转写构建需要）
    #[cfg(feature = "transcribe")]
    {
        f.push(match which("yt-dlp") {
            Some(p) => Finding::ok("yt-dlp", p.display().to_string()),
            None => Finding::bad("yt-dlp", "未安装 —— 本地转写需要"),
        });
        f.push(match which("ffmpeg") {
            Some(p) => Finding::ok("ffmpeg", p.display().to_string()),
            None => Finding::bad("ffmpeg", "未安装 —— 本地转写需要"),
        });

        let model = model_file(data_dir);
        f.push(if model.exists() {
            let size = model_dir_size(data_dir);
            Finding::ok("SenseVoice 模型", format!("{}（{size}）", model.display()))
        } else {
            Finding::bad("SenseVoice 模型", "未下载 —— 本地转写需要（约 230MB 压缩包）")
        });
    }

    f
}

/// --fix：模型缺失时自动下载解压（目标在数据目录内，无需确认）。
#[cfg(feature = "transcribe")]
async fn fix_model(data_dir: &Path, f: &[Finding]) {
    if model_file(data_dir).exists() || !f.iter().any(|x| x.name == "SenseVoice 模型" && !x.ok) {
        return;
    }
    println!("  · 下载 SenseVoice 模型（压缩包约 230MB）…");
    let models_dir = data_dir.join("models");
    if std::fs::create_dir_all(&models_dir).is_err() {
        println!("    ✗ 无法创建目录 {}", models_dir.display());
        return;
    }
    let archive = models_dir.join("bili2text-model.tar.bz2");
    let ok = tokio::process::Command::new("curl")
        .args([
            "-L",
            "--fail",
            "--retry",
            "3",
            "--retry-all-errors",
            "--retry-delay",
            "2",
            "-C",
            "-",
            "--progress-bar",
            "-o",
        ])
        .arg(&archive)
        .arg(MODEL_URL)
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
        && tokio::process::Command::new("tar")
            .args(["xjf"])
            .arg(&archive)
            .arg("-C")
            .arg(&models_dir)
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);
    let _ = std::fs::remove_file(&archive);
    println!("    {}", if ok { "✓ 完成" } else { "✗ 下载或解压失败（网络需代理时请先设置 HTTPS_PROXY，再重跑 doctor --fix）" });
}

/// --fix：yt-dlp / ffmpeg 缺失时，检测到 Homebrew 则询问后安装。
async fn fix_downloaders(f: &[Finding]) {
    let missing: Vec<&str> = ["yt-dlp", "ffmpeg"]
        .into_iter()
        .filter(|bin| which(bin).is_none() && f.iter().any(|x| x.name == *bin && !x.ok))
        .collect();
    if missing.is_empty() {
        return;
    }
    let Some(brew) = which("brew") else {
        println!(
            "  · 未检测到 Homebrew，请手动安装：{}",
            missing.iter().map(|m| format!("brew install {m}")).collect::<Vec<_>>().join(" && ")
        );
        return;
    };
    println!("  · 检测到 Homebrew，安装 {}？[Y/n] ", missing.join(" "));
    if !confirm().unwrap_or(true) {
        println!("    跳过（稍后可手动：brew install {}）", missing.join(" "));
        return;
    }
    let ok = tokio::process::Command::new(brew)
        .arg("install")
        .args(&missing)
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false);
    println!("    {}", if ok { "✓ 完成" } else { "✗ brew install 失败，请手动处理" });
}

fn confirm() -> Option<bool> {
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).ok()?;
    let t = line.trim().to_lowercase();
    Some(t.is_empty() || t == "y" || t == "yes")
}

/// 模型主文件路径。
#[cfg(feature = "transcribe")]
fn model_file(data_dir: &Path) -> PathBuf {
    data_dir.join("models").join(MODEL_SUBDIR).join("model.int8.onnx")
}

/// 模型目录人类可读大小。
#[cfg(feature = "transcribe")]
fn model_dir_size(data_dir: &Path) -> String {
    let dir = data_dir.join("models").join(MODEL_SUBDIR);
    let mut total: u64 = 0;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            if let Ok(meta) = e.metadata() {
                total += meta.len();
            }
        }
    }
    format!("{:.1}GB", total as f64 / 1024.0 / 1024.0 / 1024.0)
}

/// 极简 which：沿 PATH 找可执行文件。
pub fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|p| p.join(bin))
        .find(|p| p.is_file())
}
