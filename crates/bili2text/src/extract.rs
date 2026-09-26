//! 字幕提取：拿视频信息 → 选分 P → 挑轨 → 拉字幕内容。

use bili_client::video::{SelectedPage, VideoInfo};
use bili_client::BiliClient;

use crate::error::AppError;
use crate::output::{to_srt, Doc};

type Result<T> = std::result::Result<T, AppError>;

/// 提取请求（输入已在调用方解析为 BV 号）。
pub struct ExtractReq<'a> {
    pub bvid: &'a str,
    /// 指定分 P（CLI --page 优先于 URL ?p=；None = P1）。
    pub page: Option<u64>,
    /// 指定字幕语言（lan）；缺省自动挑选。
    pub lang: Option<&'a str>,
}

pub async fn run(client: &BiliClient, req: &ExtractReq<'_>) -> Result<Doc> {
    // 底层 BiliError 逐变体映射（原文透传），不做二次包装
    let bili = AppError::from_bili;

    let info = client.video_view(req.bvid).await.map_err(bili)?;
    // 分 P 越界是参数问题（退出码 2），不是输入解析失败
    let sel = info.select_page(req.page).map_err(AppError::Usage)?;
    let list = client
        .subtitle_tracks(req.bvid, sel.cid)
        .await
        .map_err(bili)?;

    if list.is_empty() {
        return Err(AppError::NoSubtitle {
            bvid: req.bvid.to_string(),
            title: display_title(&info, &sel),
        });
    }

    let track = match pick_track(&list, req.lang) {
        Some(t) => t.clone(),
        None => {
            return Err(AppError::Usage(format!(
                "指定的字幕语言「{}」不存在；可用语言：{}",
                req.lang.unwrap_or(""),
                list.iter().map(|t| t.lan.clone()).collect::<Vec<_>>().join(", ")
            )));
        }
    };

    let lines = client.subtitle_content(&track.url).await.map_err(bili)?;

    let text = lines
        .iter()
        .map(|l| l.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let source = if track.is_ai {
        format!("AI 字幕（{}）", track.lan)
    } else {
        format!("官方字幕（{}）", track.lan)
    };

    Ok(Doc {
        title: display_title(&info, &sel),
        bvid: req.bvid.to_string(),
        duration_secs: sel.duration_secs,
        lang: track.lan.clone(),
        source,
        lines_count: lines.len(),
        text,
        srt: to_srt(&lines),
    })
}

/// 文档标题：多 P 视频追加 P 号与分标题，避免合集视频张冠李戴。
pub fn display_title(info: &VideoInfo, sel: &SelectedPage) -> String {
    if !sel.is_multi {
        return info.title.clone();
    }
    if sel.part.is_empty() {
        format!("{} P{}", info.title, sel.page)
    } else {
        format!("{} P{}·{}", info.title, sel.page, sel.part)
    }
}

/// 挑选字幕轨：显式 lang > 非 AI 中文 > 非 AI 第一条 > AI 中文 > 第一条。
fn pick_track<'a>(
    tracks: &'a [bili_client::video::SubtitleTrack],
    lang: Option<&str>,
) -> Option<&'a bili_client::video::SubtitleTrack> {
    if let Some(want) = lang {
        return tracks.iter().find(|t| t.lan == want);
    }
    tracks
        .iter()
        .find(|t| !t.is_ai && t.lan.starts_with("zh"))
        .or_else(|| tracks.iter().find(|t| !t.is_ai))
        .or_else(|| tracks.iter().find(|t| t.is_ai && t.lan.starts_with("zh")))
        .or_else(|| tracks.first())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bili_client::video::PageInfo;

    fn mk(lan: &str, ai: bool) -> bili_client::video::SubtitleTrack {
        bili_client::video::SubtitleTrack {
            id: String::new(),
            lan: lan.into(),
            lan_doc: String::new(),
            is_ai: ai,
            url: String::new(),
        }
    }

    #[test]
    fn pick_prefers_native_zh() {
        // 有官方字幕（即使非中文）也不选 AI 字幕
        let tracks = vec![mk("en-US", false), mk("ai-zh", true)];
        assert_eq!(pick_track(&tracks, None).unwrap().lan, "en-US");

        // 中文官方 > 英文官方 > AI 中文 > AI 任意
        let tracks = vec![mk("ai-zh", true), mk("zh-Hans", false)];
        assert_eq!(pick_track(&tracks, None).unwrap().lan, "zh-Hans");

        let tracks = vec![mk("ai-zh", true)];
        assert_eq!(pick_track(&tracks, None).unwrap().lan, "ai-zh");

        // 显式指定语言优先
        let tracks = vec![mk("zh-Hans", false), mk("en-US", false)];
        assert_eq!(pick_track(&tracks, Some("en-US")).unwrap().lan, "en-US");
    }

    #[test]
    fn display_title_marks_multi_page() {
        let info = VideoInfo {
            bvid: "BV1TEST".into(),
            title: "课程合集".into(),
            cid: 111,
            duration_secs: 960,
            pages: vec![
                PageInfo { page: 1, cid: 111, part: "第一P".into(), duration_secs: 60 },
                PageInfo { page: 2, cid: 222, part: "第二P".into(), duration_secs: 900 },
            ],
        };
        let sel = info.select_page(Some(2)).unwrap();
        assert_eq!(display_title(&info, &sel), "课程合集 P2·第二P");

        // 单 P 不加标记
        let single = VideoInfo {
            bvid: "BV1TEST".into(),
            title: "普通视频".into(),
            cid: 111,
            duration_secs: 60,
            pages: vec![PageInfo { page: 1, cid: 111, part: "第一P".into(), duration_secs: 60 }],
        };
        let sel = single.select_page(None).unwrap();
        assert_eq!(display_title(&single, &sel), "普通视频");
    }
}
