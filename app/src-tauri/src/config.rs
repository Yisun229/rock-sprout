//! 配置导出 / 导入：把「这台机器上装了哪些 mod」写成一个 json，换机器时搬过去。
//!
//! 只搬「装了哪些」，不搬文件本身（mod 体积大，而且本地导入的那些本来就在用户自己手里）。
//! **令牌永远不进这个文件**：这里所有函数的参数里都没有令牌这一项，想漏也漏不进去。
//!
//! 文件长这样：
//!
//! ```json
//! {
//!   "kind": "rock-sprout-config",
//!   "schema": 1,
//!   "exported_at": 1761600000000,
//!   "app_version": "0.1.0",
//!   "mods": [ { "name": "Brighter Objects", "source": "modio",
//!               "url": "https://mod.io/g/drg/m/1179441", "filename": "brighterobjects1123-fyin.zip" } ],
//!   "settings": {}
//! }
//! ```

use std::path::Path;

use serde::{Deserialize, Serialize};

/// 用来认「这是我们的配置文件」，读别人的 json 时先看这个。
pub const KIND: &str = "rock-sprout-config";
/// 文件格式版本。以后字段变了就加一，读的时候好判断。
pub const SCHEMA: u32 = 1;

/// 导出的一个 mod。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportedMod {
    pub name: String,
    /// `modio` 或 `local`
    pub source: String,
    /// mod.io 的页面地址；本地导入的没有，为 `null`
    #[serde(default)]
    pub url: Option<String>,
    /// 本地那份文件名（换机器后能对上「是不是同一个文件」）
    #[serde(default)]
    pub filename: String,
}

impl ExportedMod {
    /// 用来判断「是不是同一个 mod」的键。
    ///
    /// mod.io 的用地址（id 稳定）；本地导入的没有地址，用名字+文件名。
    pub fn key(&self) -> String {
        match &self.url {
            Some(url) => url.clone(),
            None => format!("local:{}:{}", self.name, self.filename),
        }
    }
}

/// 设置。v0.1 里除了令牌没有别的设置，而令牌不导出，所以这里是空的。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {}

/// 整份配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub kind: String,
    pub schema: u32,
    pub exported_at: u64,
    pub app_version: String,
    pub mods: Vec<ExportedMod>,
    #[serde(default)]
    pub settings: Settings,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Config {
    /// 按当前已装列表拼一份新配置。
    pub fn build(mods: Vec<ExportedMod>, app_version: &str) -> Self {
        Self {
            kind: KIND.to_owned(),
            schema: SCHEMA,
            exported_at: now_ms(),
            app_version: app_version.to_owned(),
            mods,
            settings: Settings::default(),
        }
    }

    /// 写成 `config-YYYYMMDD.json` 这种名字，返回真正落盘的路径。
    pub fn write_to_dir(&self, dir: &Path) -> Result<std::path::PathBuf, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("建目录失败：{e}"))?;
        let path = dir.join(format!("config-{}.json", date_stamp(self.exported_at)));
        let text = serde_json::to_string_pretty(self).map_err(|e| format!("拼 json 失败：{e}"))?;
        std::fs::write(&path, text).map_err(|e| format!("写文件失败：{e}"))?;
        Ok(path)
    }

    /// 从文件读回来，并检查是不是我们的文件。
    pub fn read_from_file(path: &Path) -> Result<Self, String> {
        let text =
            std::fs::read_to_string(path).map_err(|e| format!("读不了这个文件：{e}"))?;
        let config: Self =
            serde_json::from_str(&text).map_err(|e| format!("不像是一份配置文件：{e}"))?;
        if config.kind != KIND {
            return Err(format!(
                "这个文件不是岩笋的配置（里面的类型写的是「{}」）",
                config.kind
            ));
        }
        if config.schema > SCHEMA {
            return Err(format!(
                "这份配置是更新版本的程序导出的（格式 {}，这个版本只认到 {}），请先升级",
                config.schema, SCHEMA
            ));
        }
        Ok(config)
    }
}

