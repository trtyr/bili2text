//! 评论拉取子命令：热门评论落为 Markdown 文档。

use std::path::{Path, PathBuf};

use bili_client::BiliClient;
use chrono::Local;

use crate::error::AppError;
use crate::output::sanitize;

type Result<T> = std::result::Result<T, AppError>;

/// 评论拉取请求（评论是 BV 级的，URL 中的分 P 参数不参与）。
pub struct CommentsReq<'a> {
    pub input: &'a str,
    /// 拉取条数上限（热度序），默认 50。
    pub limit: usize,
    /// 输出路径；缺省 `./<标题>.comments.md`。
    pub output: Option<&'a Path>,
}

pub async fn run(client: &BiliClient, req: &CommentsReq<'_>) -> Result<()> {
    let bili = AppError::from_bili;

    let resolved = client.resolve_video(req.input).await.map_err(bili)?;
    let info = client.video_view(&resolved.bvid).await.map_err(bili)?;
    let page = client
        .hot_comments(info.aid, req.limit)
        .await
        .map_err(bili)?;

    let path = write_comments(&info.title, &info.bvid, &page, req.output)
        .map_err(|e| AppError::Storage(format!("写出评论文档失败：{e}")))?;

    println!(
        "✓ {}（共 {} 条评论，按热度拉取 {} 条）",
        info.title,
        page.total,
        page.comments.len()
    );
    println!("已保存：{}", path.display());
    Ok(())
}

/// 渲染并写出评论 Markdown，返回文档路径。
fn write_comments(
    title: &str,
    bvid: &str,
    page: &bili_client::reply::CommentPage,
    out: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    let md_path = match out {
        Some(p) => p.to_path_buf(),
        None => PathBuf::from(format!("{}.comments.md", sanitize(title))),
    };

    let now = Local::now().format("%Y-%m-%d %H:%M");
    let mut md = format!(
        "# {title} · 评论\n\
         \n\
         | | |\n\
         |---|---|\n\
         | 来源 | https://www.bilibili.com/video/{bvid} |\n\
         | 评论总数 | {total} |\n\
         | 拉取数 | {fetched}（按热度） |\n\
         | 提取时间 | {now} |\n\
         \n\
         ## 评论\n\
         \n",
        title = title,
        bvid = bvid,
        total = page.total,
        fetched = page.comments.len(),
        now = now,
    );

    for (i, c) in page.comments.iter().enumerate() {
        md.push_str(&format_comment(i + 1, c));
    }
    if page.comments.is_empty() {
        md.push_str("（该视频还没有评论。）\n");
    }

    if let Some(dir) = md_path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)?;
        }
    }
    std::fs::write(&md_path, md)?;
    Ok(md_path)
}

/// 渲染一条主楼评论（含楼中楼预览）。
fn format_comment(idx: usize, c: &bili_client::reply::Comment) -> String {
    let date = chrono::DateTime::from_timestamp(c.ctime as i64, 0)
        .map(|d| d.with_timezone(&Local).format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    let mut meta = vec![format!("赞 {}", c.like)];
    if !date.is_empty() {
        meta.push(date);
    }
    if !c.location.is_empty() {
        meta.push(format!("IP{}", c.location));
    }

    let mut out = format!("### {idx}. {}（{}）\n\n{}\n\n", c.uname, meta.join(" · "), c.message);
    for r in &c.replies {
        out.push_str(&format!("> **↳ {}**（赞 {}）：{}\n\n", r.uname, r.like, r.message));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bili_client::reply::{Comment, CommentReply};

    #[test]
    fn format_comment_renders_meta_and_sub_replies() {
        let c = Comment {
            uname: "张三".into(),
            like: 9,
            message: "帮大家整理了资源".into(),
            ctime: 1_760_000_000, // 2025-10-09 (UTC+8: 2025-10-09)
            location: "广东".into(),
            replies: vec![CommentReply {
                uname: "李四".into(),
                like: 1,
                message: "谢谢分享".into(),
            }],
        };
        let md = format_comment(1, &c);
        assert!(md.starts_with("### 1. 张三（赞 9 · "));
        assert!(md.contains("IP广东"));
        assert!(md.contains("帮大家整理了资源"));
        assert!(md.contains("> **↳ 李四**（赞 1）：谢谢分享"));
    }
}
