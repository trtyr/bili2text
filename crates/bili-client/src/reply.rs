//! 视频评论拉取。
//!
//! - `GET /x/v2/reply/main?type=1&oid={aid}&mode=3`（热度序）→ 主楼 + cursor 翻页
//! - 主楼对象内嵌 replies（楼中楼预览，通常前 3 条）
//! - 公开接口，无需登录态

use serde::Deserialize;

use crate::{BiliClient, BiliError, Result};

const API_BASE: &str = "https://api.bilibili.com";

/// 一条主楼评论（楼中楼在 [`Comment::replies`]）。
#[derive(Debug, Clone)]
pub struct Comment {
    pub uname: String,
    pub like: u64,
    pub message: String,
    /// 发布时间戳（秒）。
    pub ctime: u64,
    /// IP 属地（如「广东」），缺失为空。
    pub location: String,
    pub replies: Vec<CommentReply>,
}

/// 一条楼中楼回复。
#[derive(Debug, Clone)]
pub struct CommentReply {
    pub uname: String,
    pub like: u64,
    pub message: String,
}

/// 一次拉取的结果：评论列表 + 视频评论总数。
#[derive(Debug, Clone)]
pub struct CommentPage {
    pub comments: Vec<Comment>,
    /// 视频评论总数（接口返回的 all_count，含楼中楼）。
    pub total: u64,
}

#[derive(Deserialize)]
struct Envelope {
    code: i64,
    #[serde(default)]
    message: String,
    #[serde(default)]
    data: serde_json::Value,
}

#[derive(Deserialize)]
struct ReplyItem {
    #[serde(default)]
    like: u64,
    #[serde(default)]
    ctime: u64,
    #[serde(default)]
    member: Member,
    #[serde(default)]
    content: Content,
    #[serde(default)]
    replies: Option<Vec<ReplyItem>>,
}

#[derive(Deserialize, Default)]
struct Member {
    #[serde(default)]
    uname: String,
}

#[derive(Deserialize, Default)]
struct Content {
    #[serde(default)]
    message: String,
    #[serde(default)]
    reply_control: Option<ReplyControl>,
}

#[derive(Deserialize)]
struct ReplyControl {
    #[serde(default)]
    location: Option<String>,
}

/// 按热度拉取评论（mode=3），cursor 翻页直到 `limit` 条或拉完。
pub async fn hot_comments(client: &BiliClient, aid: u64, limit: usize) -> Result<CommentPage> {
    let mut all: Vec<Comment> = Vec::new();
    let mut total = 0u64;
    let mut next: Option<u64> = None; // None = 第一页

    loop {
        let mut url = format!("{API_BASE}/x/v2/reply/main?type=1&oid={aid}&mode=3");
        if let Some(n) = next {
            url.push_str(&format!("&next={n}"));
        }
        let env: Envelope = client.authed_get(&url).send().await?.json().await?;
        if env.code != 0 {
            return Err(BiliError::Api { code: env.code, message: env.message });
        }

        let (page, page_next, is_end) = parse_page(&env.data);
        total = env
            .data
            .pointer("/cursor/all_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(total);
        all.extend(page);
        match (page_next, is_end) {
            (Some(n), false) if all.len() < limit => next = Some(n),
            _ => break,
        }
    }

    all.truncate(limit);
    Ok(CommentPage { comments: all, total })
}

/// 解析单页：主楼列表 + 下一页 cursor + 是否结束。
fn parse_page(data: &serde_json::Value) -> (Vec<Comment>, Option<u64>, bool) {
    let is_end = data.pointer("/cursor/is_end").and_then(|v| v.as_bool()).unwrap_or(true);
    let next = data.pointer("/cursor/next").and_then(|v| v.as_u64());
    let replies = data.get("replies").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    let comments = replies
        .iter()
        .filter_map(|r| serde_json::from_value::<ReplyItem>(r.clone()).ok())
        .map(|r| Comment {
            uname: r.member.uname,
            like: r.like,
            message: r.content.message,
            ctime: r.ctime,
            location: r
                .content
                .reply_control
                .and_then(|c| c.location)
                .and_then(|l| l.strip_prefix("IP属地：").map(str::to_string))
                .unwrap_or_default(),
            replies: r
                .replies
                .unwrap_or_default()
                .into_iter()
                .map(|s| CommentReply {
                    uname: s.member.uname,
                    like: s.like,
                    message: s.content.message,
                })
                .collect(),
        })
        .collect();

    (comments, next, is_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_page_extracts_fields() {
        let data: serde_json::Value = serde_json::json!({
            "cursor": { "is_end": false, "next": 2, "all_count": 187 },
            "replies": [
                {
                    "like": 9,
                    "ctime": 1_760_000_000u64,
                    "member": { "uname": "张三" },
                    "content": {
                        "message": "帮大家整理了资源",
                        "reply_control": { "location": "IP属地：广东" }
                    },
                    "replies": [
                        { "like": 1, "member": { "uname": "李四" }, "content": { "message": "谢谢分享" } }
                    ]
                },
                { "like": 0, "member": { "uname": "王五" }, "content": { "message": "讲的挺好" } }
            ]
        });

        let (comments, next, is_end) = parse_page(&data);
        assert_eq!(comments.len(), 2);
        assert_eq!(next, Some(2));
        assert!(!is_end);

        let c = &comments[0];
        assert_eq!(c.uname, "张三");
        assert_eq!(c.like, 9);
        assert_eq!(c.location, "广东");
        assert_eq!(c.replies.len(), 1);
        assert_eq!(c.replies[0].uname, "李四");

        // 无属地 / 无楼中楼的评论正常缺省
        assert_eq!(comments[1].location, "");
        assert!(comments[1].replies.is_empty());
    }

    #[test]
    fn parse_page_handles_empty() {
        let (comments, next, is_end) = parse_page(&serde_json::json!({}));
        assert!(comments.is_empty());
        assert_eq!(next, None);
        assert!(is_end); // 缺 cursor 视为结束，防御死循环
    }
}