/// 两份清单的差别，给用户看「这台机器上要多装什么、多出来什么」。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Diff {
    /// 配置里有、本机没有 → 需要去装
    pub only_in_file: Vec<String>,
    /// 本机有、配置里没有 → 这台机器上多出来的
    pub only_local: Vec<String>,
    /// 两边都有
    pub same: Vec<String>,
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.only_in_file.is_empty() && self.only_local.is_empty()
    }

    /// 给界面用的一句话总结。
    pub fn summary(&self) -> String {
        if self.is_empty() {
            return format!("两边一致，都是 {} 个 mod。", self.same.len());
        }
        let mut parts = Vec::new();
        if !self.only_in_file.is_empty() {
            parts.push(format!(
                "配置里多出 {} 个（本机还没有）：{}",
                self.only_in_file.len(),
                self.only_in_file.join("、")
            ));
        }
        if !self.only_local.is_empty() {
            parts.push(format!(
                "本机多出 {} 个（配置里没有）：{}",
                self.only_local.len(),
                self.only_local.join("、")
            ));
        }
        parts.push(format!("两边都有 {}", self.same.len()));
        parts.join("；")
    }
}

/// 比较两个列表。顺序不影响结果。
pub fn diff(current: &[ExportedMod], incoming: &[ExportedMod]) -> Diff {
    let current_keys: Vec<String> = current.iter().map(|m| m.key()).collect();
    let incoming_keys: Vec<String> = incoming.iter().map(|m| m.key()).collect();

    let mut out = Diff::default();
    for m in incoming {
        if current_keys.contains(&m.key()) {
            out.same.push(m.name.clone());
        } else {
            out.only_in_file.push(m.name.clone());
        }
    }
    for m in current {
        if !incoming_keys.contains(&m.key()) {
            out.only_local.push(m.name.clone());
        }
    }
    out
}

