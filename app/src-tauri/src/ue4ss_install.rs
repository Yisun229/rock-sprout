//! 把 UE4SS 铺进游戏目录、以及原样撤回来（M7-2）。
//!
//! 这件事的风险全在「写别人家的目录」：游戏目录里本来可能已经有一套改过的 UE4SS
//! （比如别的管理器装的），我们要么原样退让，要么先整个备份再替换。所以这里的顺序是死的：
//!
//! 1. 游戏没在跑才能动（改到一半游戏开着，文件会锁住）；
//! 2. 先看清现在有什么，是别人的就等用户点头（不点头就什么都不写）；
//! 3. 把 `dwmapi.dll` 和整个 `ue4ss\` 拷进我们自己的备份目录，写一份清单；
//! 4. 才把官方那四样铺进去；
//! 5. 卸载 = 撤掉我们铺的、把备份原样放回去。
//!
//! 全程只碰这四个名字：`UE4SS.dll`、`dwmapi.dll`、`UE4SS-settings.ini`、`Mods\`。
//! 游戏自己的 `x3daudio1_7.dll`、`Content\Paks` 下的 pak 一概不碰。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::proc;
use crate::ue4ss::{self, Ue4ssError};
use mint_lib::DRGInstallation;

/// 我们铺进游戏目录的四样（相对 Win64 目录）。
pub const OUR_ITEMS: [&str; 4] = ["UE4SS.dll", "dwmapi.dll", "UE4SS-settings.ini", "Mods"];

/// UE4SS 跑起来之后自己往 Win64 里写的东西，卸载时一并清掉（计划 §0 第 29 条）。
pub const RUNTIME_LOG: &str = "UE4SS.log";
pub const RUNTIME_INI_BAK: &str = "UE4SS-settings.ini.bak";
pub const RUNTIME_CRASHDUMP_DIR: &str = "CrashDumps";

/// 游戏目录里现在这套 UE4SS 是什么来路。
#[derive(Debug, Clone, Serialize)]
pub struct Existing {
    /// `none` = 干干净净；`ours` = 我们自己铺的；`other` = 别人那套（别的管理器装的）
    pub kind: &'static str,
    /// 给人看的说明：里面有什么、大概是谁装的
    pub detail: String,
}

/// 备份清单里的一条。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// 相对 Win64 的路径，用 `/` 分隔
    pub rel: String,
    pub size: u64,
    pub sha256: String,
}

/// 一次备份的清单（`manifest.json`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub created_at: u64,
    /// 备份了 `dwmapi.dll` 没有
    pub has_dwmapi: bool,
    /// 备份了 `ue4ss\` 目录没有
    pub has_ue4ss_dir: bool,
    /// 备份了原来的 `Mods\` 没有（通常没有）
    pub has_mods: bool,
    /// 以下是 UE4SS 跑起来之后自己写的东西，不算安装清单里的。
    /// 备份时记下原来有没有，卸载时只清我们这段时间新长出来的。
    #[serde(default)]
    pub had_log: bool,
    #[serde(default)]
    pub had_ini_bak: bool,
    #[serde(default)]
    pub had_crashdumps: bool,
    pub files: Vec<Entry>,
}

