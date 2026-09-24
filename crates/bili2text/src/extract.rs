//! 字幕提取：解析输入 → 拿视频信息 → 挑轨 → 拉字幕内容。

use bili_client::BiliClient;

use crate::error::AppError;
use crate::output::{to_srt, Doc};

type Result<T> = std::result::Result<T, AppError>;

/// 提取请求。
pub struct ExtractReq<'a> {
    pub input: &'a str,
    /// 指定字幕语言（lan）；缺省自动挑选。
    pub lang: Option<&'a str>,
}

pub async fn run(client: &BiliClient, req: &ExtractReq<'_>) -> Result<Doc> {
    // 底层 BiliError 逐变体映射（原文透传），不做二次包装
    let bili = AppError::from_bili;

    let bvid = client.resolve_bvid(req.input).await.map_err(bili)?;
    let info = client.video_view(&bvid).await.map_err(bili)?;
    let list = client
        .subtitle_tracks(&bvid, info.cid)
        .await
        .map_err(bili)?;

    if list.is_empty() {
        return Err(AppError::NoSubtitle {
            bvid,
            title: info.title,
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
        title: info.title,
        bvid,
        duration_secs: info.duration_secs,
        lang: track.lan.clone(),
        source,
        lines_count: lines.len(),
        text,
        srt: to_srt(&lines),
    })
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
}
