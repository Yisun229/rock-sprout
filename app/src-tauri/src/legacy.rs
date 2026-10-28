//! 认出「旧版本留下的数据」，并把它复制过来。
//!
//! 为什么需要：v0.1.3 之前，数据一定在 exe 旁边的 `data\`。这一版开始，装在 Program Files
//! 里时会退到用户目录写数据（见 `paths.rs`）。那么装过老版本的人升级之后，
//! 新版本读的是一个空目录，看着像「我的 mod 全没了」。
//!
//! 这里的做法很克制：**只找、只复制，不动旧目录一个字节**。用户不想要，删掉现在的 `data\`
//! 就能回到原样。

use std::path::{Path, PathBuf};

/// 一份被认出来的旧数据。
#[derive(Debug, Clone, serde::Serialize)]
pub struct Candidate {
    /// 旧数据目录的绝对路径
    pub path: String,
    /// 里面有几个 mod（`mods\installed\*.json` 的份数）
    pub mods: usize,
    /// 里面有几份快照
    pub snapshots: usize,
    /// 有没有存令牌（沿用了就不用重新粘）
    pub has_token: bool,
}

/// 认过就不再问的记号文件名（写在**当前**数据目录里）。
pub const MARKER: &str = "data-adopted.txt";

/// 这个目录里算不算「真数据」：要么有记账，要么有令牌。空目录不算。
pub fn is_real_data(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    if dir.join("modio-token.txt").is_file() {
        return true;
    }
    count_files(&dir.join("mods").join("installed"), "json") > 0
}

/// 数一个目录里指定后缀的文件有几个（不递归）。
fn count_files(dir: &Path, ext: &str) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some(ext))
        .count()
}

/// 数一个目录下面有几个子目录。
fn count_dirs(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries.flatten().filter(|e| e.path().is_dir()).count()
}

/// 把候选目录做成一份可直接给界面的描述。
fn describe(dir: &Path) -> Candidate {
    Candidate {
        path: dir.display().to_string(),
        mods: count_files(&dir.join("mods").join("installed"), "json"),
        snapshots: count_dirs(&dir.join("snapshots")),
        has_token: dir.join("modio-token.txt").is_file(),
    }
}

/// 环境变量给出的候选目录（`%LOCALAPPDATA%\…`、`%PROGRAMFILES%\…` 这些）。
fn scan_roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut push = |base: Option<std::ffi::OsString>, rest: &[&str]| {
        let Some(base) = base else { return };
        if base.is_empty() {
            return;
        }
        let mut p = PathBuf::from(base);
        for part in rest {
            p.push(part);
        }
        out.push(p);
    };

    let local = std::env::var_os("LOCALAPPDATA");
    push(local.clone(), &["RockSprout", "data"]);
    push(local, &["Programs", "RockSprout", "data"]);
    push(std::env::var_os("PROGRAMFILES"), &["RockSprout", "data"]);
    push(std::env::var_os("PROGRAMFILES(X86)"), &["RockSprout", "data"]);
    out
}

/// 扫描旧数据。
///
/// 只在「当前数据目录还是空的」「还没认过/拒过」时才真的去扫——这两个条件不满足，
/// 用户要么已经有数据了，要么已经做过选择，都不该再弹一次。
pub fn detect(data_root: &Path) -> Option<Candidate> {
    detect_with(data_root, &scan_roots())
}

/// 同 [`detect`]，但候选目录由调用方给（方便测试，不去读环境变量）。
pub fn detect_with(data_root: &Path, roots: &[PathBuf]) -> Option<Candidate> {
    // 已经决定过（沿用或不用）就不再问
    if data_root.join(MARKER).exists() {
        return None;
    }
    // 当前已经有自己的数据了，也不问
    if is_real_data(data_root) || count_dirs(&data_root.join("snapshots")) > 0 {
        return None;
    }

    let ours = normalize(data_root);
    for root in roots {
        let key = normalize(root);
        if key == ours {
            continue;
        }
        if is_real_data(root) {
            return Some(describe(root));
        }
    }
    None
}

