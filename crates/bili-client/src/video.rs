//! 视频信息与字幕拉取。
//!
//! - `GET /x/web-interface/view?bvid=` → 基本信息 + 分 P（首个 cid）
//! - `GET /x/player/wbi/v2`（需 wbi 签名 + 登录态）→ 字幕列表
//!   （CC 与 AI 字幕混排；**未登录 subtitles 为空数组**）
//! - 字幕本体：`aisubtitle.hdslb.com` 的 JSON（`body[{from,to,content}]`）

use serde::Deserialize;

use crate::{BiliClient, BiliError, Result};

const API_BASE: &str = "https://api.bilibili.com";

#[derive(Debug, Clone)]
pub struct VideoInfo {
    pub bvid: String,
    pub title: String,
    /// 第一个分 P 的 cid（多 P 支持在后续任务扩展）。
    pub cid: u64,
    pub duration_secs: u64,
    pub pages: Vec<PageInfo>,
}

#[derive(Debug, Clone)]
pub struct PageInfo {
    pub cid: u64,
    pub part: String,
    pub duration_secs: u64,
}

/// 一条字幕轨。AI 字幕的 lan 以 "ai" 开头（如 ai-zh）。
#[derive(Debug, Clone)]
pub struct SubtitleTrack {
    pub id: String,
    pub lan: String,
    pub lan_doc: String,
    pub is_ai: bool,
    /// 字幕 JSON 下载地址（协议相对 // → 补 https:）。
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct SubtitleLine {
    pub from: f64,
    pub to: f64,
    pub content: String,
}

#[derive(Deserialize)]
struct Envelope {
    code: i64,
    #[serde(default)]
    message: String,
    #[serde(default)]
    data: serde_json::Value,
}

fn api_err(env: &Envelope) -> BiliError {
    BiliError::Api {
        code: env.code,
        message: env.message.clone(),
    }
}

/// 视频信息（公开接口，无需登录）。
pub async fn video_view(client: &BiliClient, bvid: &str) -> Result<VideoInfo> {
    let env: Envelope = client
        .http()
        .get(format!("{API_BASE}/x/web-interface/view"))
        .query(&[("bvid", bvid)])
        .header("Referer", "https://www.bilibili.com/")
        .send()
        .await?
        .json()
        .await?;
    if env.code != 0 {
        return Err(api_err(&env));
    }

    let d = &env.data;
    let get_u64 = |key: &str| d.get(key).and_then(|v| v.as_u64()).unwrap_or(0);
    let get_str = |key: &str| {
        d.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };

    let pages: Vec<PageInfo> = d
        .get("pages")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    Some(PageInfo {
                        cid: p.get("cid")?.as_u64()?,
                        part: p
                            .get("part")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        duration_secs: p
                            .get("duration")
                            .and_then(|v| v.as_u64())
                            .unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(VideoInfo {
        bvid: get_str("bvid"),
        title: get_str("title"),
        cid: get_u64("cid"),
        duration_secs: get_u64("duration"),
        pages,
    })
}

/// 字幕列表（wbi 签名 + 登录态；未登录返回空列表由调用方决定语义）。
pub async fn subtitle_tracks(
    client: &BiliClient,
    bvid: &str,
    cid: u64,
) -> Result<Vec<SubtitleTrack>> {
    let signed = client
        .wbi()
        .signed_query(&[
            ("bvid".to_string(), bvid.to_string()),
            ("cid".to_string(), cid.to_string()),
        ])
        .await?;

    let env: Envelope = client
        .authed_get(&format!("{API_BASE}/x/player/wbi/v2?bvid={bvid}&cid={cid}&{signed}"))
        .send()
        .await?
        .json()
        .await?;
    if env.code != 0 {
        return Err(api_err(&env));
    }

    let subtitles = env
        .data
        .get("subtitle")
        .and_then(|s| s.get("subtitles"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut tracks = Vec::new();
    for s in &subtitles {
        let lan = s
            .get("lan")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let raw_url = s
            .get("subtitle_url")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if raw_url.is_empty() {
            continue;
        }
        let url = if raw_url.starts_with("//") {
            format!("https:{raw_url}")
        } else {
            raw_url.to_string()
        };
        tracks.push(SubtitleTrack {
            id: s
                .get("id_str")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            is_ai: lan.starts_with("ai"),
            lan_doc: s
                .get("lan_doc")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            lan,
            url,
        });
    }
    Ok(tracks)
}

/// 下载字幕 JSON 并解析为时间轴行。
pub async fn subtitle_content(client: &BiliClient, url: &str) -> Result<Vec<SubtitleLine>> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        body: Vec<RawLine>,
    }
    #[derive(Deserialize)]
    struct RawLine {
        from: f64,
        to: f64,
        content: String,
    }

    let resp = client.http().get(url).send().await?;
    if !resp.status().is_success() {
        return Err(BiliError::Api {
            code: resp.status().as_u16() as i64,
            message: "字幕资源下载失败".into(),
        });
    }
    let body: Body = resp.json().await?;
    Ok(body
        .body
        .into_iter()
        .map(|l| SubtitleLine {
            from: l.from,
            to: l.to,
            content: l.content,
        })
        .collect())
}

/// 供单元测试与调用方复用的 wbi 签名入口（当前未直接暴露）。
pub(crate) fn _wbi_marker() {}