/// unix 毫秒 → `20261028`（**本机时区**的年月日）。
///
/// 用本机时区而不是 UTC：用户看到文件名时对的是自己墙上的日期。
/// 取不到本机时区（少数环境不允许）就退回 UTC，只是文件名差一天，不影响内容。
fn date_stamp(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    let stamp = time::OffsetDateTime::from_unix_timestamp(secs)
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH)
        .to_offset(offset);
    format!(
        "{:04}{:02}{:02}",
        stamp.year(),
        u8::from(stamp.month()),
        stamp.day()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let base = std::env::var("CARGO_TARGET_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("target"));
        let dir = base.join("rock-sprout-tests").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建测试目录");
        dir
    }

    fn sample_mods() -> Vec<ExportedMod> {
        vec![
            ExportedMod {
                name: "Brighter Objects".to_owned(),
                source: "modio".to_owned(),
                url: Some("https://mod.io/g/drg/m/1179441".to_owned()),
                filename: "brighterobjects1123-fyin.zip".to_owned(),
            },
            ExportedMod {
                name: "我自己改的".to_owned(),
                source: "local".to_owned(),
                url: None,
                filename: "我自己改的-321054.pak".to_owned(),
            },
        ]
    }

    #[test]
    fn 导出的文件里绝不会出现令牌() {
        let config = Config::build(sample_mods(), "0.1.0");
        let text = serde_json::to_string(&config).expect("拼 json");

        let token = "ghp_THIS_IS_A_SECRET_TOKEN_1234567890";
        assert!(!text.contains(token), "令牌不该出现在导出内容里");
        // 也不该出现「令牌」这类字段名，避免以后有人手滑加进去
        assert!(!text.contains("token"), "导出内容里不该有 token 字段：{text}");
        assert!(!text.contains("modio-token"), "连文件名都不该带");
    }

    #[test]
    fn 写出来再读回来内容一样() {
        let dir = temp_dir("config-roundtrip");
        let config = Config::build(sample_mods(), "0.1.0");
        let path = config.write_to_dir(&dir).expect("写配置");
        assert!(path.exists(), "文件要真的在");
        assert_eq!(
            path.file_name().unwrap().to_string_lossy().starts_with("config-"),
            true,
            "文件名要是 config-YYYYMMDD.json：{}",
            path.display()
        );

        let back = Config::read_from_file(&path).expect("读配置");
        assert_eq!(back.mods, config.mods);
        assert_eq!(back.kind, KIND);
        assert_eq!(back.schema, SCHEMA);
    }

    #[test]
    fn 别人的json不会被当成我们的配置() {
        let dir = temp_dir("config-not-ours");
        let path = dir.join("config-20261028.json");
        std::fs::write(&path, r#"{"kind":"something-else","schema":1,"exported_at":0,"app_version":"0.1.0","mods":[]}"#)
            .expect("写别人的 json");

        let err = Config::read_from_file(&path).expect_err("应该拒绝");
        assert!(err.contains("不是岩笋的配置"), "{err}");
    }

    #[test]
    fn 更新版本导出的配置会提示先升级() {
        let dir = temp_dir("config-too-new");
        let path = dir.join("config-20261028.json");
        std::fs::write(
            &path,
            format!(r#"{{"kind":"{KIND}","schema":99,"exported_at":0,"app_version":"9.9.9","mods":[]}}"#),
        )
        .expect("写配置");

        let err = Config::read_from_file(&path).expect_err("应该拒绝");
        assert!(err.contains("更新版本"), "{err}");
    }

    #[test]
    fn 两份清单一致时说两边一致() {
        let a = sample_mods();
        let d = diff(&a, &a.clone());
        assert!(d.is_empty());
        assert_eq!(d.same.len(), 2);
        assert!(d.summary().contains("两边一致"));
    }

    #[test]
    fn 能说出多了什么少了什么() {
        let current = sample_mods();
        let mut incoming = sample_mods();
        incoming.remove(1); // 配置里没有「我自己改的」
        incoming.push(ExportedMod {
            name: "Deep Dive 信息".to_owned(),
            source: "modio".to_owned(),
            url: Some("https://mod.io/g/drg/m/123".to_owned()),
            filename: "dd.zip".to_owned(),
        });

        let d = diff(&current, &incoming);
        assert_eq!(d.only_in_file, vec!["Deep Dive 信息"]);
        assert_eq!(d.only_local, vec!["我自己改的"]);
        assert_eq!(d.same, vec!["Brighter Objects"]);

        let text = d.summary();
        assert!(text.contains("配置里多出 1 个"), "{text}");
        assert!(text.contains("本机多出 1 个"), "{text}");
    }

    #[test]
    fn 同名但来源不同的mod不会被当成同一个() {
        let a = vec![ExportedMod {
            name: "同一个名字".to_owned(),
            source: "modio".to_owned(),
            url: Some("https://mod.io/g/drg/m/1".to_owned()),
            filename: "a.zip".to_owned(),
        }];
        let b = vec![ExportedMod {
            name: "同一个名字".to_owned(),
            source: "local".to_owned(),
            url: None,
            filename: "a.zip".to_owned(),
        }];

        let d = diff(&a, &b);
        assert_eq!(d.only_in_file, vec!["同一个名字"]);
        assert_eq!(d.only_local, vec!["同一个名字"]);
        assert!(d.same.is_empty());
    }

    #[test]
    fn 文件名里的日期是八位的年月日() {
        // 2026-10-28 12:00 UTC：无论本机时区是哪一档，年月日都在 2026 年 10 月里
        let stamp = date_stamp(1_793_188_800_000);
        assert_eq!(stamp.len(), 8, "要是 8 位：{stamp}");
        assert!(stamp.starts_with("2026"), "{stamp}");
        assert!(stamp.chars().all(|c| c.is_ascii_digit()), "{stamp}");
    }
}