impl Manifest {
    fn empty(created_at: u64) -> Self {
        Self {
            created_at,
            has_dwmapi: false,
            has_ue4ss_dir: false,
            has_mods: false,
            had_log: false,
            had_ini_bak: false,
            had_crashdumps: false,
            files: Vec::new(),
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}


/// 看游戏目录里现在有什么。只读，不写。
pub fn detect(win64: &Path) -> Existing {
    let dll = win64.join("UE4SS.dll");
    let ini = win64.join("UE4SS-settings.ini");
    let sub = win64.join("ue4ss");
    let sub_core = sub.join("UE4SS.dll");
    let sub_mint = sub.join("UE4SSL.dll");
    let proxy = win64.join("dwmapi.dll");

    // 根目录下有核心文件 = 已经有一套平铺的 UE4SS（可能是我们铺的，也可能是别人）
    if dll.exists() || ini.exists() {
        let ours = dll
            .metadata()
            .map(|m| m.len() == ue4ss::UE4SS_DLL_SIZE)
            .unwrap_or(false)
            && dll.exists()
            && ue4ss::sha256_file(&dll).as_deref() == Some(ue4ss::UE4SS_DLL_SHA256);
        return Existing {
            kind: if ours { "ours" } else { "other" },
            detail: if ours {
                "根目录里是我们铺的官方 UE4SS。".to_owned()
            } else {
                "根目录里已经有一套 UE4SS，不是我们铺的（可能来自别的管理器）。".to_owned()
            },
        };
    }

    // 子目录里有一套 = 别的管理器的装法
    if sub_core.exists() || sub_mint.exists() {
        let what = if sub_mint.exists() {
            "ue4ss\\UE4SSL.dll"
        } else {
            "ue4ss\\UE4SS.dll"
        };
        return Existing {
            kind: "other",
            detail: format!("游戏目录里已经有一套 UE4SS：`{what}` 在 `ue4ss\\` 子目录里，不是我们铺的。"),
        };
    }

    // 只剩一个改名过的代理 dll（没有核心文件）= 半截残留
    if proxy.exists() {
        let our_proxy = ue4ss::sha256_file(&proxy).as_deref() == Some(ue4ss::DWMAPI_SHA256);
        if our_proxy {
            return Existing {
                kind: "ours",
                detail: "游戏目录里只有代理 dll，核心文件不见了（上次可能没装完整）。".to_owned(),
            };
        }
        return Existing {
            kind: "other",
            detail: "游戏目录里有个 `dwmapi.dll`（别的工具留下的代理），里面没有 UE4SS 的核心文件。"
                .to_owned(),
        };
    }

    Existing {
        kind: "none",
        detail: "游戏目录里还没有任何 UE4SS。".to_owned(),
    }
}

/// 备份目录：`<数据目录>\backup\ue4ss-<毫秒>\`。
pub fn backup_dir(data_root: &Path, created_at: u64) -> PathBuf {
    data_root
        .join("backup")
        .join(format!("ue4ss-{created_at}"))
}

/// 把 `path` 这份文件/目录整个拷到 `dest`，同时记账。
fn copy_tree(src: &Path, dest: &Path, base: &Path, out: &mut Vec<Entry>) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dest)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let child_src = entry.path();
            let child_dest = dest.join(entry.file_name());
            copy_tree(&child_src, &child_dest, base, out)?;
        }
        return Ok(());
    }
    std::fs::copy(src, dest)?;
    let rel = src
        .strip_prefix(base)
        .unwrap_or(src)
        .to_string_lossy()
        .replace('\\', "/");
    out.push(Entry {
        rel,
        size: src.metadata().map(|m| m.len()).unwrap_or(0),
        sha256: ue4ss::sha256_file(src).unwrap_or_default(),
    });
    Ok(())
}

/// 把现有的那套（dwmapi.dll + ue4ss\ + 可能存在的 Mods\）备份到我们自己的目录。
///
/// 返回清单；什么都没备份到就是空清单。
pub fn backup_existing(win64: &Path, data_root: &Path) -> Result<(PathBuf, Manifest), Ue4ssError> {
    let created_at = now_ms();
    let dir = backup_dir(data_root, created_at);
    std::fs::create_dir_all(&dir).map_err(|e| Ue4ssError {
        code: "backup_io",
        message: format!("建备份目录失败：{e}"),
    })?;

    let mut manifest = Manifest::empty(created_at);

    let proxy = win64.join("dwmapi.dll");
    if proxy.is_file() {
        let mut files = Vec::new();
        copy_tree(&proxy, &dir.join("dwmapi.dll"), win64, &mut files).map_err(|e| Ue4ssError {
            code: "backup_io",
            message: format!("备份 dwmapi.dll 失败：{e}"),
        })?;
        manifest.has_dwmapi = true;
        manifest.files.extend(files);
    }

    let sub = win64.join("ue4ss");
    if sub.is_dir() {
        let mut files = Vec::new();
        copy_tree(&sub, &dir.join("ue4ss"), win64, &mut files).map_err(|e| Ue4ssError {
            code: "backup_io",
            message: format!("备份 ue4ss 目录失败：{e}"),
        })?;
        manifest.has_ue4ss_dir = true;
        manifest.files.extend(files);
    }

    let mods = win64.join("Mods");
    if mods.is_dir() {
        let mut files = Vec::new();
        copy_tree(&mods, &dir.join("Mods"), win64, &mut files).map_err(|e| Ue4ssError {
            code: "backup_io",
            message: format!("备份 Mods 目录失败：{e}"),
        })?;
        manifest.has_mods = true;
        manifest.files.extend(files);
    }

    // 运行期文件：只记号它们原来在不在，不复制（日志动辄几十 KB，留着没用）
    manifest.had_log = win64.join(RUNTIME_LOG).is_file();
    manifest.had_ini_bak = win64.join(RUNTIME_INI_BAK).is_file();
    manifest.had_crashdumps = win64.join(RUNTIME_CRASHDUMP_DIR).is_dir();

    let text = serde_json::to_string_pretty(&manifest).map_err(|e| Ue4ssError {
        code: "backup_io",
        message: format!("写备份清单失败：{e}"),
    })?;
    std::fs::write(dir.join("manifest.json"), text).map_err(|e| Ue4ssError {
        code: "backup_io",
        message: format!("写备份清单失败：{e}"),
    })?;

    tracing::info!(dir = %dir.display(), files = manifest.files.len(), "已备份现有 UE4SS");
    Ok((dir, manifest))
}

