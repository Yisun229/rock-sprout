//! 设置页「存档备份」卡背后的两条命令：备份一次、读最近一次备份。
//!
//! 备份：把当前选中的那份账号主档整份拷到数据目录的 `backups\` 下，文件名是
//! `<原文件名去掉 .sav>_<时间戳>.sav`。
//!
//! 还原（v0.3.1 #29）：把 `backups\` 里的某一份**替换**成当前存档。顺序不可反——
//! 先列备份、再点还原、**覆盖前先把当前档另存一份**（`…_before-restore.sav`）。
//! 还原是文件替换，不是改存档内容；替换完的档能不能被当前版本的游戏读，本版没有验证。
//!
//! 原档只读：`fs::copy` 打开源文件是读、目标是新建/覆盖目标，源文件一个字节都不改。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use super::ApiResult;
use crate::paths;

/// 一份备份的样子。界面只用它来显示「最近一次备份」。
#[derive(Debug, Clone, Serialize)]
pub struct SaveBackup {
    /// 备份文件名（`76561199218191195_Player_20261107-133012.sav`）
    pub name: String,
    /// 备份的大小（字节）——验收要求它和原档一致
    pub bytes: u64,
    /// 备份的时刻（unix 毫秒），界面用 `whenText` 排版
    pub at_ms: u64,
    /// 是不是还原时自动存的那份（`…_before-restore_…`）。
    /// 自动的和手动的长得一样会让用户点错：一个想回到「那天」，
    /// 一个只是「还原之前的状态」，来历不同，界面要分开列。
    pub auto: bool,
}

/// 文件名里带这个标记的，是还原时自动存的
const AUTO_MARK: &str = "_before-restore_";

/// `backups\` 目录的现状：放哪、有几份、最新的是哪份。
#[derive(Debug, Serialize)]
pub struct SaveBackupList {
    /// 目录的字面路径（界面显示，方便自己拷回去）
    pub dir: String,
    /// 里面有几份 `.sav`
    pub count: usize,
    /// 最新的一份；一份都没有时是 `null`
    pub latest: Option<SaveBackup>,
    /// 全部备份，新的在前（还原时按名字挑）
    pub items: Vec<SaveBackup>,
}

/// 还原完成后的回执：从哪份还原、替换前另存到哪、替换后的大小。
#[derive(Debug, Serialize)]
pub struct RestoredSave {
    /// 还原用的那份备份的文件名
    pub from: String,
    /// 覆盖前给当前档做的那份安全备份
    pub safety: SaveBackup,
    /// 被替换的存档文件字面路径
    pub target: String,
    /// 替换后的大小（字节）
    pub bytes: u64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(crate) fn backups_dir() -> PathBuf {
    paths::data_root().join("backups")
}

/// unix 毫秒 → `20261107-133012`（**本机时区**）。
///
/// 和 `config::date_stamp` 同一个理由：用户看到文件名时对的是自己墙上的时间。
/// 精确到秒是为了「连点两次」也能得到两个不同的名字（同秒撞上了还有 [`free_name`] 兜底）。
fn stamp(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    let t = time::OffsetDateTime::from_unix_timestamp(secs)
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH)
        .to_offset(offset);
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    )
}

/// 原档文件名 → 备份文件名的主干：`…_Player.sav` → `…_Player`。
fn stem_of(source: &Path) -> String {
    source
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "save".to_owned())
}

/// 在 `dir` 里找一个还没被占的名字：先试 `主干_时间戳.sav`，撞了就加 `-2`、`-3`……
///
/// 只读探测，不建文件——真正落盘还是 [`fs::copy`]。
fn free_name(dir: &Path, stem: &str, stamp: &str) -> PathBuf {
    let first = dir.join(format!("{stem}_{stamp}.sav"));
    if !first.exists() {
        return first;
    }
    for n in 2..1000 {
        let candidate = dir.join(format!("{stem}_{stamp}-{n}.sav"));
        if !candidate.exists() {
            return candidate;
        }
    }
    // 一千个同名前缀已经不属于「正常连点两次」了，交给调用方去报错。
    first
}

