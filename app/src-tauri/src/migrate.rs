//! 启动时把老记账迁到新格式（只跑一次）。
//!
//! v0.1 的本地 mod 的 id 是对「用户给的那个路径字符串」算出来的，于是同一个文件从两个不同
//! 位置导入会变成两条记录。v0.2 起 id 改成按**内容**算（见 [`crate::local::fingerprint`]），
//! 这里负责把老的本地记账换成新 id，并给每条记录补上内容指纹。
//!
//! 原则：
//!
//! - 动手之前先把整个 `installed\` 目录复制一份成 `installed.bak_<时间戳>\`，出问题能翻回去。
//! - 能找到原文件的：重算内容指纹，换成新 id，补上 `content_hash`。
//! - 换完发现跟另一条撞了（两条其实是同一份内容）：**合成一条**，留装得更早的那条。
//! - 找不到原文件的：**不删记录**，只标 `missing: true`，界面上标灰让用户自己决定要不要删。
//! - 跑完写一个 `migrated_v2` 标记文件，下次启动不再扫一遍（重跑结果一样）。

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ipc::mods::{load_records, record_path, records_dir_at, save_record};
use crate::local;

/// 迁移结果，写进日志用。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// 扫到的本地记账条数
    pub scanned: usize,
    /// 换过 id 的条数
    pub id_changed: usize,
    /// 只是补了指纹、id 没变的条数
    pub hash_added: usize,
    /// 找不到原文件、标成缺失的条数
    pub missing: usize,
    /// 两条记录其实是同一份内容、被合成一条的条数
    pub merged: usize,
}

impl Report {
    pub fn did_anything(&self) -> bool {
        self.scanned > 0
    }

    pub fn summary(&self) -> String {
        format!(
            "老记账迁移：扫了 {} 条本地记录，换 id {} 条，补指纹 {} 条，合并重复 {} 条，原文件缺失 {} 条",
            self.scanned, self.id_changed, self.hash_added, self.merged, self.missing
        )
    }
}

/// 标记文件：有了它就不再重复迁移。
fn marker_path(data_root: &Path) -> PathBuf {
    data_root.join("migrated_v2")
}

/// 迁移要跑吗？
pub fn needed(data_root: &Path) -> bool {
    !marker_path(data_root).exists() && records_dir_at(data_root).is_dir()
}

/// 该跑就跑；已经跑过就直接返回 `None`。
pub fn run_if_needed(data_root: &Path) -> Result<Option<Report>, String> {
    if !needed(data_root) {
        return Ok(None);
    }
    let report = run(data_root)?;
    Ok(Some(report))
}

fn run(data_root: &Path) -> Result<Report, String> {
    let dir = records_dir_at(data_root);
    let records = load_records(&dir);
    if records.is_empty() {
        write_marker(data_root)?;
        return Ok(Report::default());
    }

    back_up(data_root, &dir)?;

    let mut report = Report::default();
    for mut rec in records {
        // mod.io 的记账号码从来没变过，不碰
        if rec.source != "local" {
            continue;
        }
        report.scanned += 1;

        if rec.content_hash.is_some() {
            // 已经是新格式，不重复动
            continue;
        }

        let file = PathBuf::from(&rec.path);
        if !file.exists() {
            rec.missing = true;
            report.missing += 1;
            save_record(&dir, &rec).map_err(|e| e.message())?;
            continue;
        }

        match local::fingerprint(&file) {
            Ok(fp) => {
                if fp.id != rec.id {
                    // 换成新的 id 之前先看看那个 id 上是不是已经有一条了：
                    // 有的话说明这两条其实是同一份内容（v0.1 按路径算漏掉的重复），合成一条。
                    let existing = load_records(&dir).into_iter().find(|r| r.id == fp.id);
                    if let Some(prev) = existing {
                        let keep_prev = match (prev.installed_at, rec.installed_at) {
                            (Some(a), Some(b)) => a <= b,
                            // 老的没有时间戳：留已经在的那条，别把先装的那份挤掉
                            _ => true,
                        };
                        if keep_prev {
                            let _ = std::fs::remove_file(record_path(&dir, rec.id));
                            report.merged += 1;
                            continue;
                        }
                        let _ = std::fs::remove_file(record_path(&dir, prev.id));
                        report.merged += 1;
                    }
                    // 换 id：先删旧文件名，再按新 id 写
                    let _ = std::fs::remove_file(record_path(&dir, rec.id));
                    rec.id = fp.id;
                    report.id_changed += 1;
                } else {
                    report.hash_added += 1;
                }
                rec.content_hash = Some(fp.hash);
                save_record(&dir, &rec).map_err(|e| e.message())?;
            }
            Err(e) => {
                // 指纹算不出来（文件读不了之类）：标缺失，别把记录弄丢
                rec.missing = true;
                report.missing += 1;
                save_record(&dir, &rec).map_err(|e| e.message())?;
                tracing::warn!("迁移时算指纹失败（{}）：{e}", rec.path);
            }
        }
    }

    write_marker(data_root)?;
    Ok(report)
}