/// 找最近一次备份（卸载时还原用）。
pub fn latest_backup(data_root: &Path) -> Option<PathBuf> {
    let root = data_root.join("backup");
    let mut best: Option<(u64, PathBuf)> = None;
    for entry in std::fs::read_dir(root).ok()? {
        let entry = entry.ok()?;
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(stamp) = name.strip_prefix("ue4ss-") else {
            continue;
        };
        let Ok(stamp) = stamp.parse::<u64>() else {
            continue;
        };
        if best.as_ref().map(|(b, _)| stamp > *b).unwrap_or(true) {
            best = Some((stamp, entry.path()));
        }
    }
    best.map(|(_, p)| p)
}

/// 装完 / 卸完之后，回报给界面的东西。
#[derive(Debug, Clone, Serialize)]
pub struct InstallOutcome {
    /// 我们把哪四样放进去了（或撤掉了）
    pub items: Vec<String>,
    /// 备份目录（没有备份过就是空串）
    pub backup_dir: String,
    /// 现在游戏目录里是什么状态
    pub existing: Existing,
}

fn copy_into(src: &Path, dest: &Path) -> Result<(), Ue4ssError> {
    if src.is_dir() {
        std::fs::create_dir_all(dest).map_err(|e| Ue4ssError {
            code: "write_game",
            message: format!("没法在游戏目录建文件夹：{e}"),
        })?;
        for entry in std::fs::read_dir(src).map_err(|e| Ue4ssError {
            code: "write_game",
            message: format!("读包里的文件夹失败：{e}"),
        })? {
            let entry = entry.map_err(|e| Ue4ssError {
                code: "write_game",
                message: format!("读包里的文件失败：{e}"),
            })?;
            copy_into(&entry.path(), &dest.join(entry.file_name()))?;
        }
        return Ok(());
    }
    std::fs::copy(src, dest).map_err(|e| Ue4ssError {
        code: "write_game",
        message: format!("往游戏目录写 {} 失败：{e}", src.file_name().unwrap_or_default().to_string_lossy()),
    })?;
    Ok(())
}

