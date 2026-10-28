//! 记住「游戏本体那个主 pak 长什么样」，游戏被人换了（更新了）就要知道。
//!
//! 为什么要记：合并包是拿**这一版**游戏的主 pak 数据合成的。
//! 游戏一更新，旧合并包和旧快照里装的还是上一版的数据，硬塞回去等于往新游戏里投毒。
//! 所以这里只做一件事——比对大小和修改时间，变了就让界面把「退回到某个快照」按灰。
//!
//! 存在 `<数据目录>/game-state.json`，只在合并成功之后写一次。

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::game;
use crate::paths;

/// 允许的时间差：`2` 秒。
///
/// 文件系统记修改时间的精度不一样（有的到秒、有的到 0.1 秒），
/// 差个一两秒不代表内容变了，别拿它吓用户。
const MTIME_TOLERANCE_MS: u64 = 2_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    /// 主 pak 多大
    pub main_pak_size: u64,
    /// 主 pak 的最后修改时间（毫秒时间戳）
    pub main_pak_mtime: u64,
    /// 什么时候记下的
    pub captured_at: u64,
}

/// 一次比对的结果，前端按它决定顶部提不提示、按钮灰不灰。
#[derive(Debug, Clone, Serialize)]
pub struct GameCheck {
    /// 找到游戏了没有
    pub has_game: bool,
    /// 之前记过指纹没有
    pub recorded: bool,
    /// 主 pak 变了（游戏更新过）
    pub changed: bool,
    /// 一句话结论，界面直接显示
    pub summary: String,
    pub recorded_size: Option<u64>,
    pub current_size: Option<u64>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn state_file(data_root: &Path) -> std::path::PathBuf {
    data_root.join("game-state.json")
}

/// 读现在这个主 pak 的大小和修改时间。
pub fn current_fingerprint() -> Option<(u64, u64)> {
    let installation = game::find_installation()?;
    let meta = std::fs::metadata(installation.main_pak()).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)?;
    Some((meta.len(), mtime))
}

/// 记下现在这个主 pak 的样子（合并成功之后调它）。
pub fn capture() {
    let Some((size, mtime)) = current_fingerprint() else {
        return;
    };
    let state = GameState {
        main_pak_size: size,
        main_pak_mtime: mtime,
        captured_at: now_ms(),
    };
    if let Err(e) = write_at(&paths::data_root(), &state) {
        tracing::warn!(%e, "记游戏指纹没成功");
    }
}

fn write_at(data_root: &Path, state: &GameState) -> Result<(), String> {
    if let Err(e) = std::fs::create_dir_all(data_root) {
        return Err(format!("建数据目录失败：{e}"));
    }
    let text = serde_json::to_string_pretty(state).map_err(|e| format!("写指纹失败：{e}"))?;
    std::fs::write(state_file(data_root), text).map_err(|e| format!("写指纹失败：{e}"))
}

/// 读之前记下的指纹。
pub fn read() -> Option<GameState> {
    read_at(&paths::data_root())
}

fn read_at(data_root: &Path) -> Option<GameState> {
    let text = std::fs::read_to_string(state_file(data_root)).ok()?;
    serde_json::from_str(&text).ok()
}

/// 比一比：游戏更新过没有。
pub fn check() -> GameCheck {
    check_at(&paths::data_root(), current_fingerprint())
}

fn check_at(data_root: &Path, current: Option<(u64, u64)>) -> GameCheck {
    let recorded = read_at(data_root);
    let Some((size, mtime)) = current else {
        return GameCheck {
            has_game: false,
            recorded: recorded.is_some(),
            changed: false,
            summary: "没找到《深岩银河》（目前只支持 Steam 版）".to_owned(),
            recorded_size: recorded.as_ref().map(|r| r.main_pak_size),
            current_size: None,
        };
    };

    let Some(state) = recorded else {
        return GameCheck {
            has_game: true,
            recorded: false,
            changed: false,
            summary: "还没装过 mod，所以没有可对比的记录".to_owned(),
            recorded_size: None,
            current_size: Some(size),
        };
    };

    let changed = state.main_pak_size != size
        || mtime.abs_diff(state.main_pak_mtime) > MTIME_TOLERANCE_MS;
    let summary = if changed {
        format!(
            "游戏本体好像更新过（主 pak {} → {} 字节），现在的合并包是上一版游戏的数据，得重装一次 mod",
            state.main_pak_size, size
        )
    } else {
        "游戏本体和上次记录一致".to_owned()
    };

    GameCheck {
        has_game: true,
        recorded: true,
        changed,
        summary,
        recorded_size: Some(state.main_pak_size),
        current_size: Some(size),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_data(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("rs-gstate-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建测试目录");
        dir
    }

    fn state(size: u64, mtime: u64) -> GameState {
        GameState {
            main_pak_size: size,
            main_pak_mtime: mtime,
            captured_at: 1,
        }
    }

    #[test]
    fn 没记过的时候说没法比而不是说游戏变了() {
        let data = temp_data("none");
        let c = check_at(&data, Some((100, 5_000)));
        assert!(c.has_game && !c.recorded && !c.changed, "{c:?}");
    }

    #[test]
    fn 大小一样时间也一样就算没变() {
        let data = temp_data("same");
        write_at(&data, &state(100, 5_000)).expect("写指纹");
        let c = check_at(&data, Some((100, 5_000)));
        assert!(!c.changed, "{c:?}");
    }

    #[test]
    fn 时间差两秒以内不算变() {
        let data = temp_data("tolerance");
        write_at(&data, &state(100, 5_000)).expect("写指纹");
        let c = check_at(&data, Some((100, 6_500)));
        assert!(!c.changed, "差 1.5 秒不该当成更新：{c:?}");
        let c2 = check_at(&data, Some((100, 7_500)));
        assert!(c2.changed, "差 2.5 秒应该当成更新：{c2:?}");
    }

    #[test]
    fn 大小变了就是游戏更新了() {
        let data = temp_data("size");
        write_at(&data, &state(100, 5_000)).expect("写指纹");
        let c = check_at(&data, Some((200, 5_000)));
        assert!(c.changed, "{c:?}");
        assert!(c.summary.contains("更新"), "{}", c.summary);
    }

    #[test]
    fn 找不到游戏时说没找到而不是说变了() {
        let data = temp_data("nogame");
        write_at(&data, &state(100, 5_000)).expect("写指纹");
        let c = check_at(&data, None);
        assert!(!c.has_game && !c.changed, "{c:?}");
        assert!(c.summary.contains("没找到"), "{}", c.summary);
    }
}