fn file_bytes(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// 一份备份「什么时候做的」。
///
/// Windows 的 `CopyFile` 会把源档的**最后写入时间**一起带过来（真实存档上那是游戏里
/// 存盘的那一刻，可能是一年前），所以不能拿 `modified()` 当备份时刻——卡片上会写着
/// 「最近一次备份：3 月 3 日」，而那次备份其实是今天做的。创建时间才是复制发生的那一秒；
/// 拿不到创建时间（非 Windows）再退回修改时间。
fn made_at(meta: &fs::Metadata) -> Option<SystemTime> {
    meta.created().or_else(|_| meta.modified()).ok()
}

/// 目录里所有 `.sav` 里最新的那份（按备份时刻）。
fn latest_in(dir: &Path) -> (usize, Option<SaveBackup>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return (0, None);
    };
    let mut count = 0usize;
    let mut newest: Option<(u64, SaveBackup)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("sav"))
        {
            continue;
        }
        count += 1;
        let meta = entry.metadata().ok();
        let at_ms = meta
            .as_ref()
            .and_then(made_at)
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let auto = name.contains(AUTO_MARK);
        if newest.as_ref().is_none_or(|(best, _)| at_ms >= *best) {
            newest = Some((at_ms, SaveBackup { name, bytes, at_ms, auto }));
        }
    }
    (count, newest.map(|(_, b)| b))
}

/// `backups\` 里的全部 `.sav`，按备份时刻从新到旧。
fn list_in(dir: &Path) -> Vec<SaveBackup> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut all: Vec<(u64, SaveBackup)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("sav"))
        {
            continue;
        }
        let meta = entry.metadata().ok();
        let at_ms = meta
            .as_ref()
            .and_then(made_at)
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let auto = name.contains(AUTO_MARK);
        all.push((at_ms, SaveBackup { name, bytes, at_ms, auto }));
    }
    all.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.name.cmp(&a.1.name)));
    all.into_iter().map(|(_, b)| b).collect()
}

/// 备份一次：把当前存档复制成 `名字_时间戳.sav`，放进 `backups\`。
#[tauri::command]
pub fn backup_save() -> ApiResult<SaveBackup> {
    let Some(file) = super::save::pick(None) else {
        return ApiResult::err(
            "not_found",
            "没找到存档：这台机器上还没存过档，或者装的是别的版本（目前只认 Steam 版）",
        );
    };

    let dir = backups_dir();
    if let Err(e) = fs::create_dir_all(&dir) {
        return ApiResult::err("io", format!("建不了备份目录 {}：{e}", dir.display()));
    }

    let at_ms = now_ms();
    let target = free_name(&dir, &stem_of(&file.path), &stamp(at_ms));
    let source_bytes = file_bytes(&file.path);
    if let Err(e) = fs::copy(&file.path, &target) {
        return ApiResult::err(
            "io",
            format!("复制存档失败（{} → {}）：{e}", file.path.display(), target.display()),
        );
    }

    // 复制完核一遍大小：字节数对不上就当这次备份没成。
    // 这个文件是上面刚建的，清掉它不会碰到任何别的东西。
    let copied = file_bytes(&target);
    if copied != source_bytes {
        let _ = fs::remove_file(&target);
        return ApiResult::err(
            "io",
            format!("备份出来是 {copied} 字节，原档是 {source_bytes} 字节，这份不算数"),
        );
    }

    // 复制过来的文件会继承原档的「最后写入时间」，在资源管理器里看着像去年的东西。
    // 备份就是现在做的，把时间改回现在；改不动也不影响卡片（卡片读创建时间）。
    let _ = fs::OpenOptions::new()
        .write(true)
        .open(&target)
        .and_then(|f| f.set_times(fs::FileTimes::new().set_modified(SystemTime::now())));

    ApiResult::ok(SaveBackup {
        name: target
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
        bytes: copied,
        at_ms,
        auto: false,
    })
}

