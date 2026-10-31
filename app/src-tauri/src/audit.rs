//! 体检：把「该在的不在、不该在的还在」一次列清楚。
//!
//! 检查五件事（决定 #8 只做 a + b 两类）：
//!
//! | 类 | 查什么 | 能处理吗 |
//! |---|---|---|
//! | a1 | `mods_P.pak.tmp` / `.prev` 残留 | 能，清掉 |
//! | a2 | 加载器 DLL 三态（不在 / 是我们的 / 是游戏原版） | 只报 |
//! | a3 | 游戏原版 DLL 的备份还在不在 | 只报（没了就还原不回去） |
//! | b1 | 记账里有、磁盘上没文件 | 能，「只删记录」 |
//! | b2 | 磁盘上有下载文件、记账里没有 | 能，挪进备份目录 |
//!
//! 除了「清残留」和「挪孤儿文件」，这个模块**不删任何东西**。

use std::path::{Path, PathBuf};

use crate::game;
use crate::integrate;
use crate::ipc::mods;

/// 一条体检发现。
#[derive(Debug, Clone, serde::Serialize)]
pub struct Issue {
    /// 分类：`residue` / `dll_missing` / `dll_stock` / `dll_backup_lost` / `record_missing` / `orphan`。
    pub kind: String,
    /// 一句话说清是什么问题。
    pub title: String,
    /// 补充说明（路径、数量、后果）。
    pub detail: String,
    /// 能点一下处理掉吗。
    pub fixable: bool,
    /// 处理动作：`clean_residue` / `drop_record` / `clean_orphans`；不能处理就是空串。
    pub action: String,
    /// 处理哪个目标（记录 id 或文件路径）；`clean_residue` / `clean_orphans` 不需要，给空串。
    pub target: String,
}

/// 一次体检的结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuditReport {
    /// 找到游戏了吗。
    pub has_game: bool,
    /// 游戏目录（Paks 的上一级），找不到就是空串。
    pub game_dir: String,
    /// 已装几个 mod。
    pub mods: usize,
    /// 合并包在不在。
    pub merged_pak: bool,
    /// 发现的问题，能处理的排在前面。
    pub issues: Vec<Issue>,
    /// 一句话结论。
    pub summary: String,
}

/// 体检入口：扫一遍现在的状态。
pub fn audit() -> AuditReport {
    audit_at(&crate::paths::data_root())
}

