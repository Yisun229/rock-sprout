//! 数据目录的确定方式。
//!
//! 岩笋是绿色程序：配置、日志、缓存都跟着程序目录走，不往 C 盘用户目录写东西。
//! 想临时换个位置，设环境变量 `ROCK_SPROUT_DATA` 即可。
//!
//! 但「跟着程序走」有个前提——那个位置真能写。装在 `C:\Program Files` 里的话，
//! 程序目录是只读的，第一次写文件就失败，而报错里根本不会出现「没有权限」这四个字。
//! 所以这里按顺序试几个地方，挑第一个确实能写的：环境变量 → exe 同目录 `data\` →
//! 用户目录 `%LOCALAPPDATA%\RockSprout\data`。挑到哪个、为什么，都记在 [`DataMode`] 里。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// 数据目录是怎么定下来的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataMode {
    /// 用户用环境变量 `ROCK_SPROUT_DATA` 指定的。
    Env,
    /// 就在 exe 旁边的 `data\`（绿色模式，正常情况）。
    Portable,
    /// 程序目录写不了，退到用户目录 `%LOCALAPPDATA%\RockSprout\data`。
    Fallback,
}

impl DataMode {
    /// 给界面和日志看的短名字（稳定，别随手改）。
    pub fn as_str(self) -> &'static str {
        match self {
            DataMode::Env => "env",
            DataMode::Portable => "portable",
            DataMode::Fallback => "fallback",
        }
    }

    /// 中文说法，直接给人看。
    pub fn label(self) -> &'static str {
        match self {
            DataMode::Env => "环境变量指定的位置",
            DataMode::Portable => "程序旁边的 data 文件夹",
            DataMode::Fallback => "用户目录（程序目录写不了）",
        }
    }
}

/// 进程内只判定一次：数据目录定了就不再变（不然日志写到一半换地方）。
static CHOSEN: OnceLock<(PathBuf, DataMode)> = OnceLock::new();

/// 实际用的数据目录。
pub fn data_root() -> PathBuf {
    writable_data_root().0
}

/// 实际用的数据目录 + 它属于哪种模式。
///
/// 第一次调用时才去试写，结果缓存住。
pub fn writable_data_root() -> (PathBuf, DataMode) {
    CHOSEN.get_or_init(resolve_data_root).clone()
}

fn resolve_data_root() -> (PathBuf, DataMode) {
    // 1. 环境变量优先（测试脚本和高级用户都靠它）
    if let Ok(dir) = std::env::var("ROCK_SPROUT_DATA") {
        if !dir.is_empty() {
            let path = PathBuf::from(dir);
            // 环境变量是显式指令，写不了也照用（让错误出现在它该出现的地方）
            return (path, DataMode::Env);
        }
    }

    // 2. exe 旁边的 data\
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    let base = exe.parent().unwrap_or_else(|| Path::new("."));
    let portable = base.join("data");
    if is_writable(&portable) {
        return (portable, DataMode::Portable);
    }

    // 3. 用户目录兜底
    if let Some(fallback) = fallback_root() {
        if is_writable(&fallback) {
            return (fallback, DataMode::Fallback);
        }
    }

    // 都写不了（几乎不可能）：还是返回 exe 旁边，让错误发生在该发生的地方
    (portable, DataMode::Portable)
}

/// `%LOCALAPPDATA%\RockSprout\data`。
pub fn fallback_root() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    if base.is_empty() {
        return None;
    }
    Some(PathBuf::from(base).join("RockSprout").join("data"))
}

/// 这个目录能不能写：建出来、写一个测试文件、删掉。
///
/// 只看「目录存在」不够——Program Files 里的目录存在，但写不进去。
pub fn is_writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".write-test");
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// 令牌文件：`<数据目录>\modio-token.txt`，里面只有一行令牌。
///
/// 之所以先用文件、不用系统钥匙串：M2 阶段要能拿命令行工具直接测，
/// 钥匙串要等界面做好、用户真去粘贴令牌时再接。
pub fn token_file() -> PathBuf {
    data_root().join("modio-token.txt")
}

/// 下载下来的 mod 压缩包放这里（`<数据目录>\mods\`）。
pub fn mods_cache_dir() -> PathBuf {
    data_root().join("mods")
}

/// 日志目录（`<数据目录>\logs\`）。
pub fn logs_dir() -> PathBuf {
    data_root().join("logs")
}

/// 快照目录（`<数据目录>\snapshots\`）：每次动合并包之前留一份，出事了能退回去。
pub fn snapshots_dir() -> PathBuf {
    data_root().join("snapshots")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 能写的目录判定为可写() {
        let dir = std::env::temp_dir().join("rock-sprout-writable-test");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(is_writable(&dir));
        assert!(dir.exists());
        // 测试文件用完要擦掉，别留在里面
        assert!(!dir.join(".write-test").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 模式的名字是稳定的三个() {
        assert_eq!(DataMode::Env.as_str(), "env");
        assert_eq!(DataMode::Portable.as_str(), "portable");
        assert_eq!(DataMode::Fallback.as_str(), "fallback");
    }

    #[test]
    fn 建不出来的目录判定为不可写() {
        // 拿一个普通文件当"目录"的上级：create_dir_all 必然失败
        let file = std::env::temp_dir().join("rock-sprout-is-a-file");
        let _ = std::fs::remove_file(&file);
        std::fs::write(&file, b"x").unwrap();
        assert!(!is_writable(&file.join("data")));
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn 用户目录兜底路径以localappdata开头() {
        if let Some(p) = fallback_root() {
            assert!(p.ends_with("data"));
            assert!(p.to_string_lossy().contains("RockSprout"));
        }
    }
}