/// 读一下 `backups\` 现在什么样（进设置页时用）。
#[tauri::command]
pub fn save_backup_info() -> ApiResult<SaveBackupList> {
    let dir = backups_dir();
    let (count, latest) = latest_in(&dir);
    ApiResult::ok(SaveBackupList {
        dir: dir.display().to_string(),
        count,
        latest,
        items: list_in(&dir),
    })
}

/// 备份文件名是否安全：只允许一个 `.sav` 文件名，不许带目录、不许带 `..`。
fn safe_backup_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && name.to_ascii_lowercase().ends_with(".sav")
        && !name.contains(['/', '\\', ':', '\0'])
        && !name.split(['/', '\\']).any(|p| p == "..")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '(' | ')'))
}

/// 还原：先用当前档备份一份，再把选中的那份替换上去。
///
/// 三步顺序是硬要求（v0.3.1 决策 #29）：列备份（界面）、点还原、覆盖前先存当前档。
/// 这里做后两步：安全备份失败就停，绝不在没备份的情况下覆盖。
#[tauri::command]
pub fn restore_backup(name: String) -> ApiResult<RestoredSave> {
    if !safe_backup_name(&name) {
        return ApiResult::err("bad_name", "备份名不像一份 .sav 文件名，没有动任何文件");
    }
    let dir = backups_dir();
    let backup = dir.join(&name);
    if !backup.is_file() {
        return ApiResult::err(
            "not_found",
            format!("备份目录里没有这一份：{}", backup.display()),
        );
    }
    let Some(current) = super::save::pick(None) else {
        return ApiResult::err(
            "not_found",
            "没找到当前存档：这台机器上还没存过档，或者装的是别的版本（目前只认 Steam 版）",
        );
    };

    // 第一步：先给当前档做安全备份，做完核对大小；不成就停，绝不覆盖。
    let at_ms = now_ms();
    let safety_path = free_name(
        &dir,
        &format!("{}_before-restore", stem_of(&current.path)),
        &stamp(at_ms),
    );
    let current_bytes = file_bytes(&current.path);
    if let Err(e) = fs::copy(&current.path, &safety_path) {
        return ApiResult::err(
            "io",
            format!(
                "先把当前档另存一份失败（{} → {}）：{e}；还没有覆盖任何东西",
                current.path.display(),
                safety_path.display()
            ),
        );
    }
    let safety_bytes = file_bytes(&safety_path);
    if safety_bytes != current_bytes {
        let _ = fs::remove_file(&safety_path);
        return ApiResult::err(
            "io",
            format!(
                "另存的当前档是 {safety_bytes} 字节、原档是 {current_bytes} 字节，算备份失败；还没有覆盖任何东西"
            ),
        );
    }
    let _ = fs::OpenOptions::new()
        .write(true)
        .open(&safety_path)
        .and_then(|f| f.set_times(fs::FileTimes::new().set_modified(SystemTime::now())));

    // 第二步：替换。备份的字节数也核一遍，免得读到半份文件。
    let from_bytes = file_bytes(&backup);
    if from_bytes == 0 {
        return ApiResult::err(
            "io",
            format!(
                "这份备份是空的（{}），没有覆盖当前档；当前档另存在 {}",
                backup.display(),
                safety_path.display()
            ),
        );
    }
    if let Err(e) = fs::copy(&backup, &current.path) {
        return ApiResult::err(
            "io",
            format!(
                "替换存档失败（{} → {}）：{e}；当前档的另一份在 {}",
                backup.display(),
                current.path.display(),
                safety_path.display()
            ),
        );
    }
    let after = file_bytes(&current.path);
    if after != from_bytes {
        return ApiResult::err(
            "io",
            format!(
                "替换完是 {after} 字节、备份是 {from_bytes} 字节，对不上；当前档的另一份在 {}",
                safety_path.display()
            ),
        );
    }

    ApiResult::ok(RestoredSave {
        from: name,
        safety: SaveBackup {
            name: safety_path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
            bytes: safety_bytes,
            at_ms,
            auto: true,
        },
        target: current.path.display().to_string(),
        bytes: after,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_names_that_could_escape_the_folder_are_refused() {
        // 正常名字放行
        assert!(safe_backup_name("76561199027062409_Player_20261107-133012.sav"));
        assert!(safe_backup_name("76561199027062409_Player_20261107-133012-2.sav"));
        // 想跳目录、想写别的扩展名、想塞路径，一律拒绝
        assert!(!safe_backup_name("../outside.sav"));
        assert!(!safe_backup_name("sub/name.sav"));
        assert!(!safe_backup_name("sub\\name.sav"));
        assert!(!safe_backup_name("C:\\save.sav"));
        assert!(!safe_backup_name("player.txt"));
        assert!(!safe_backup_name(""));
        assert!(!safe_backup_name(".."));
    }

    #[test]
    fn stamp_is_second_precise_and_sortable() {
        // 同一天里两个不同秒必须给出两个不同的名字。
        assert_ne!(stamp(0), stamp(1_000));
        // 形状固定：8 位日期 + 横线 + 6 位时间。
        let s = stamp(1_700_000_000_000);
        assert_eq!(s.len(), 15, "{s}");
        assert_eq!(&s[8..9], "-");
    }

    #[test]
    fn free_name_never_overwrites_an_existing_backup() {
        let dir = std::env::temp_dir().join(format!("rs-savebak-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("建临时目录");
        let first = free_name(&dir, "123_Player", "20261107-133012");
        fs::write(&first, b"x").expect("占位文件");

        let second = free_name(&dir, "123_Player", "20261107-133012");
        assert_ne!(first, second, "同名时不能覆盖上一份");
        assert!(second.to_string_lossy().ends_with("-2.sav"), "{second:?}");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn latest_in_picks_the_newest_sav_and_ignores_other_files() {
        let dir = std::env::temp_dir().join(format!("rs-savebak-latest-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("建临时目录");
        fs::write(dir.join("a_1.sav"), b"aa").expect("写第一份");
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(dir.join("a_2.sav"), b"bbb").expect("写第二份");
        fs::write(dir.join("note.txt"), b"n").expect("写干扰文件");

        let (count, latest) = latest_in(&dir);
        assert_eq!(count, 2, "只数 .sav");
        let latest = latest.expect("应该有一份");
        assert_eq!(latest.name, "a_2.sav");
        assert_eq!(latest.bytes, 3);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn latest_in_reports_when_the_backup_was_made_not_the_saves_own_date() {
        let root = std::env::temp_dir().join(format!("rs-savebak-at-{}", std::process::id()));
        let dir = root.join("backups");
        fs::create_dir_all(&dir).expect("建临时目录");

        // 源档的「最后写入时间」是游戏存盘那天（这里用 2023 年代替），复制会把它带过去。
        let src = root.join("123_Player.sav");
        fs::write(&src, b"abc").expect("写源档");
        let old = UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        fs::OpenOptions::new()
            .write(true)
            .open(&src)
            .expect("打开源档")
            .set_times(fs::FileTimes::new().set_modified(old))
            .expect("把源档改老");

        let dst = dir.join("123_Player_20261107-133012.sav");
        fs::copy(&src, &dst).expect("复制");
        // 备份命令会把这个时间改回现在；这里模拟同一件事，断言卡片读到的是「现在」。
        fs::OpenOptions::new()
            .write(true)
            .open(&dst)
            .expect("打开备份")
            .set_times(fs::FileTimes::new().set_modified(SystemTime::now()))
            .expect("把备份改成现在");

        let (_, latest) = latest_in(&dir);
        let latest = latest.expect("应该有一份");
        let now = now_ms();
        assert!(
            latest.at_ms + 120_000 >= now,
            "卡片要显示备份时刻（≈{now}），不能是原档的日期 {}",
            latest.at_ms
        );

        let _ = fs::remove_dir_all(&root);
    }
}
