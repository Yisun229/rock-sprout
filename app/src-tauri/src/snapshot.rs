//! 快照与回滚：每次动合并包之前先留一份现场，出事了一条命令退回去。
//!
//! 一份快照里必须**成对**存两样东西，少一样就会「列表和游戏里的东西对不上」：
//!
//! ```text
//! <数据目录>/snapshots/<时间戳>-<原因>/
//! ├── mods_P.pak     合并包（没有装任何 mod 时这份快照里就没有它）
//! ├── installed/     记账目录整体副本
//! └── meta.json      { 原因, 时间, 是手动还是自动, mod 数量, 原 pak 大小+时间 }
//! ```
//!
//! 保留策略：自动快照最多留 3 份（超了删最旧的），手动保存点不参与自动清理。

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::game;
use crate::integrate;
use crate::ipc::mods::records_dir_at;
use crate::paths;

/// 自动快照最多留几份。
pub const KEEP_AUTO: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMeta {
    /// 为什么拍的这份（「装之前」「回滚前」…）
    pub reason: String,
    /// 拍的时刻（毫秒时间戳）
    pub created_at: u64,
    /// 手动保存点（不参与自动清理）
    pub manual: bool,
    /// 当时记账里有几个 mod
    pub mods: usize,
    /// 当时合并包多大（没有合并包就是 0）
    pub pak_bytes: u64,
    /// 当时原 pak 的大小与修改时间，用来判断游戏有没有更新
    pub main_pak_size: Option<u64>,
    pub main_pak_mtime: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    /// 目录名，也是给前端用的 id
    pub id: String,
    pub dir: String,
    pub meta: SnapshotMeta,
}

#[derive(Debug, Clone, Serialize)]
pub struct RollbackOutcome {
    /// 退回到了哪一份
    pub id: String,
    /// 退回之后记账里有几个 mod
    pub mods: usize,
    /// 退回之前那一刻的现场存在哪（想再退回来就用它）
    pub before: String,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 把原因变成目录名的一部分：只留字母数字和汉字，其它换成 `-`。
fn slug(reason: &str) -> String {
    let mut out: String = reason
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    out = out.trim_matches('-').to_owned();
    out.truncate(24);
    if out.is_empty() {
        "snapshot".to_owned()
    } else {
        out
    }
}

fn root_at(data_root: &Path) -> PathBuf {
    data_root.join("snapshots")
}

fn main_pak_fingerprint() -> (Option<u64>, Option<u64>) {
    let Some(installation) = game::find_installation() else {
        return (None, None);
    };
    match std::fs::metadata(installation.main_pak()) {
        Ok(m) => (
            Some(m.len()),
            m.modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64),
        ),
        Err(_) => (None, None),
    }
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("建目录失败 {}：{e}", to.display()))?;
    let entries =
        std::fs::read_dir(from).map_err(|e| format!("读目录失败 {}：{e}", from.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let dest = to.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &dest)?;
        } else {
            std::fs::copy(&path, &dest).map_err(|e| format!("复制失败 {}：{e}", path.display()))?;
        }
    }
    Ok(())
}

fn count_records(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|it| {
            it.flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
                .count()
        })
        .unwrap_or(0)
}

/// 拍一份快照（用程序自己的数据目录）。
pub fn create(reason: &str, manual: bool) -> Result<Snapshot, String> {
    create_at(&paths::data_root(), reason, manual)
}