fn audit_at(data_root: &Path) -> AuditReport {
    let installation = game::find_installation();
    let game_dir = installation
        .as_ref()
        .map(|i| i.root.display().to_string())
        .unwrap_or_default();
    let paks = installation.as_ref().map(|i| i.paks_path());

    let records = mods::load_records(&mods::records_dir_at(data_root));
    let merged_pak = paks
        .as_ref()
        .map(|p| p.join("mods_P.pak").exists())
        .unwrap_or(false);

    let mut issues = Vec::new();

    // a1 · 残留的临时文件
    if let Some(paks) = paks.as_ref() {
        let (pak, tmp, prev) = integrate::pak_paths(paks);
        let mut leftover = Vec::new();
        if tmp.exists() {
            leftover.push(tmp.display().to_string());
        }
        if prev.exists() {
            leftover.push(prev.display().to_string());
        }
        if !leftover.is_empty() {
            issues.push(Issue {
                kind: "residue".to_owned(),
                title: "上次合并没做完，留下了临时文件".to_owned(),
                detail: format!(
                    "残留：{}。它们不影响玩游戏，但会占地方；点一下就能清掉。正式包 {}。",
                    leftover.join("、"),
                    if pak.exists() { "还在" } else { "现在没有" }
                ),
                fixable: true,
                action: "clean_residue".to_owned(),
                target: String::new(),
            });
        }
    }

    // a2 / a3 · 加载器 DLL
    match game::dll_status_at(data_root) {
        Some(dll) => {
            match dll.state {
                "missing" => issues.push(Issue {
                    kind: "dll_missing".to_owned(),
                    title: "游戏目录里没有加载器文件".to_owned(),
                    detail: format!(
                        "找不到 {}。现在应该也没装 mod；下次装的时候会自动写回去。",
                        dll.path
                    ),
                    fixable: false,
                    action: String::new(),
                    target: String::new(),
                }),
                "ours" => {
                    if !dll.backup_exists {
                        issues.push(Issue {
                            kind: "dll_backup_lost".to_owned(),
                            title: "游戏原本的加载器没存副本".to_owned(),
                            detail: format!(
                                "现在用的是我们写的加载器（{}），但没找到游戏原版那份的备份。\
                                 这不影响玩，只是这个版本里卸载不会自动换回原版——\
                                 想恢复可以在 Steam 里「验证游戏文件完整性」。",
                                dll.path
                            ),
                            fixable: false,
                            action: String::new(),
                            target: String::new(),
                        });
                    }
                }
                _ => issues.push(Issue {
                    kind: "dll_stock".to_owned(),
                    title: "加载器是游戏原版（没装 mod 时正常）".to_owned(),
                    detail: format!("{} 还是游戏自带的那份。", dll.path),
                    fixable: false,
                    action: String::new(),
                    target: String::new(),
                }),
            }
        }
        None => {}
    }

    // b1 · 记账里有、文件没了
    for rec in records.iter().filter(|r| !Path::new(&r.path).exists()) {
        issues.push(Issue {
            kind: "record_missing".to_owned(),
            title: format!("「{}」的文件不见了", rec.name),
            detail: format!(
                "记录还在，但磁盘上找不到 {}。多半是手动删了下载目录。\
                 这条记录可以留着（下次装同名的会接上），也可以只删记录。",
                rec.path
            ),
            fixable: true,
            action: "drop_record".to_owned(),
            target: rec.id.to_string(),
        });
    }

    // b2 · 磁盘上有文件、记账里没有
    let orphans = find_orphans(data_root, &records);
    if !orphans.is_empty() {
        issues.push(Issue {
            kind: "orphan".to_owned(),
            title: format!("有 {} 个文件没人认领", orphans.len()),
            detail: format!(
                "下载目录里这些文件不在任何一条已装记录里，合并不用它们：{}。\
                 点一下会挪进备份目录（不删，随时能拿回来）。",
                orphans
                    .iter()
                    .take(5)
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join("、")
                    + if orphans.len() > 5 { "…" } else { "" }
            ),
            fixable: true,
            action: "clean_orphans".to_owned(),
            target: String::new(),
        });
    }

    // c1 · 游戏目录里不认识的 pak（多半是别的工具留下的）
    if let Some(installation) = installation.as_ref() {
        let strays = find_stray_paks(&installation.paks_path(), &installation.main_pak());
        if !strays.is_empty() {
            issues.push(Issue {
                kind: "stray".to_owned(),
                title: format!("游戏目录里有 {} 个不认识的文件", strays.len()),
                detail: format!(
                    "这些不是岩笋写的，也不是游戏自己的：{}。\
                     它们可能是别的工具留下的，游戏会一起加载——想干净就把它们挪进备份目录（不删）。",
                    strays
                        .iter()
                        .map(|p| {
                            p.file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| p.display().to_string())
                        })
                        .collect::<Vec<_>>()
                        .join("、")
                ),
                fixable: true,
                action: "move_stray".to_owned(),
                target: String::new(),
            });
        }
    }

    let fixable = issues.iter().filter(|i| i.fixable).count();
    let summary = if issues.is_empty() {
        format!(
            "一切正常：{} 个 mod，合并包{}。",
            records.len(),
            if merged_pak { "在" } else { "还没有" }
        )
    } else {
        format!(
            "{} 个 mod，合并包{}；发现 {} 处情况，其中 {} 处能一键处理。",
            records.len(),
            if merged_pak { "在" } else { "还没有" },
            issues.len(),
            fixable
        )
    };

    AuditReport {
        has_game: installation.is_some(),
        game_dir,
        mods: records.len(),
        merged_pak,
        issues,
        summary,
    }
}

/// 处理一条体检发现：`action` 来自体检报告，`target` 跟着一起传。
pub fn perform(action: &str, target: &str) -> Result<String, String> {
    perform_at(&crate::paths::data_root(), action, target)
}

