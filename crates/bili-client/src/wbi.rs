//! WBI 参数签名：`x/player/wbi/v2` 等接口的公共参数防伪。
//!
//! 流程（bilibili-API-collect · docs/misc/sign/wbi.md）：
//! 1. `GET /x/web-interface/nav` 取 `data.wbi_img.img_url/sub_url` 的文件名
//!    （去扩展名）得 img_key + sub_key；
//! 2. 按 `MIXIN_KEY_ENC_TAB` 重排拼接结果、截前 32 字节 = mixin_key；
//! 3. 请求参数加 `wts`（秒级时间戳），按 key 升序、value 做 encodeURIComponent
//!    式编码（大写 hex、空格为 %20、过滤 `!'()*`），拼上 mixin_key 取 MD5 = w_rid；
//! 4. 请求参数附带 wts 与 w_rid。
//!
//! 内置官方测试向量做单元测试，防止重排表抄错。

use std::sync::Mutex;
use std::time::{Duration, Instant};

use md5::{Digest, Md5};

use crate::{API_BASE, BiliError, Result};

/// 官方重排映射表（长 64）。
const MIXIN_KEY_ENC_TAB: [usize; 64] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49, 33, 9, 42, 19, 29,
    28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0, 1, 60, 51, 30, 4, 22, 25,
    54, 21, 56, 59, 6, 63, 57, 62, 11, 36, 20, 34, 44, 52,
];

/// 对 img_key + sub_key 重排取前 32 位。
pub fn gen_mixin_key(raw_wbi_key: &str) -> String {
    let raw = raw_wbi_key.as_bytes();
    MIXIN_KEY_ENC_TAB
        .iter()
        .filter_map(|&i| raw.get(i).copied())
        .take(32)
        .map(|b| b as char)
        .collect()
}

/// encodeURIComponent 语义编码：保留 `A-Za-z0-9-_.~`，空格为 %20，hex 大写。
fn encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 对参数（键值对）计算 wbi 签名查询串附加值：返回 (wts, w_rid)。
pub fn sign_params(params: &[(String, String)], mixin_key: &str, wts: i64) -> String {
    let mut sorted: Vec<(String, String)> = params.to_vec();
    sorted.push(("wts".into(), wts.to_string()));
    sorted.sort_by(|a, b| a.0.cmp(&b.0));

    let query: Vec<String> = sorted
        .iter()
        .map(|(k, v)| {
            // 官方口径：过滤值中的 !'()* 字符
            let filtered: String = v.chars().filter(|c| !"!'()*".contains(*c)).collect();
            format!("{}={}", encode_component(k), encode_component(&filtered))
        })
        .collect();

    let base = format!("{}{}", query.join("&"), mixin_key);
    let digest = Md5::digest(base.as_bytes());
    let w_rid: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("w_rid={w_rid}&wts={wts}")
}

#[derive(Debug, Clone)]
struct Keys {
    mixin: String,
    fetched_at: Instant,
}

/// WBI key 获取与缓存（1 小时刷新）。
#[derive(Debug)]
pub struct WbiKeys {
    cached: Mutex<Option<Keys>>,
}

impl Default for WbiKeys {
    fn default() -> Self {
        Self::new()
    }
}

impl WbiKeys {
    pub fn new() -> Self {
        Self { cached: Mutex::new(None) }
    }

    fn extract_keys(img_url: &str, sub_url: &str) -> Option<String> {
        let file_name = |url: &str| {
            let name = url.rsplit('/').next()?;
            let name = name.split('?').next()?;
            Some(name.trim_end_matches(".png").to_string())
        };
        let img = file_name(img_url)?;
        let sub = file_name(sub_url)?;
        Some(gen_mixin_key(&format!("{img}{sub}")))
    }

    /// 拿 mixin key（惰性拉取 + 缓存）。`fetch` 传入实际的 nav 请求实现，便于测试注入。
    async fn mixin_key(&self) -> Result<String> {
        {
            let cached = self.cached.lock().unwrap();
            if let Some(k) = cached.as_ref() {
                if k.fetched_at.elapsed() < Duration::from_secs(3600) {
                    return Ok(k.mixin.clone());
                }
            }
        }

        #[derive(serde::Deserialize)]
        struct Nav {
            #[serde(default)]
            data: NavData,
        }
        #[derive(serde::Deserialize, Default)]
        struct NavData {
            #[serde(default)]
            wbi_img: WbiImg,
        }
        #[derive(serde::Deserialize, Default)]
        struct WbiImg {
            #[serde(default)]
            img_url: String,
            #[serde(default)]
            sub_url: String,
        }

        let http = reqwest::Client::builder()
            .user_agent(crate::USER_AGENT)
            .build()?;
        let nav: Nav = http
            .get(format!("{API_BASE}/x/web-interface/nav"))
            .send()
            .await?
            .json()
            .await?;

        let mixin = Self::extract_keys(&nav.data.wbi_img.img_url, &nav.data.wbi_img.sub_url)
            .ok_or(BiliError::Api {
                code: -1,
                message: "nav 接口未返回 wbi key".into(),
            })?;

        *self.cached.lock().unwrap() = Some(Keys {
            mixin: mixin.clone(),
            fetched_at: Instant::now(),
        });
        Ok(mixin)
    }

    /// 为 wbi 接口的查询参数附加 w_rid 与 wts（追加到 URL 查询串末尾）。
    pub async fn signed_query(&self, params: &[(String, String)]) -> Result<String> {
        let mixin = self.mixin_key().await?;
        let wts = chrono_secs();
        Ok(sign_params(params, &mixin, wts))
    }
}

fn chrono_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 官方测试向量（wbi.md）
    const IMG: &str = "7cd084941338484aae1ad9425b84077c";
    const SUB: &str = "4932caff0ff746eab6f01bf08b70ac45";

    #[test]
    fn mixin_key_matches_official_vector() {
        assert_eq!(
            gen_mixin_key(&format!("{IMG}{SUB}")),
            "ea1db124af3c7062474693fa704f4ff8"
        );
    }

    #[test]
    fn w_rid_matches_official_vector() {
        let params = vec![
            ("foo".to_string(), "114".to_string()),
            ("bar".to_string(), "514".to_string()),
            ("zab".to_string(), "1919810".to_string()),
        ];
        let mixin = "ea1db124af3c7062474693fa704f4ff8";
        let signed = sign_params(&params, mixin, 1702204169);
        assert_eq!(
            signed,
            "w_rid=8f6f2b5b3d485fe1886cec6a0be8c5d4&wts=1702204169"
        );
    }

    #[test]
    fn encode_component_matches_ecmascript() {
        assert_eq!(encode_component("one one four"), "one%20one%20four");
        assert_eq!(encode_component("五一四"), "%E4%BA%94%E4%B8%80%E5%9B%9B");
        assert_eq!(encode_component("a-b_c.d~e"), "a-b_c.d~e");
        assert_eq!(encode_component("!*'()"), "%21%2A%27%28%29");
    }

    #[test]
    fn extract_keys_from_nav_urls() {
        let mixin = WbiKeys::extract_keys(
            "https://i0.hdslb.com/bfs/wbi/7cd084941338484aae1ad9425b84077c.png",
            "https://i0.hdslb.com/bfs/wbi/4932caff0ff746eab6f01bf08b70ac45.png",
        )
        .unwrap();
        assert_eq!(mixin, "ea1db124af3c7062474693fa704f4ff8");
    }
}
