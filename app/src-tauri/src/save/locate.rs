//! 找存档文件：只找、只读。
//!
//! 这里**不复制、不移走、不删除**任何东西——找完把路径交出去，读的时候
//! 由 [`super::store::load`] 自己复制一份副本来解析，原档一个字节都不碰。
//!
//! 找两处（都只扫一层层往下、深度有上限，不整盘乱翻）：
//!
//! 1. `%LOCALAPPDATA%\FSD\Saved\SaveGames`——游戏自己的存档目录；
//! 2. `<Steam>\userdata\<账号>\548430\`——Steam 云同步的那份。
//!
//! 只认 `*_Player.sav`：这是账号级主档，`_Backup_*` 之类的是自动备份。
//!
//! 设了环境变量 `ROCK_SPROUT_SAVE_DIR` 就只扫那个目录——这是给测试看的开关，
//! 用来拧「找不到存档 / 版本不认识 / 结构读不通」那几屏，平时不用设。

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// 一个候选存档文件。
#[derive(Debug, Clone)]
pub struct SaveFile {
    pub path: PathBuf,
    pub modified: SystemTime,
    pub bytes: u64,
}

/// 往下翻的层数上限：`548430\ac\GameInstall\FSD\Saved\SaveGames` 是 5 层。
const MAX_DEPTH: usize = 6;

/// 所有找得到的账号主档，按修改时间从新到旧。
///
/// 同一份存档可能被找到两次（`D:\Steam` 和 `D:\steam` 在 Windows 上是同一个目录，
/// 大小写不同、字符串比不出来），所以按真实路径去重，别让选择器里出现两条一样的。
pub fn candidates() -> Vec<SaveFile> {
    let mut out = Vec::new();
    for root in roots() {
        walk(&root, 0, &mut out);
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    let mut seen: Vec<PathBuf> = Vec::new();
    out.retain(|f| {
        let key = std::fs::canonicalize(&f.path).unwrap_or_else(|_| f.path.clone());
        if seen.iter().any(|s| s == &key) {
            return false;
        }
        seen.push(key);
        true
    });
    out
}

/// 最新的一份存档（用户最近玩的那个账号）。
pub fn newest() -> Option<SaveFile> {
    candidates().into_iter().next()
}

fn roots() -> Vec<PathBuf> {
    // 测试用的开关：指到某个目录就只扫那里，不去翻真实的存档位置。
    // 只为拧一下「找不到 / 版本不认识 / 结构读不通」这几屏，正常运行时不设。
    if let Some(dir) = std::env::var_os("ROCK_SPROUT_SAVE_DIR") {
        let p = PathBuf::from(dir);
        return if p.is_dir() { vec![p] } else { Vec::new() };
    }
    let mut roots = Vec::new();
    if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
        let p = PathBuf::from(dir).join("FSD").join("Saved").join("SaveGames");
        if p.is_dir() {
            roots.push(p);
        }
    }
    for steam in steam_dirs() {
        let p = steam.join("userdata");
        if p.is_dir() {
            roots.push(p);
        }
    }
    roots
}

/// Steam 装在哪：先问注册表，再看几个常见位置。
fn steam_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut push = |p: PathBuf| {
        if p.is_dir() && !dirs.iter().any(|d| d == &p) {
            dirs.push(p);
        }
    };

    #[cfg(windows)]
    {
        use crate::webview2::win::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, read_string};
        for sub in ["Software\\Valve\\Steam", "Software\\WOW6432Node\\Valve\\Steam"] {
            if let Some(p) = read_string(HKEY_CURRENT_USER, sub, "SteamPath") {
                push(PathBuf::from(p));
            }
            if let Some(p) = read_string(HKEY_LOCAL_MACHINE, sub, "InstallPath") {
                push(PathBuf::from(p));
            }
        }
    }

    for p in [
        "C:\\Program Files (x86)\\Steam",
        "C:\\Program Files\\Steam",
        "D:\\Steam",
        "D:\\steam",
        "E:\\Steam",
        "E:\\steam",
    ] {
        push(PathBuf::from(p));
    }
    dirs
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<SaveFile>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() {
            walk(&path, depth + 1, out);
            continue;
        }
        if !is_player_save(&path) {
            continue;
        }
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        out.push(SaveFile { path, modified, bytes: meta.len() });
    }
}

fn is_player_save(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    name.ends_with("_Player.sav")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_player_saves_count() {
        assert!(is_player_save(Path::new("76561199027062409_Player.sav")));
        assert!(!is_player_save(Path::new("76561199027062409_Backup_0.sav")));
        assert!(!is_player_save(Path::new("76561199027062409_Player.txt")));
    }
}