fn perform_at(data_root: &Path, action: &str, target: &str) -> Result<String, String> {
    match action {
        // 清残留：只动 .tmp / .prev，正式包不会碰
        "clean_residue" => {
            let installation =
                game::find_installation().ok_or_else(|| "没找到《深岩银河》的安装位置".to_owned())?;
            let n = integrate::clean_stale(&installation.paks_path())
                .map_err(|e| format!("清残留失败：{e}"))?;
            Ok(format!("清掉了 {n} 个临时文件。"))
        }
        // 只删记录：文件本来就不在了，所以不动磁盘
        "drop_record" => {
            let id: u64 = target
                .parse()
                .map_err(|_| format!("记不住这个 id：{target}"))?;
            crate::ipc::maintenance::delete_record_at(data_root, id)
                .map(|name| format!("「{name}」的记录已经删掉了。"))
                .map_err(|(_, message)| message)
        }
        // 孤儿文件：挪进备份目录，不删
        "clean_orphans" => {
            let records = mods::load_records(&mods::records_dir_at(data_root));
            let orphans = find_orphans(data_root, &records);
            if orphans.is_empty() {
                return Ok("没有要清理的文件。".to_owned());
            }
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let dest = data_root
                .join("game-backup")
                .join("stray")
                .join(format!("orphan-{stamp}"));
            std::fs::create_dir_all(&dest).map_err(|e| format!("建备份目录失败：{e}"))?;

            let mut moved = 0usize;
            for path in &orphans {
                let Some(name) = path.file_name() else {
                    continue;
                };
                let to = dest.join(name);
                if std::fs::rename(path, &to).is_ok() {
                    moved += 1;
                }
            }
            Ok(format!(
                "把 {moved} 个文件挪到了 {}（想拿回来从这里拿）。",
                dest.display()
            ))
        }
        // 陌生产物：挪进备份目录，不删
        "move_stray" => {
            let installation =
                game::find_installation().ok_or_else(|| "没找到《深岩银河》的安装位置".to_owned())?;
            let strays = find_stray_paks(&installation.paks_path(), &installation.main_pak());
            if strays.is_empty() {
                return Ok("没有要挪的文件。".to_owned());
            }
            let dest = stray_dir(data_root);
            std::fs::create_dir_all(&dest).map_err(|e| format!("建备份目录失败：{e}"))?;
            let mut moved = 0usize;
            for path in &strays {
                let Some(name) = path.file_name() else {
                    continue;
                };
                if std::fs::rename(path, dest.join(name)).is_ok() {
                    moved += 1;
                }
            }
            Ok(format!(
                "把 {moved} 个文件挪到了 {}（想拿回来从这里拿）。",
                dest.display()
            ))
        }
        other => Err(format!("不认识的处理动作：{other}")),    }
}

/// 下载目录里、不属于任何一条记账的文件。
fn find_orphans(data_root: &Path, records: &[mods::InstalledRecord]) -> Vec<PathBuf> {
    let known: Vec<String> = records.iter().map(|r| norm(&r.path)).collect();
    let root = data_root.join("mods");
    let mut found = Vec::new();
    collect_files(&root, &mut found, 0);
    found
        .into_iter()
        .filter(|p| {
            let text = p.to_string_lossy();
            let key = norm(&text);
            let is_mod_file = key.ends_with(".pak")
                || key.ends_with(".zip")
                || key.ends_with(".rar")
                || key.ends_with(".7z");
            is_mod_file && !known.contains(&key)
        })
        .collect()
}

/// 比路径用的统一写法：斜杠一律正斜杠、字母一律小写（Windows 不区分大小写）。
fn norm(path: &str) -> String {
    path.replace('\\', "/").to_lowercase()
}

/// 陌生产物挪到哪：`<数据目录>/game-backup/stray/stray-<秒级时间戳>/`。
fn stray_dir(data_root: &Path) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    data_root
        .join("game-backup")
        .join("stray")
        .join(format!("stray-{stamp}"))
}

