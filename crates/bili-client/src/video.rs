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
    /// av 号（评论等以 aid 为键的接口使用）。
    pub aid: u64,
    pub title: String,
    /// 默认分 P 的 cid（= P1；多 P 时即 pages[0].cid）。
    pub cid: u64,
    /// 全部分 P 的总时长（合集语义；单 P 即该 P 时长）。
    pub duration_secs: u64,
    pub pages: Vec<PageInfo>,
}

#[derive(Debug, Clone)]
pub struct PageInfo {
    /// 分 P 序号（从 1 起，与 URL `?p=N` 对应）。
    pub page: u64,
    pub cid: u64,
    pub part: String,
    pub duration_secs: u64,
}

/// 选定的分 P（字幕 / 转写都以它为准，避免合集视频静默拿错 P）。
#[derive(Debug, Clone)]
pub struct SelectedPage {
    pub page: u64,
    pub cid: u64,
    pub part: String,
    pub duration_secs: u64,
    /// 视频是否多 P（决定标题是否需要带 P 标记）。
    pub is_multi: bool,
}

impl VideoInfo {
    /// 选定分 P：`n = None` 跟随 B 站网页语义取 P1；带 `?p=N` 或显式指定时取对应 P。
    /// 越界返回人读错误消息（调用方按参数问题处理）。
    pub fn select_page(&self, n: Option<u64>) -> std::result::Result<SelectedPage, String> {
        let n = n.unwrap_or(1);
        let pages = if self.pages.is_empty() {
            // 理论上 view 接口总有 pages；防御：退回默认 cid
            return Ok(SelectedPage {
                page: 1,
                cid: self.cid,
                part: String::new(),
                duration_secs: self.duration_secs,
                is_multi: false,
            });
        } else {
            &self.pages
        };
        let p = pages.iter().find(|p| p.page == n).ok_or_else(|| {
            format!("分 P 号 {n} 超出范围：该视频共 {} 个分 P（1-{}）", pages.len(), pages.len())
        })?;
        Ok(SelectedPage {
            page: p.page,
            cid: p.cid,
            part: p.part.clone(),
            duration_secs: p.duration_secs,
            is_multi: pages.len() > 1,
        })
    }
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
                        page: p.get("page")?.as_u64()?,
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
        aid: get_u64("aid"),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn info(pages: &[(u64, &str, u64, u64)]) -> VideoInfo {
        VideoInfo {
            bvid: "BV1TEST".into(),
            aid: 42,
            title: "总标题".into(),
            cid: pages.first().map(|p| p.2).unwrap_or(0),
            duration_secs: pages.iter().map(|p| p.3).sum(),
            pages: pages
                .iter()
                .map(|&(page, part, cid, dur)| PageInfo {
                    page,
                    cid,
                    part: part.into(),
                    duration_secs: dur,
                })
                .collect(),
        }
    }

    #[test]
    fn select_page_defaults_and_ranges() {
        let multi = info(&[
            (1, "第一P", 111, 60),
            (3, "第三P", 333, 900),
            (7, "第七P", 777, 1200),
        ]);

        // 缺省 = P1（网页语义）
        let s = multi.select_page(None).unwrap();
        assert_eq!((s.page, s.cid), (1, 111));
        assert!(s.is_multi);

        // 指定 P3：cid / part / 时长都以该 P 为准
        let s = multi.select_page(Some(3)).unwrap();
        assert_eq!((s.page, s.cid, s.part.as_str()), (3, 333, "第三P"));
        assert_eq!(s.duration_secs, 900);

        // 超范围报错
        assert!(multi.select_page(Some(2)).is_err());
        assert!(multi.select_page(Some(99)).is_err());

        // 单 P 视频不是 multi
        let single = info(&[(1, "唯一P", 111, 60)]);
        let s = single.select_page(Some(1)).unwrap();
        assert!(!s.is_multi);
        assert_eq!(s.duration_secs, 60);
    }
}