/// 把官方那四样铺进游戏目录。`confirm_replace` = 用户已经在弹窗里点了同意。
pub fn install(
    game: &DRGInstallation,
    data_root: &Path,
    package_dir: &Path,
    confirm_replace: bool,
) -> Result<InstallOutcome, Ue4ssError> {
    proc::guard().map_err(|message| Ue4ssError {
        code: "game_running",
        message,
    })?;

    let win64 = game.binaries_directory();
    let existing = detect(&win64);
    if existing.kind == "other" && !confirm_replace {
        return Err(Ue4ssError {
            code: "ue4ss_conflict",
            message: existing.detail.clone(),
        });
    }

    // 包得先齐（M7-1 那步负责）
    ue4ss::verify_package(package_dir)?;

    // 只有「游戏目录里原来那套是别人的」才备份。
    // 已经是我们自己铺的就原地覆盖：这时候再备份会把我们自己的文件记成「原来那套」，
    // 以后一卸载就把我们这份又放回去了。
    let backup = if existing.kind == "none" {
        None
    } else if existing.kind == "ours" {
        tracing::info!("游戏目录里本来就是我们的，原地覆盖，不再新增备份");
        None
    } else {
        Some(backup_existing(&win64, data_root)?)
    };

    for item in OUR_ITEMS {
        let src = package_dir.join(item);
        let dest = win64.join(item);
        // 目录项先清干净，免得留下旧版本的文件
        if item == "Mods" && dest.is_dir() && backup.is_none() {
            std::fs::remove_dir_all(&dest).map_err(|e| Ue4ssError {
                code: "write_game",
                message: format!("清理旧的 Mods 目录失败：{e}"),
            })?;
        }
        copy_into(&src, &dest)?;
    }

    let backup_dir = backup
        .as_ref()
        .map(|(dir, _)| dir.display().to_string())
        .unwrap_or_default();
    tracing::info!(win64 = %win64.display(), backup = %backup_dir, "已铺好 UE4SS");

    // 记下「这套是什么时候铺的」：判断「上次启动加载成功没有」时要拿它当地板（M7-3）
    if let Err(e) = crate::ue4ss_health::remember(data_root, now_ms()) {
        tracing::warn!(error = %e, "装好了，但没记下安装时刻（会影响失效检测）");
    }

    Ok(InstallOutcome {
        items: OUR_ITEMS.iter().map(|s| (*s).to_owned()).collect(),
        backup_dir,
        existing: detect(&win64),
    })
}

/// 撤掉我们铺的，把备份原样放回去。
///
/// 没有备份过（原来就是干净的）就只撤不补。
///
/// `Mods\` 格外小心：那是放脚本 mod 的地方，用户可能往里装过东西。所以只有当「原来的游戏目录里
/// 就没这个目录」时才动它，而且不是删——挪进备份目录留一份，万一里面真有用户的脚本还能捞回来。
///
/// 另外，`UE4SS.log` / `UE4SS-settings.ini.bak` / `CrashDumps\` 是 UE4SS 跑起来自己写的，
/// 不在安装清单里；备份时若原来没有，卸载就一并清掉，不给用户留垃圾（计划 §0 第 29 条）。
pub fn uninstall(game: &DRGInstallation, data_root: &Path) -> Result<InstallOutcome, Ue4ssError> {
    proc::guard().map_err(|message| Ue4ssError {
        code: "game_running",
        message,
    })?;

    let win64 = game.binaries_directory();

    // 先把备份和清单读出来，才知道原来有没有 `Mods\`
    let backup = latest_backup(data_root);
    let mut manifest: Option<Manifest> = None;
    if let Some(dir) = &backup {
        let text = std::fs::read_to_string(dir.join("manifest.json")).map_err(|e| Ue4ssError {
            code: "restore_io",
            message: format!("读备份清单失败：{e}"),
        })?;
        let parsed: Manifest = serde_json::from_str(&text).map_err(|e| Ue4ssError {
            code: "restore_io",
            message: format!("备份清单看不懂：{e}"),
        })?;
        manifest = Some(parsed);
    }
    let had_mods = manifest.as_ref().map(|m| m.has_mods).unwrap_or(false);

    // 撤我们那四样
    for item in OUR_ITEMS {
        let path = win64.join(item);
        if !path.exists() {
            continue;
        }
        // Mods 目录：原来没有的话，挪走而不是删（可能装着用户的脚本 mod）
        if item == "Mods" && path.is_dir() && !had_mods {
            let dest = match (&backup, manifest.as_ref()) {
                (Some(dir), _) => dir.join(format!("Mods-installed-{}", now_ms())),
                (None, _) => data_root
                    .join("backup")
                    .join(format!("ue4ss-mods-{}", now_ms()))
                    .join("Mods"),
            };
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| Ue4ssError {
                    code: "restore_io",
                    message: format!("建目录失败：{e}"),
                })?;
            }
            move_tree(&path, &dest)?;
            tracing::info!(from = %path.display(), to = %dest.display(), "Mods 目录已挪走保留");
            continue;
        }
        let result = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        result.map_err(|e| Ue4ssError {
            code: "write_game",
            message: format!("撤掉 {item} 失败：{e}"),
        })?;
    }

    // 清掉 UE4SS 跑起来自己长出来的东西（原来没有才清）
    clean_runtime(win64.as_path(), manifest.as_ref())?;

    // 撤掉了就不再判「有没有生效」
    crate::ue4ss_health::forget(data_root);

    // 把备份放回去
    let mut backup_shown = String::new();
    if let (Some(dir), Some(manifest)) = (&backup, &manifest) {
        if manifest.has_dwmapi {
            copy_into(&dir.join("dwmapi.dll"), &win64.join("dwmapi.dll"))?;
        }
        if manifest.has_ue4ss_dir {
            copy_into(&dir.join("ue4ss"), &win64.join("ue4ss"))?;
        }
        if manifest.has_mods {
            copy_into(&dir.join("Mods"), &win64.join("Mods"))?;
        }
        backup_shown = dir.display().to_string();
        tracing::info!(dir = %backup_shown, "已把原来的 UE4SS 放回去");
    }

    Ok(InstallOutcome {
        items: OUR_ITEMS.iter().map(|s| (*s).to_owned()).collect(),
        backup_dir: backup_shown,
        existing: detect(&win64),
    })
}