/// 游戏 Paks 目录里，既不是游戏主包、也不是我们写的合并包的那些 `*.pak`。
fn find_stray_paks(paks_dir: &Path, main_pak: &Path) -> Vec<PathBuf> {
    let keep: Vec<String> = vec![
        norm(&main_pak.to_string_lossy()),
        norm(&paks_dir.join("mods_P.pak").to_string_lossy()),
    ];
    let Ok(entries) = std::fs::read_dir(paks_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let key = norm(&p.to_string_lossy());
            p.is_file() && key.ends_with(".pak") && !keep.contains(&key)
        })
        .collect()
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 3 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out, depth + 1);
        } else {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rock-sprout-audit-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn record(id: u64, path: &Path) -> mods::InstalledRecord {
        mods::InstalledRecord {
            id,
            name: format!("mod-{id}"),
            file_id: 0,
            filename: "x.zip".to_owned(),
            size: 1,
            path: path.to_string_lossy().to_string(),
            source: "local".to_owned(),
            original_path: None,
            content_hash: None,
            missing: false,
            installed_at: Some(1),
            kind: "pak".to_owned(),
        }
    }

    #[test]
    fn 记录有文件没了的会被列出来() {
        let root = temp_root("missing");
        let gone = root.join("mods/gone.zip");
        mods::save_record(&mods::records_dir_at(&root), &record(7, &gone)).unwrap();

        let report = audit_at(&root);
        let hit = report
            .issues
            .iter()
            .find(|i| i.kind == "record_missing")
            .expect("应该报出「文件没了」");
        assert_eq!(hit.target, "7");
        assert!(hit.fixable);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn 没人认领的文件会被列出来() {
        let root = temp_root("orphan");
        let dir = root.join("mods");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("nobody.zip"), b"x").unwrap();

        let report = audit_at(&root);
        let hit = report
            .issues
            .iter()
            .find(|i| i.kind == "orphan")
            .expect("应该报出孤儿文件");
        assert!(hit.detail.contains("nobody.zip"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn 清理孤儿文件是挪走不是删掉() {
        let root = temp_root("move");
        let dir = root.join("mods");
        fs::create_dir_all(&dir).unwrap();
        let stray = dir.join("nobody.zip");
        fs::write(&stray, b"x").unwrap();

        let msg = perform_at(&root, "clean_orphans", "").unwrap();
        assert!(msg.contains("挪到了"), "结论不对：{msg}");
        assert!(!stray.exists(), "原位置应该清空");
        let moved = find_orphans(&root, &[]);
        assert!(moved.is_empty(), "备份目录里的文件不该再算孤儿");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn 已经登记的文件不算孤儿() {
        let root = temp_root("known");
        let dir = root.join("mods");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("known.zip");
        fs::write(&file, b"x").unwrap();
        mods::save_record(&mods::records_dir_at(&root), &record(1, &file)).unwrap();

        let report = audit_at(&root);
        assert!(
            !report.issues.iter().any(|i| i.kind == "orphan"),
            "登记过的文件被当成孤儿了"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn 空数据目录里没有能修的问题() {
        let root = temp_root("clean");
        let report = audit_at(&root);
        // 只关心「和数据目录有关」的那几类；加载器状态、游戏目录里别家工具留下的包
        // 都是这台机器的真实情况，跟传进来的数据目录无关。
        let mine: Vec<_> = report
            .issues
            .iter()
            .filter(|i| matches!(i.kind.as_str(), "record_missing" | "orphan" | "residue"))
            .collect();
        assert!(mine.is_empty(), "空目录不该有这些问题：{mine:?}");
        assert!(report.summary.contains("0 个 mod"), "结论不对：{}", report.summary);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn 游戏目录里别家工具的包算陌生产物() {
        let dir = temp_root("stray");
        let main = dir.join("FSD-WindowsNoEditor.pak");
        fs::write(&main, b"game").unwrap();
        fs::write(dir.join("mods_P.pak"), b"ours").unwrap();
        fs::write(dir.join("FSD-WindowsNoEditor_Mods.pak"), b"other tool").unwrap();

        let found = find_stray_paks(&dir, &main);
        assert_eq!(found.len(), 1, "应该只挑出那一个：{found:?}");
        assert!(found[0].to_string_lossy().contains("_Mods.pak"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