/// 比较路径用的规范化：转小写、统一斜杠、去掉结尾的分隔符。
fn normalize(p: &Path) -> String {
    let mut s = p.to_string_lossy().replace('\\', "/").to_lowercase();
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    s
}

/// 用户的选择。
pub enum Decision {
    /// 把旧数据复制过来
    Adopt,
    /// 不沿用，从空的开始
    Decline,
}

/// 当前毫秒时间戳（记号与备份名都用它）。
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 执行选择：沿用就把旧数据整目录复制过来，然后写记号（写过就不再问）。
///
/// 步骤固定，顺序不能换：
/// 1. 当前目录非空 → 先复制成 `data.bak_<时间戳>` 留底
/// 2. 把旧目录里的东西逐个复制进当前目录（跳过已存在的同名文件，不覆盖）
/// 3. 写 `data-adopted.txt`，记下来源、时间和选择
///
/// 失败时返回错误信息，并把备份位置带出来——用户不会因为一次失败就丢东西。
pub fn apply(data_root: &Path, candidate: &Path, decision: Decision) -> Result<String, String> {
    let stamp = now_ms();
    match decision {
        Decision::Decline => {
            let text = format!(
                "declined\n来源：{}\n时间：{}\n",
                candidate.display(),
                stamp
            );
            std::fs::write(data_root.join(MARKER), text)
                .map_err(|e| format!("没写成记号文件：{e}"))?;
            Ok("好的，从空白的开始。要改用旧数据，删掉现在的 data 文件夹再启动一次就行。".to_owned())
        }
        Decision::Adopt => {
            if !is_real_data(candidate) {
                return Err("那个位置已经找不到旧数据了".to_owned());
            }

            // 1. 留底（当前目录空就跳过）
            let mut backup_note = String::new();
            if has_anything(data_root) {
                let backup = data_root.with_file_name(format!("data.bak_{stamp}"));
                copy_dir(data_root, &backup)
                    .map_err(|e| format!("备份现在的数据失败：{e}"))?;
                backup_note = format!("，原来的数据留了一份在 {}", backup.display());
            }

            // 2. 整目录复制旧数据过来
            if let Err(e) = copy_into(candidate, data_root) {
                return Err(format!(
                    "复制旧数据时出错：{e}{}",
                    if backup_note.is_empty() {
                        String::new()
                    } else {
                        format!("（你原来的数据还在 {}）", data_root.with_file_name(format!("data.bak_{stamp}")).display())
                    }
                ));
            }

            // 3. 写记号
            let text = format!(
                "adopted\n来源：{}\n时间：{}\n",
                candidate.display(),
                stamp
            );
            std::fs::write(data_root.join(MARKER), text)
                .map_err(|e| format!("旧数据复制好了，但没写成记号文件：{e}"))?;

            let c = describe(data_root);
            Ok(format!(
                "沿用好了：{} 个 mod、{} 份快照{}{}",
                c.mods,
                c.snapshots,
                if c.has_token { "，令牌也在" } else { "" },
                backup_note
            ))
        }
    }
}

/// 这个目录里有没有东西（用来决定要不要备份）。
fn has_anything(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|mut it| it.next().is_some())
        .unwrap_or(false)
}