/// 清掉 UE4SS 运行期自己写的东西。
///
/// 清单里记了「原来有没有」：原来就没有的才删，避免误删用户/别家工具留下的文件。
/// 没有清单（从来没备份过）时按「原来没有」处理——那时游戏目录是我们第一次碰。
fn clean_runtime(win64: &Path, manifest: Option<&Manifest>) -> Result<(), Ue4ssError> {
    let (had_log, had_ini_bak, had_crashdumps) = match manifest {
        Some(m) => (m.had_log, m.had_ini_bak, m.had_crashdumps),
        None => (false, false, false),
    };

    let log = win64.join(RUNTIME_LOG);
    if !had_log && log.is_file() {
        std::fs::remove_file(&log).map_err(|e| Ue4ssError {
            code: "write_game",
            message: format!("删 {RUNTIME_LOG} 失败：{e}"),
        })?;
        tracing::info!("已清掉 UE4SS.log");
    }

    let ini_bak = win64.join(RUNTIME_INI_BAK);
    if !had_ini_bak && ini_bak.is_file() {
        std::fs::remove_file(&ini_bak).map_err(|e| Ue4ssError {
            code: "write_game",
            message: format!("删 {RUNTIME_INI_BAK} 失败：{e}"),
        })?;
        tracing::info!("已清掉 UE4SS-settings.ini.bak");
    }

    let dumps = win64.join(RUNTIME_CRASHDUMP_DIR);
    if !had_crashdumps && dumps.is_dir() {
        std::fs::remove_dir_all(&dumps).map_err(|e| Ue4ssError {
            code: "write_game",
            message: format!("删 {RUNTIME_CRASHDUMP_DIR} 失败：{e}"),
        })?;
        tracing::info!("已清掉 CrashDumps 目录");
    }

    Ok(())
}

