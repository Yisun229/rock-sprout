//! 用户设置。翻译与商店地区两组，存在数据目录的 `settings.json` 里。
//!
//! 单独的模块（不是塞进 `config.rs`）是因为 `config.rs` 管的是「导出/导入配置」那种
//! 带版本号的文件格式，而这里是程序自己的偏好，两者的生命周期不一样。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::translate::{DEFAULT_ENDPOINT, DEFAULT_TARGET};

/// 程序设置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// 翻译服务地址（默认 Google 那个免密钥的接口）
    #[serde(default = "default_endpoint")]
    pub translate_endpoint: String,
    /// 翻成哪种语言
    #[serde(default = "default_target")]
    pub translate_target: String,
    /// 查 Steam 价格用的国家/地区代码（两个字母，如 `CN`、`US`）
    #[serde(default = "default_store_country")]
    pub store_country: String,
    /// Steam Web API key。
    ///
    /// **这里的值永远不落盘**：真 key 存在 `<数据目录>\steam-key.sealed`（DPAPI 封装），
    /// `skip_serializing` 保证 `settings.json` 里连空字段都不会出现，免得有人把它当真的。
    #[serde(default, skip_serializing)]
    pub steam_key: String,
    /// 我的 SteamID64（17 位数字）。公开标识，可以明文存。
    #[serde(default)]
    pub steam_id: String,
}

fn default_endpoint() -> String {
    DEFAULT_ENDPOINT.to_owned()
}

fn default_target() -> String {
    DEFAULT_TARGET.to_owned()
}

fn default_store_country() -> String {
    "CN".to_owned()
}

/// SteamID64 只认 17 位数字；别的一律当空。
pub fn normalize_steam_id(raw: &str) -> String {
    let text = raw.trim();
    if text.len() == 17 && text.chars().all(|c| c.is_ascii_digit()) {
        text.to_owned()
    } else {
        String::new()
    }
}

/// 国家代码只认两个 ASCII 字母，别的（含空）一律回 `CN`。
///
/// 不在这里校验「是不是真的存在这个区」——那是 Steam 的事，写错了两字母也只是
/// 查回来没有价格，而不是把设置存坏。
pub fn normalize_country(raw: &str) -> String {
    let up = raw.trim().to_ascii_uppercase();
    if up.len() == 2 && up.chars().all(|c| c.is_ascii_alphabetic()) {
        up
    } else {
        default_store_country()
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            translate_endpoint: default_endpoint(),
            translate_target: default_target(),
            store_country: default_store_country(),
            steam_key: String::new(),
            steam_id: String::new(),
        }
    }
}

impl Settings {
    /// 空字段回落到默认值：手改坏了文件也不至于翻不了。
    pub fn normalized(mut self) -> Self {
        if self.translate_endpoint.trim().is_empty() {
            self.translate_endpoint = default_endpoint();
        }
        if self.translate_target.trim().is_empty() {
            self.translate_target = default_target();
        }
        self.store_country = normalize_country(&self.store_country);
        // key 只在内存里由 DPAPI 那份读出来；这里清空，避免任何路径把它写进 json
        self.steam_key.clear();
        self.steam_id = normalize_steam_id(&self.steam_id);
        self
    }
}

/// 配置文件路径：`<数据目录>/settings.json`
pub fn file_at(root: &Path) -> PathBuf {
    root.join("settings.json")
}

/// 读设置。文件不存在、读不动、内容不合法，全都回默认值（不报错）。
pub fn load_at(root: &Path) -> Settings {
    let path = file_at(root);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Settings::default();
    };
    match serde_json::from_str::<Settings>(&raw) {
        Ok(s) => s.normalized(),
        Err(e) => {
            tracing::warn!(%e, "settings.json 读不懂，按默认值来");
            Settings::default()
        }
    }
}

/// 写设置。
pub fn save_at(root: &Path, settings: &Settings) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(root)?;
    let path = file_at(root);
    let body = serde_json::to_string_pretty(settings)?;
    std::fs::write(&path, body)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translate::hash16;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rs-set-{}", hash16(tag)));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn 没文件时给默认值() {
        let dir = tmp("no-file");
        let s = load_at(&dir);
        assert_eq!(s.translate_target, DEFAULT_TARGET);
        assert!(s.translate_endpoint.starts_with("https://"));
    }

    #[test]
    fn 存了再读一模一样() {
        let dir = tmp("round-trip");
        let s = Settings {
            translate_endpoint: "https://example.com/t?q=".to_owned(),
            translate_target: "ja".to_owned(),
            store_country: "US".to_owned(),
            steam_key: String::new(),
            steam_id: "76561199027062409".to_owned(),
        };
        save_at(&dir, &s).unwrap();
        assert_eq!(load_at(&dir), s);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn key_不会写进_settings_json() {
        let dir = tmp("steam-key-not-plain");
        let mut s = Settings::default();
        s.steam_key = "0123456789ABCDEF0123456789ABCDEF".to_owned();
        save_at(&dir, &s).unwrap();
        let raw = std::fs::read_to_string(file_at(&dir)).unwrap();
        assert!(!raw.contains("steam_key"), "settings.json 里不该出现 steam_key：{raw}");
        assert!(!raw.contains("0123456789ABCDEF"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn steam_id_只收_17_位数字() {
        assert_eq!(normalize_steam_id(" 76561199027062409 "), "76561199027062409");
        assert_eq!(normalize_steam_id("7656119902706240"), "");
        assert_eq!(normalize_steam_id("7656119902706240a"), "");
    }

    #[test]
    fn 空字段回落默认值() {
        let dir = tmp("empty-fields");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(file_at(&dir), r#"{"translate_target":"  "}"#).unwrap();
        let s = load_at(&dir);
        assert_eq!(s.translate_target, DEFAULT_TARGET);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 国家代码只收两个字母_其余回_cn() {
        assert_eq!(normalize_country(" us "), "US");
        assert_eq!(normalize_country("cn"), "CN");
        assert_eq!(normalize_country(""), "CN");
        assert_eq!(normalize_country("CHN"), "CN");
        assert_eq!(normalize_country("中"), "CN");
        let s = Settings {
            store_country: " ".to_owned(),
            ..Settings::default()
        }
        .normalized();
        assert_eq!(s.store_country, "CN");
    }

    #[test]
    fn 文件坏了按默认值() {
        let dir = tmp("broken-json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(file_at(&dir), "not json").unwrap();
        assert_eq!(load_at(&dir), Settings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
