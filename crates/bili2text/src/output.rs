//! 结果输出：Markdown 文档渲染与 SRT 导出。

use std::path::{Path, PathBuf};

use chrono::Local;

/// 待写入文档的统一结果模型（字幕提取与本地转写共用）。
pub struct Doc {
    pub title: String,
    pub bvid: String,
    pub duration_secs: u64,
    /// 原始语言码（字幕 lan 或转写语言标记，如 zh-CN / ai-zh / zh）。
    pub lang: String,
    /// 文本来源描述，如「官方字幕（zh-Hans）」「本地转写（SenseVoice, zh）」。
    pub source: String,
    pub lines_count: usize,
    pub text: String,
    pub srt: String,
}

/// 写出 Markdown 文档；`out` 缺省为 `./<标题>.md`。
/// `with_srt` 时在同目录导出同名 .srt。返回文档路径。
pub fn write_doc(doc: &Doc, out: Option<&Path>, with_srt: bool) -> anyhow::Result<PathBuf> {
    let md_path = match out {
        Some(p) => p.to_path_buf(),
        None => PathBuf::from(format!("{}.md", sanitize(&doc.title))),
    };

    let now = Local::now().format("%Y-%m-%d %H:%M");
    let mins = doc.duration_secs / 60;
    let secs = doc.duration_secs % 60;
    let markdown = format!(
        "# {title}\n\
        \n\
        | | |\n\
        |---|---|\n\
        | 来源 | https://www.bilibili.com/video/{bvid} |\n\
        | 时长 | {mins}分{secs}秒 |\n\
        | 文本来源 | {source} |\n\
        | 段落数 | {lines} |\n\
        | 提取时间 | {now} |\n\
        \n\
        ## 正文\n\
        \n\
        {text}\n",
        title = doc.title,
        bvid = doc.bvid,
        mins = mins,
        secs = secs,
        source = doc.source,
        lines = doc.lines_count,
        now = now,
        text = doc.text,
    );
    if let Some(dir) = md_path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)?;
        }
    }
    std::fs::write(&md_path, markdown)?;

    if with_srt {
        let srt_path = md_path.with_extension("srt");
        std::fs::write(&srt_path, &doc.srt)?;
    }
    Ok(md_path)
}

/// 文件名净化：路径分隔符与非法字符替换为 `_`。
pub fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "untitled".into()
    } else {
        trimmed.to_string()
    }
}

/// 字幕行转 SRT 格式（00:00:00,000 --> ...）。
pub fn to_srt(lines: &[bili_client::video::SubtitleLine]) -> String {
    fn stamp(secs: f64) -> String {
        let total_ms = (secs.max(0.0) * 1000.0).round() as u64;
        let h = total_ms / 3_600_000;
        let m = (total_ms % 3_600_000) / 60_000;
        let s = (total_ms % 60_000) / 1000;
        let ms = total_ms % 1000;
        format!("{h:02}:{m:02}:{s:02},{ms:03}")
    }
    lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            format!(
                "{}\n{} --> {}\n{}\n",
                i + 1,
                stamp(l.from),
                stamp(l.to),
                l.content
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srt_format() {
        let lines = vec![
            bili_client::video::SubtitleLine {
                from: 0.0,
                to: 1.5,
                content: "你好".into(),
            },
            bili_client::video::SubtitleLine {
                from: 62.25,
                to: 65.0,
                content: "世界".into(),
            },
        ];
        let srt = to_srt(&lines);
        assert!(srt.starts_with("1\n00:00:00,000 --> 00:00:01,500\n你好\n"));
        assert!(srt.contains("2\n00:01:02,250 --> 00:01:05,000\n世界"));
    }

    #[test]
    fn sanitize_replaces_path_chars() {
        assert_eq!(sanitize("a/b:c*d"), "a_b_c_d");
        assert_eq!(sanitize("  "), "untitled");
        assert_eq!(sanitize("正常标题"), "正常标题");
    }

    #[test]
    fn doc_writes_md_and_srt() {
        let dir = std::env::temp_dir().join(format!("b2t-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let doc = Doc {
            title: "测试".into(),
            bvid: "BV1x".into(),
            duration_secs: 65,
            lang: "zh-CN".into(),
            source: "官方字幕".into(),
            lines_count: 1,
            text: "你好".into(),
            srt: "1\n00:00:00,000 --> 00:00:01,000\n你好\n".into(),
        };
        let md = write_doc(&doc, Some(&dir.join("t.md")), true).unwrap();
        assert!(md.exists());
        assert!(md.with_extension("srt").exists());
        let content = std::fs::read_to_string(&md).unwrap();
        assert!(content.contains("# 测试"));
        assert!(content.contains("| 文本来源 | 官方字幕 |"));
        assert!(content.contains("1分5秒"));
        std::fs::remove_dir_all(dir).ok();
    }
}