fn create_at(data_root: &Path, reason: &str, manual: bool) -> Result<Snapshot, String> {
    let created_at = now_ms();
    let root = root_at(data_root);
    let mut dir = root.join(format!("{created_at}-{}", slug(reason)));
    let mut n = 1;
    while dir.exists() {
        dir = root.join(format!("{created_at}-{}-{n}", slug(reason)));
        n += 1;
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("建快照目录失败：{e}"))?;

    // 合并包（没装任何 mod 时就没有这个文件）
    let mut pak_bytes = 0u64;
    if let Ok(pak) = game::installed_pak_path() {
        if pak.exists() {
            let dest = dir.join("mods_P.pak");
            std::fs::copy(&pak, &dest).map_err(|e| format!("复制合并包失败：{e}"))?;
            pak_bytes = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
        }
    }

    // 记账目录整体副本
    let records = records_dir_at(data_root);
    let mods = count_records(&records);
    if records.exists() {
        copy_dir(&records, &dir.join("installed"))?;
    } else {
        std::fs::create_dir_all(dir.join("installed"))
            .map_err(|e| format!("建快照目录失败：{e}"))?;
    }

    let (main_pak_size, main_pak_mtime) = main_pak_fingerprint();
    let meta = SnapshotMeta {
        reason: reason.to_owned(),
        created_at,
        manual,
        mods,
        pak_bytes,
        main_pak_size,
        main_pak_mtime,
    };
    let text = serde_json::to_string_pretty(&meta).map_err(|e| format!("写快照信息失败：{e}"))?;
    std::fs::write(dir.join("meta.json"), text).map_err(|e| format!("写快照信息失败：{e}"))?;

    info!(
        id = %dir.file_name().unwrap_or_default().to_string_lossy(),
        reason, manual, mods, "拍了快照"
    );
    if !manual {
        prune_auto_at(data_root)?;
    }

    Ok(Snapshot {
        id: dir.file_name().unwrap_or_default().to_string_lossy().to_string(),
        dir: dir.display().to_string(),
        meta,
    })
}

/// 只留最近 [`KEEP_AUTO`] 份自动快照，手动保存点不动。
fn prune_auto_at(data_root: &Path) -> Result<(), String> {
    let mut auto: Vec<Snapshot> = list_at(data_root)?
        .into_iter()
        .filter(|s| !s.meta.manual)
        .collect();
    if auto.len() <= KEEP_AUTO {
        return Ok(());
    }
    auto.sort_by_key(|s| std::cmp::Reverse(s.meta.created_at));
    for old in auto.into_iter().skip(KEEP_AUTO) {
        let dir = PathBuf::from(&old.dir);
        if let Err(e) = std::fs::remove_dir_all(&dir) {
            warn!(dir = %dir.display(), %e, "删旧快照没成功");
        } else {
            info!(id = %old.id, "删掉旧的自动快照");
        }
    }
    Ok(())
}

/// 列出所有快照，新的在前。读不出信息的目录直接跳过（用户可能手动删过东西）。
pub fn list() -> Result<Vec<Snapshot>, String> {
    list_at(&paths::data_root())
}

fn list_at(data_root: &Path) -> Result<Vec<Snapshot>, String> {
    let root = root_at(data_root);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let entries = std::fs::read_dir(&root).map_err(|e| format!("读快照目录失败：{e}"))?;
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(dir.join("meta.json")) else {
            continue;
        };
        let Ok(meta) = serde_json::from_str::<SnapshotMeta>(&text) else {
            continue;
        };
        out.push(Snapshot {
            id: entry.file_name().to_string_lossy().to_string(),
            dir: dir.display().to_string(),
            meta,
        });
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.meta.created_at));
    Ok(out)
}

fn find_at(data_root: &Path, id: &str) -> Result<Snapshot, String> {
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err("快照 id 不对".to_owned());
    }
    list_at(data_root)?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| format!("找不到这份快照：{id}"))
}

/// 删掉一份快照（目录整个删）。
pub fn delete(id: &str) -> Result<(), String> {
    let snap = find_at(&paths::data_root(), id)?;
    std::fs::remove_dir_all(&snap.dir).map_err(|e| format!("删快照失败：{e}"))?;
    info!(id, "删掉了一份快照");
    Ok(())
}