/// 迁移前整目录复制一份，名字带时间戳。
fn back_up(data_root: &Path, dir: &Path) -> Result<PathBuf, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup = data_root
        .join("mods")
        .join(format!("installed.bak_{stamp}"));
    copy_dir(dir, &backup)?;
    Ok(backup)
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("建备份目录失败：{e}"))?;
    let entries = std::fs::read_dir(from).map_err(|e| format!("读记账目录失败：{e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let name = match path.file_name() {
                Some(n) => n,
                None => continue,
            };
            std::fs::copy(&path, to.join(name)).map_err(|e| format!("备份记账失败：{e}"))?;
        }
    }
    Ok(())
}

fn write_marker(data_root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(data_root).map_err(|e| format!("建数据目录失败：{e}"))?;
    std::fs::write(marker_path(data_root), b"v2\n").map_err(|e| format!("写迁移标记失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::mods::InstalledRecord;

    fn test_root(name: &str) -> PathBuf {
        let base = std::env::var("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("target"));
        let dir = base.join("rock-sprout-tests").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建测试目录");
        dir
    }

    fn legacy(rec: InstalledRecord) -> InstalledRecord {
        rec
    }

    #[test]
    fn 老本地记账会换成按内容算的新id() {
        let root = test_root("migrate-id");
        let dir = records_dir_at(&root);
        std::fs::create_dir_all(&dir).expect("建记账目录");

        let pak = root.join("mods").join("local").join("旧文件-123456.pak");
        std::fs::create_dir_all(pak.parent().unwrap()).expect("建目录");
        std::fs::write(&pak, b"real-pak-content").expect("写文件");
        let want = local::fingerprint(&pak).expect("算指纹");

        let old = legacy(InstalledRecord {
            id: 0x0011_1111_1111_1111,
            name: "旧文件".to_owned(),
            file_id: 0,
            filename: "旧文件-123456.pak".to_owned(),
            size: 16,
            path: pak.display().to_string(),
            source: "local".to_owned(),
            original_path: None,
            content_hash: None,
            installed_at: None,
            kind: "pak".to_owned(),
            missing: false,
        });
        save_record(&dir, &old).expect("写老记账");

        let report = run_if_needed(&root).expect("迁移").expect("应该跑一次");
        assert_eq!(report.scanned, 1);
        assert_eq!(report.id_changed, 1);
        assert_eq!(report.missing, 0);

        let after = load_records(&dir);
        assert_eq!(after.len(), 1, "迁移后应该还是一条");
        assert_eq!(after[0].id, want.id, "id 要换成按内容算的");
        assert_eq!(after[0].content_hash.as_deref(), Some(want.hash.as_str()));
        assert!(!record_path(&dir, old.id).exists(), "老记录文件要清掉");

        // 第二次启动不再跑
        assert!(run_if_needed(&root).expect("第二次").is_none());
    }

    #[test]
    fn 两条老记录其实是同一份内容时合成一条留更早的() {
        let root = test_root("migrate-merge");
        let dir = records_dir_at(&root);
        std::fs::create_dir_all(&dir).expect("建记账目录");

        // 同一个 pak 被复制到两个名字下——v0.1 按路径算 id，会留下两条
        let a = root.join("mods").join("local").join("甲-111111.pak");
        let b = root.join("mods").join("local").join("乙-222222.pak");
        std::fs::create_dir_all(a.parent().unwrap()).expect("建目录");
        std::fs::write(&a, b"same-bytes").expect("写甲");
        std::fs::write(&b, b"same-bytes").expect("写乙");
        let want = local::fingerprint(&a).expect("算指纹");

        let mut first = legacy(InstalledRecord {
            id: 0x0014_4444_4444_4444,
            name: "甲".to_owned(),
            file_id: 0,
            filename: "甲-111111.pak".to_owned(),
            size: 10,
            path: a.display().to_string(),
            source: "local".to_owned(),
            original_path: None,
            content_hash: None,
            kind: "pak".to_owned(),
            installed_at: Some(1_000),
            missing: false,
        });
        save_record(&dir, &first).expect("写甲");
        first.id = 0x0015_5555_5555_5555;
        first.name = "乙".to_owned();
        first.filename = "乙-222222.pak".to_owned();
        first.path = b.display().to_string();
        first.installed_at = Some(2_000);
        save_record(&dir, &first).expect("写乙");

        let report = run_if_needed(&root).expect("迁移").expect("应该跑一次");
        assert_eq!(report.scanned, 2);
        assert_eq!(report.merged, 1, "两条撞在一起要合成一条");

        let after = load_records(&dir);
        assert_eq!(after.len(), 1, "同一份内容只该留一条");
        assert_eq!(after[0].id, want.id);
        assert_eq!(after[0].name, "甲", "留装得更早的那条");
        assert_eq!(after[0].installed_at, Some(1_000));
    }

    #[test]
    fn 找不到原文件的记录会标成缺失但不删掉() {
        let root = test_root("migrate-missing");
        let dir = records_dir_at(&root);
        std::fs::create_dir_all(&dir).expect("建记账目录");

        let old = legacy(InstalledRecord {
            id: 0x0012_2222_2222_2222,
            name: "已经删掉的".to_owned(),
            file_id: 0,
            filename: "已经删掉的-654321.pak".to_owned(),
            size: 1,
            path: root.join("没有这个文件.pak").display().to_string(),
            source: "local".to_owned(),
            original_path: None,
            kind: "pak".to_owned(),
            content_hash: None,
            installed_at: None,
            missing: false,
        });
        save_record(&dir, &old).expect("写老记账");

        let report = run_if_needed(&root).expect("迁移").expect("应该跑一次");
        assert_eq!(report.missing, 1);

        let after = load_records(&dir);
        assert_eq!(after.len(), 1, "找不到文件也不能把记录删了");
        assert!(after[0].missing, "要标成缺失");
        assert_eq!(after[0].id, old.id, "缺失记录保留原 id，用户自己决定要不要删");
    }

    #[test]
    fn 迁移前会把记账目录整个备份一份() {
        let root = test_root("migrate-backup");
        let dir = records_dir_at(&root);
        std::fs::create_dir_all(&dir).expect("建记账目录");
        let old = legacy(InstalledRecord {
            id: 0x0013_3333_3333_3333,
            name: "备份用".to_owned(),
            file_id: 0,
            filename: "备份用-111111.pak".to_owned(),
            size: 1,
            path: root.join("没有.pak").display().to_string(),
            source: "local".to_owned(),
            kind: "pak".to_owned(),
            original_path: None,
            content_hash: None,
            installed_at: None,
            missing: false,
        });
        save_record(&dir, &old).expect("写老记账");

        run_if_needed(&root).expect("迁移").expect("应该跑一次");
        let backups: Vec<_> = std::fs::read_dir(root.join("mods"))
            .expect("读 mods 目录")
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("installed.bak_")
            })
            .collect();
        assert_eq!(backups.len(), 1, "应该有一个带时间戳的备份目录");
    }

    #[test]
    fn modio的记账完全不碰() {
        let root = test_root("migrate-modio");
        let dir = records_dir_at(&root);
        std::fs::create_dir_all(&dir).expect("建记账目录");
        let rec = InstalledRecord {
            id: 1179441,
            name: "Brighter Objects".to_owned(),
            file_id: 1,
            filename: "brighterobjects1123-fyin.zip".to_owned(),
            size: 462_029,
            path: root.join("no-such.zip").display().to_string(),
            kind: "pak".to_owned(),
            source: "modio".to_owned(),
            original_path: None,
            content_hash: None,
            installed_at: None,
            missing: false,
        };
        save_record(&dir, &rec).expect("写记账");

        let report = run_if_needed(&root).expect("迁移").expect("应该跑一次");
        assert_eq!(report.scanned, 0, "mod.io 的记账不算在迁移范围里");

        let after = load_records(&dir);
        assert_eq!(after[0].id, 1179441);
        assert!(!after[0].missing, "mod.io 的记录即使文件不在也不该被标缺失");
        assert!(after[0].content_hash.is_none());
    }
}
