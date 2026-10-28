//! 数据目录的确定方式。
//!
//! 岩笋是绿色程序：配置、日志、缓存都跟着程序目录走，不往 C 盘用户目录写东西。
//! 想临时换个位置，设环境变量 `ROCK_SPROUT_DATA` 即可。

use std::path::PathBuf;

pub fn data_root() -> PathBuf {
    if let Ok(dir) = std::env::var("ROCK_SPROUT_DATA") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }

    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    let base = exe.parent().unwrap_or_else(|| std::path::Path::new("."));
    base.join("data")
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