/// 把目录整个搬到别处（先复制再删，避免跨盘失败丢了东西）。
fn move_tree(src: &Path, dest: &Path) -> Result<(), Ue4ssError> {
    let mut ignored = Vec::new();
    copy_tree(src, dest, src, &mut ignored).map_err(|e| Ue4ssError {
        code: "restore_io",
        message: format!("挪走 Mods 目录失败：{e}"),
    })?;
    std::fs::remove_dir_all(src).map_err(|e| Ue4ssError {
        code: "restore_io",
        message: format!("挪完删不掉原目录：{e}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rs-ue4ss-{tag}-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn 空目录里什么都没有() {
        let dir = tmpdir("empty");
        let got = detect(&dir);
        assert_eq!(got.kind, "none");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn 子目录里那套算别人的() {
        let dir = tmpdir("subdir");
        std::fs::create_dir_all(dir.join("ue4ss")).unwrap();
        std::fs::write(dir.join("ue4ss").join("UE4SSL.dll"), b"x").unwrap();
        let got = detect(&dir);
        assert_eq!(got.kind, "other");
        assert!(got.detail.contains("UE4SSL.dll"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn 根目录下的核心文件算别人的而不是我们的() {
        let dir = tmpdir("rootdll");
        std::fs::write(dir.join("UE4SS.dll"), b"not-ours").unwrap();
        let got = detect(&dir);
        assert_eq!(got.kind, "other");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn 备份会把代理和整目录都拷走并记账() {
        let win64 = tmpdir("bk-win");
        let data = tmpdir("bk-data");
        std::fs::write(win64.join("dwmapi.dll"), b"proxy").unwrap();
        std::fs::create_dir_all(win64.join("ue4ss").join("mods").join("x")).unwrap();
        std::fs::write(win64.join("ue4ss").join("UE4SSL.dll"), b"core").unwrap();
        std::fs::write(win64.join("ue4ss").join("mods").join("x").join("main.dll"), b"m").unwrap();

        let (dir, manifest) = backup_existing(&win64, &data).unwrap();
        assert!(manifest.has_dwmapi && manifest.has_ue4ss_dir);
        assert!(dir.join("dwmapi.dll").is_file());
        assert!(dir.join("ue4ss").join("mods").join("x").join("main.dll").is_file());
        // 三条：dwmapi + UE4SSL.dll + main.dll
        assert_eq!(manifest.files.len(), 3);
        assert!(manifest.files.iter().all(|f| f.sha256.len() == 64));

        std::fs::remove_dir_all(&win64).ok();
        std::fs::remove_dir_all(&data).ok();
    }

    #[test]
    fn 最近一次备份要挑时间最大的那个() {
        let data = tmpdir("latest");
        std::fs::create_dir_all(data.join("backup").join("ue4ss-100")).unwrap();
        std::fs::create_dir_all(data.join("backup").join("ue4ss-300")).unwrap();
        std::fs::create_dir_all(data.join("backup").join("ue4ss-200")).unwrap();
        let got = latest_backup(&data).unwrap();
        assert!(got.ends_with("ue4ss-300"));
        std::fs::remove_dir_all(&data).ok();
    }

    #[test]
    fn 备份目录带毫秒时间戳() {
        let dir = backup_dir(Path::new("C:/data"), 1700000000123);
        assert!(dir.ends_with("backup/ue4ss-1700000000123") || dir.ends_with("backup\\ue4ss-1700000000123"));
    }

    #[test]
    fn 备份时记下运行期文件原来在不在() {
        let win64 = tmpdir("runtime-flag");
        std::fs::create_dir_all(&win64).unwrap();
        std::fs::write(win64.join(RUNTIME_LOG), "old log").unwrap();
        let data = tmpdir("runtime-flag-data");

        let (_, manifest) = backup_existing(&win64, &data).unwrap();
        assert!(manifest.had_log, "原来有日志要记下来");
        assert!(!manifest.had_ini_bak);
        assert!(!manifest.had_crashdumps);

        std::fs::remove_dir_all(&win64).ok();
        std::fs::remove_dir_all(&data).ok();
    }

    #[test]
    fn 卸载会清掉运行期长出来的日志和崩溃目录() {
        let win64 = tmpdir("runtime-clean");
        std::fs::create_dir_all(&win64).unwrap();
        // 装的时候没有这些，跑完之后长出来了
        let data = tmpdir("runtime-clean-data");
        let (_, manifest) = backup_existing(&win64, &data).unwrap();
        std::fs::write(win64.join(RUNTIME_LOG), "[2026-01-01 00:00:00] Console created").unwrap();
        std::fs::write(win64.join(RUNTIME_INI_BAK), "bak").unwrap();
        std::fs::create_dir_all(win64.join(RUNTIME_CRASHDUMP_DIR)).unwrap();

        clean_runtime(&win64, Some(&manifest)).unwrap();
        assert!(!win64.join(RUNTIME_LOG).exists());
        assert!(!win64.join(RUNTIME_INI_BAK).exists());
        assert!(!win64.join(RUNTIME_CRASHDUMP_DIR).exists());

        std::fs::remove_dir_all(&win64).ok();
        std::fs::remove_dir_all(&data).ok();
    }

    #[test]
    fn 运行期日志原来就有的就不删() {
        let win64 = tmpdir("runtime-keep");
        std::fs::create_dir_all(&win64).unwrap();
        std::fs::write(win64.join(RUNTIME_LOG), "别人的日志").unwrap();
        let data = tmpdir("runtime-keep-data");
        let (_, manifest) = backup_existing(&win64, &data).unwrap();

        clean_runtime(&win64, Some(&manifest)).unwrap();
        assert!(win64.join(RUNTIME_LOG).exists(), "原来就在的日志不能删");

        std::fs::remove_dir_all(&win64).ok();
        std::fs::remove_dir_all(&data).ok();
    }
}