/// 递归复制一个目录到另一个位置（目标是新建的）。
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.flatten() {
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

/// 把 `from` 里的内容复制进 `to`：目录照建，文件已存在就跳过（不覆盖现在的）。
fn copy_into(from: &Path, to: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(from)?.flatten() {
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            std::fs::create_dir_all(&dst)?;
            copy_into(&src, &dst)?;
        } else if !dst.exists() {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rock-sprout-legacy-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_old(dir: &Path, mods: usize) {
        std::fs::create_dir_all(dir.join("mods/installed")).unwrap();
        std::fs::create_dir_all(dir.join("snapshots/a")).unwrap();
        for i in 0..mods {
            std::fs::write(dir.join(format!("mods/installed/{i}.json")), b"{}").unwrap();
        }
        std::fs::write(dir.join("modio-token.txt"), "t").unwrap();
    }

    #[test]
    fn 空目录不算真数据() {
        let dir = temp("empty");
        assert!(!is_real_data(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 有记账或有令牌就算真数据() {
        let dir = temp("real");
        std::fs::create_dir_all(dir.join("mods/installed")).unwrap();
        std::fs::write(dir.join("mods/installed/1.json"), b"{}").unwrap();
        assert!(is_real_data(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 当前目录为空时会认出旧数据并报对数字() {
        let cur = temp("cur");
        let old = temp("old");
        make_old(&old, 3);
        let found = detect_with(&cur, std::slice::from_ref(&old)).expect("应该认出来");
        assert_eq!(found.mods, 3);
        assert_eq!(found.snapshots, 1);
        assert!(found.has_token);
        let _ = std::fs::remove_dir_all(&cur);
        let _ = std::fs::remove_dir_all(&old);
    }

    #[test]
    fn 当前目录已经有数据就不问() {
        let cur = temp("cur2");
        let old = temp("old2");
        make_old(&cur, 1);
        make_old(&old, 2);
        assert!(detect_with(&cur, std::slice::from_ref(&old)).is_none());
        let _ = std::fs::remove_dir_all(&cur);
        let _ = std::fs::remove_dir_all(&old);
    }

    #[test]
    fn 选了不用之后不再提示() {
        let cur = temp("cur3");
        let old = temp("old3");
        make_old(&old, 2);
        assert!(detect_with(&cur, std::slice::from_ref(&old)).is_some());
        apply(&cur, &old, Decision::Decline).unwrap();
        assert!(detect_with(&cur, std::slice::from_ref(&old)).is_none());
        let text = std::fs::read_to_string(cur.join(MARKER)).unwrap();
        assert!(text.starts_with("declined"));
        let _ = std::fs::remove_dir_all(&cur);
        let _ = std::fs::remove_dir_all(&old);
    }

    #[test]
    fn 沿用会把旧数据整目录复制过来且不动旧目录() {
        let cur = temp("cur4");
        let old = temp("old4");
        make_old(&old, 4);
        let before: Vec<_> = std::fs::read_dir(&old)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        let msg = apply(&cur, &old, Decision::Adopt).unwrap();
        assert!(msg.contains("4 个 mod"), "话术不对：{msg}");
        assert!(is_real_data(&cur));
        assert_eq!(count_files(&cur.join("mods/installed"), "json"), 4);
        // 旧目录原件还在，内容没被动过
        let after: Vec<_> = std::fs::read_dir(&old)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(before.len(), after.len());
        assert!(old.join("modio-token.txt").exists());
        let _ = std::fs::remove_dir_all(&cur);
        let _ = std::fs::remove_dir_all(&old);
    }

    #[test]
    fn 沿用前会把当前数据留一份底() {
        let cur = temp("cur5");
        let old = temp("old5");
        std::fs::write(cur.join("modio-token.txt"), "new").unwrap();
        make_old(&old, 1);
        let parent = cur.parent().unwrap().to_path_buf();
        let msg = apply(&cur, &old, Decision::Adopt).unwrap();
        assert!(msg.contains("data.bak_"), "没提到备份：{msg}");
        let backups: Vec<_> = std::fs::read_dir(&parent)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("data.bak_"))
            .collect();
        assert!(!backups.is_empty(), "没找到备份目录");
        for b in backups {
            let _ = std::fs::remove_dir_all(b.path());
        }
        let _ = std::fs::remove_dir_all(&cur);
        let _ = std::fs::remove_dir_all(&old);
    }
}
