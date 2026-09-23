//! 登录态：扫码成功后的 cookie 集合与文件持久化。

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// 登录态 = 扫码成功后 B 站下发的 cookie 集合
/// （关键字段：SESSDATA / bili_jct / DedeUserID）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Credential {
    #[serde(default, flatten)]
    cookies: BTreeMap<String, String>,
}

impl Credential {
    pub fn from_pairs(pairs: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            cookies: pairs.into_iter().collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.cookies.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.cookies.get(name).map(|s| s.as_str())
    }

    /// 组装 Cookie 请求头值；无 SESSDATA 视为未登录，返回 None。
    pub fn header_value(&self) -> Option<String> {
        let sessdata = self.cookies.get("SESSDATA")?;
        if sessdata.is_empty() {
            return None;
        }
        let pairs: Vec<String> = self
            .cookies
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        Some(pairs.join("; "))
    }

    /// 落盘（JSON）。父目录不存在则自动创建。
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
    }

    /// 从磁盘加载；文件不存在返回默认空登录态。
    pub fn load(path: &Path) -> serde_json::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(json) => serde_json::from_str(&json),
            Err(_) => Ok(Self::default()),
        }
    }

    /// 导出为 Netscape cookie 文件格式（yt-dlp / curl 可直接使用）。
    /// domain 固定 .bilibili.com。未登录（无 SESSDATA）返回 None。
    pub fn to_netscape_file(&self, path: &Path) -> std::io::Result<Option<String>> {
        let Some(header_value) = self.header_value() else {
            return Ok(None);
        };
        let mut out = String::from("# Netscape HTTP Cookie File\n");
        for (name, value) in &self.cookies {
            if value.is_empty() {
                continue;
            }
            out.push_str(&format!(
                ".bilibili.com\tTRUE\t/\tTRUE\t0\t{name}\t{value}\n"
            ));
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, out)?;
        let _ = header_value;
        Ok(Some(path.display().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_value_requires_sessdata() {
        let empty = Credential::default();
        assert!(empty.header_value().is_none());

        let cred = Credential::from_pairs([
            ("SESSDATA".to_string(), "abc%2Cdef".to_string()),
            ("bili_jct".to_string(), "xyz".to_string()),
        ]);
        let header = cred.header_value().unwrap();
        assert!(header.contains("SESSDATA=abc%2Cdef"));
        assert!(header.contains("bili_jct=xyz"));
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("toolbox-test-{}", std::process::id()));
        let path = dir.join("credential.json");
        let cred = Credential::from_pairs([("SESSDATA".to_string(), "s".to_string())]);
        cred.save(&path).unwrap();
        let loaded = Credential::load(&path).unwrap();
        assert_eq!(loaded.get("SESSDATA"), Some("s"));
        std::fs::remove_dir_all(dir).ok();
    }
}
