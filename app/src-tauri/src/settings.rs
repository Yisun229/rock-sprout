//! 用户设置。目前只有翻译这一组，存在数据目录的 `settings.json` 里。
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
}

fn default_endpoint() -> String {
    DEFAULT_ENDPOINT.to_owned()
}

fn default_target() -> String {
    DEFAULT_TARGET.to_owned()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            translate_endpoint: default_endpoint(),
            translate_target: default_target(),
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
        };
        save_at(&dir, &s).unwrap();
        assert_eq!(load_at(&dir), s);
        let _ = std::fs::remove_dir_all(&dir);
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
    fn 文件坏了按默认值() {
        let dir = tmp("broken-json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(file_at(&dir), "not json").unwrap();
        assert_eq!(load_at(&dir), Settings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
