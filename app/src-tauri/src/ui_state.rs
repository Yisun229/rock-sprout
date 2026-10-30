//! 界面上「下次打开还记得」的两件事（Q1 / Q13）。
//!
//! 这两个状态和数据缓存不是一回事：清缓存不该把它们清掉，换回旧版本时也不该让
//! 数据缓存把它们覆盖。所以单独一个 `ui-state.json`，跟 `cache/today.json` 分开。

use std::path::Path;

use serde::{Deserialize, Serialize};

/// 下次打开要恢复的界面状态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiState {
    /// 上次停留的页面 id（`today` / `mine` / `bag` / `settings`）。
    /// 前端做合法性判断，这里不校验：认不出来的值前端会退回默认页。
    pub page: Option<String>,
    /// 「没有令牌」那张引导卡已收起。收起后不再自动弹。
    pub token_hint_collapsed: bool,
}

fn file(data_root: &Path) -> std::path::PathBuf {
    data_root.join("ui-state.json")
}

/// 读；文件不在或读坏了都当默认值（默认值 = 不记页面、引导卡未收起）。
pub fn load(data_root: &Path) -> UiState {
    let Ok(text) = std::fs::read_to_string(file(data_root)) else {
        return UiState::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// 写。目录不在就建，写失败只回错误字符串，由调用方决定是否记日志。
pub fn save(data_root: &Path, state: &UiState) -> Result<(), String> {
    std::fs::create_dir_all(data_root).map_err(|e| format!("建数据目录失败：{e}"))?;
    let text = serde_json::to_string(state).map_err(|e| format!("序列化失败：{e}"))?;
    std::fs::write(file(data_root), text).map_err(|e| format!("写 ui-state.json 失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let base = std::env::var("CARGO_TARGET_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("target"));
        let dir = base.join("rock-sprout-tests").join("ui-state").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn 写进去能读回来() {
        let dir = temp_dir("roundtrip");
        let state = UiState {
            page: Some("bag".to_owned()),
            token_hint_collapsed: true,
        };
        save(&dir, &state).expect("写");
        assert_eq!(load(&dir), state);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 还没写过就是默认值() {
        let dir = temp_dir("missing");
        assert_eq!(load(&dir), UiState::default());
        assert_eq!(load(&dir).page, None);
        assert!(!load(&dir).token_hint_collapsed);
    }

    #[test]
    fn 文件读坏了退回默认值不报错() {
        let dir = temp_dir("corrupt");
        std::fs::create_dir_all(&dir).expect("建目录");
        std::fs::write(file(&dir), "这不是 JSON").expect("故意写坏");
        assert_eq!(load(&dir), UiState::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 只写一半字段也能读() {
        let dir = temp_dir("partial");
        std::fs::create_dir_all(&dir).expect("建目录");
        std::fs::write(file(&dir), r#"{"page":"mine"}"#).expect("写");
        let st = load(&dir);
        assert_eq!(st.page.as_deref(), Some("mine"));
        assert!(!st.token_hint_collapsed, "缺的字段按默认值");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