/// 把某份快照里的现场铺回去：合并包 + 记账目录。
///
/// 合并包走的是和装 mod 一样的「先写 `.tmp` 再换上去」，
/// 所以这一步本身失败了也不会把当前这个包弄坏。
fn apply_snapshot(snap: &Snapshot) -> Result<usize, String> {
    let dir = PathBuf::from(&snap.dir);
    let installation =
        game::find_installation().ok_or_else(|| "没找到《深岩银河》的安装位置".to_owned())?;
    let paks = installation.paks_path();

    let pak_in_snapshot = dir.join("mods_P.pak");
    let (main, tmp, _) = integrate::pak_paths(&paks);
    if snap.meta.mods == 0 {
        // 这份快照是「一个都没装」的时候拍的：不管当时目录里有没有残留，
        // 退回这个状态都等于「游戏里不该有任何 mod」，当前合并包直接去掉。
        if main.exists() {
            std::fs::remove_file(&main).map_err(|e| format!("删当前合并包失败：{e}"))?;
        }
    } else if pak_in_snapshot.exists() {
        std::fs::copy(&pak_in_snapshot, &tmp)
            .map_err(|e| format!("复制快照里的合并包失败：{e}"))?;
        integrate::swap_in(&paks).map_err(|e| format!("换上快照里的合并包失败：{e}"))?;
    } else {
        warn!(id = %snap.id, "这份快照里没有合并包，只把记账退回去了");
    }

    // 记账目录整体换回去
    let records = records_dir_at(&paths::data_root());
    if records.exists() {
        std::fs::remove_dir_all(&records).map_err(|e| format!("清当前记账失败：{e}"))?;
    }
    copy_dir(&dir.join("installed"), &records)?;

    Ok(count_records(&records))
}

/// 退回到某份快照。
///
/// 顺序：先给「现在」拍一份快照（万一退回错了还能回来），再铺快照，
/// 最后核对记账条数与快照记录的是否一致——不一致就退回「回滚前」那份并报错。
pub fn rollback(id: &str) -> Result<RollbackOutcome, String> {
    let snap = find_at(&paths::data_root(), id)?;
    let before = create("回滚前", false)?;

    let mods = match apply_snapshot(&snap) {
        Ok(n) => n,
        Err(e) => {
            let _ = apply_snapshot(&before);
            return Err(format!("回滚没做成，已经退回原样：{e}"));
        }
    };

    if mods != snap.meta.mods {
        let _ = apply_snapshot(&before);
        return Err(format!(
            "回滚后记账有 {mods} 条，快照里记的是 {} 条，对不上，已经退回原样",
            snap.meta.mods
        ));
    }

    info!(id, mods, "回滚完成");
    Ok(RollbackOutcome {
        id: snap.id,
        mods,
        before: before.id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_data(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rs-snap-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("mods/installed")).expect("建测试目录");
        dir
    }

    #[test]
    fn 原因会变成能当目录名的样子() {
        assert_eq!(slug("装之前"), "装之前");
        assert_eq!(slug("auto before install!"), "auto-before-install");
        assert_eq!(slug("///"), "snapshot");
    }

    #[test]
    fn 拍快照会把记账一起存下来() {
        let data = temp_data("create");
        std::fs::write(data.join("mods/installed/1.json"), "{}").expect("写记账");
        let snap = create_at(&data, "测试", true).expect("应该能拍");
        assert_eq!(snap.meta.mods, 1, "应该记下 1 条");
        assert!(
            PathBuf::from(&snap.dir).join("installed/1.json").exists(),
            "记账副本应该在"
        );
        assert!(snap.meta.manual, "手动标记应该传下去");
    }

    #[test]
    fn 自动快照只留三份手动保存点不动() {
        let data = temp_data("prune");
        let manual = create_at(&data, "保存点", true).expect("拍手动保存点");
        for _ in 0..KEEP_AUTO + 2 {
            create_at(&data, "自动", false).expect("拍自动快照");
        }
        let all = list_at(&data).expect("列快照");
        assert_eq!(
            all.iter().filter(|s| s.meta.manual).count(),
            1,
            "手动保存点不该被清"
        );
        assert_eq!(
            all.iter().filter(|s| !s.meta.manual).count(),
            KEEP_AUTO,
            "自动快照应该只剩 3 份"
        );
        assert!(PathBuf::from(manual.dir).exists(), "手动保存点还在");
    }

    #[test]
    fn 删快照之后就列不出来了() {
        let data = temp_data("delete");
        let snap = create_at(&data, "要删的", true).expect("拍快照");
        let dir = PathBuf::from(&snap.dir);
        std::fs::remove_dir_all(&dir).expect("删掉");
        assert!(list_at(&data).expect("列快照").is_empty(), "删完应该一条都没有");
    }

    #[test]
    fn 快照id_里带斜杠会被拒绝() {
        let data = temp_data("idcheck");
        assert!(find_at(&data, "../../etc").is_err(), "带 .. 的 id 应该拒绝");
        assert!(find_at(&data, "a/b").is_err(), "带斜杠的 id 应该拒绝");
    }
}
