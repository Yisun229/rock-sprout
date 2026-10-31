//! 本地 mod：用户手头已经有现成文件（网上自己下的、别人拷来的），不需要走 mod.io。
//!
//! 三种输入都能用：
//!
//! - **单个 `.pak`**：直接拿来用（整合逻辑认这种）。
//! - **装着 `.pak` 的 `.zip`**：也直接用——整合逻辑会自己把 zip 里那个 pak 抽出来。
//! - **解包后的文件夹**：找到文件夹里的 `.pak`；一个都没有时，把整个文件夹打成一个 pak。
//!
//! 不管哪种，最后都会**复制一份到我们自己的 mods 目录**再装。这样用户之后把原文件删了、
//! 移动了，已经装进游戏的东西不受影响，卸载也还能正常找回记账。

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// 本地 mod 的记账 id 从这里起步（`2^52`），往上到 `2^53` 为止。
pub const LOCAL_ID_FLOOR: u64 = 0x0010_0000_0000_0000;

/// 认下来的一个本地 mod。
#[derive(Debug, Clone)]
pub struct LocalMod {
    /// 显示用名字：文件名去掉扩展名（文件夹打成的包用文件夹名）。
    pub name: String,
    /// 复制到我们 mods 目录之后的位置，交给整合逻辑的就是它。
    pub path: PathBuf,
    /// 记账 id（按内容算，见 [`fingerprint`]）。
    pub id: u64,
    /// 内容指纹的十六进制串，跟 `id` 同源，记账里存一份方便排查。
    pub hash: String,
}

/// 一份东西的内容指纹。
#[derive(Debug, Clone)]
pub struct Fingerprint {
    /// sha256 十六进制（文件夹是把每个文件的名字和内容汇总起来算的）。
    pub hash: String,
    /// 记账 id：从指纹里取，落在 `[2^52, 2^53)`。
    pub id: u64,
}

/// 把一段哈希塞进本地 id 的区间里。
///
/// - 抬到 `2^52` 以上：mod.io 的 id 都在 10 位以内，两边永远不会撞号。
/// - 停在 `2^53` 以下：界面是 JS，超过 `2^53` 会丢精度，界面上显示的 id 就跟后端不一样了，
///   点「卸载」时会找不着那条记录（v0.1 真踩过这个坑）。
fn id_from_digest(digest: &[u8]) -> u64 {
    let mut head = [0u8; 8];
    head.copy_from_slice(&digest[..8]);
    (u64::from_be_bytes(head) & 0x000F_FFFF_FFFF_FFFF) | LOCAL_ID_FLOOR
}

/// 算「这份东西」的内容指纹。
///
/// 同一个文件不管放在哪个目录、叫什么名字，算出来都一样——所以从两个不同路径导入同一个
/// mod，记账里只会有一条。文件夹是按「每个文件的相对路径 + 它的内容」汇总算的，
/// 所以文件夹里改了任何一个文件都会变成另一个 id。
pub fn fingerprint(src: &Path) -> Result<Fingerprint, String> {
    if !src.exists() {
        return Err(format!("找不到这个路径：{}", src.display()));
    }

    let mut hasher = Sha256::new();
    if src.is_file() {
        feed_file(&mut hasher, src)?;
    } else {
        let mut files = Vec::new();
        collect_files(src, src, &mut files)?;
        if files.is_empty() {
            return Err(format!("这个文件夹里没有任何文件：{}", src.display()));
        }
        files.sort_by(|a, b| a.1.cmp(&b.1));
        for (full, relative) in files {
            hasher.update(relative.as_bytes());
            hasher.update([0u8]);
            feed_file(&mut hasher, &full)?;
        }
    }

    let digest = hasher.finalize();
    let hash = digest.iter().map(|b| format!("{b:02x}")).collect::<String>();
    Ok(Fingerprint {
        id: id_from_digest(&digest),
        hash,
    })
}

fn feed_file(hasher: &mut Sha256, path: &Path) -> Result<(), String> {
    let data = std::fs::read(path).map_err(|e| format!("读 {} 失败：{e}", path.display()))?;
    hasher.update(Sha256::digest(&data));
    Ok(())
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "本地 mod".to_owned())
}

/// 把名字洗成能当文件名的样子（和下载那条链路一样的规则）。
fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches(['.', ' '].as_slice());
    if trimmed.is_empty() {
        "local-mod".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// 在文件夹里找 `.pak`（只找一层里的第一个、以及往下两层，够用且不会把整个盘扫一遍）。
fn find_pak_files(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                find_pak_files(&path, depth - 1, out);
            }
        } else if extension(&path) == "pak" {
            out.push(path);
        }
    }
}

/// 递归列出目录里的文件，同时算出 pak 里的相对路径（用 `/` 分隔）。
fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, String)>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("读目录失败：{e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|e| format!("算相对路径失败：{e}"))?
                .to_string_lossy()
                .replace('\\', "/");
            out.push((path, relative));
        }
    }
    Ok(())
}

/// 复制到我们自己的 mods 目录，并用复制后的文件名当显示名。
fn copy_into(src: &Path, dest_dir: &Path, fp: &Fingerprint) -> Result<LocalMod, String> {
    let name = stem(src);
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("建目录失败：{e}"))?;
    let ext = extension(src);
    let dest = dest_dir.join(format!(
        "{}-{}.{}",
        sanitize(&name),
        fp.id % 1_000_000,
        ext
    ));
    if dest != src {
        std::fs::copy(src, &dest).map_err(|e| format!("复制文件失败：{e}"))?;
    }
    Ok(LocalMod {
        name,
        path: dest,
        id: fp.id,
        hash: fp.hash.clone(),
    })
}

/// 认下用户给的路径，返回「复制作准备装」的那一份。
///
/// `dest_dir` 一般是 `<数据目录>\mods\local`。
pub fn resolve(src: &Path, dest_dir: &Path) -> Result<LocalMod, String> {
    if !src.exists() {
        return Err(format!("找不到这个路径：{}", src.display()));
    }

    if src.is_dir() {
        let mut paks = Vec::new();
        find_pak_files(src, 2, &mut paks);
        match paks.len() {
            1 => {
                // 装的是文件夹里那个 pak，指纹按 pak 算：同一个包放在哪儿都算同一个 mod
                let fp = fingerprint(&paks[0])?;
                return copy_into(&paks[0], dest_dir, &fp);
            }
            0 => {
                // 没有 pak 就不是数据包。脚本类 mod（带 main.dll / main.lua / main.js）在
                // 进来之前就已经被认走、走 UE4SS 那条路了，剩下的到这儿就是真的没东西可装。
                return Err(format!(
                    "这个文件夹里没有 .pak，不是能装的数据包：{}",
                    src.display()
                ));
            }
            _ => {
                let names: Vec<String> = paks
                    .iter()
                    .take(5)
                    .map(|p| {
                        p.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default()
                    })
                    .collect();
                return Err(format!(
                    "这个文件夹里有 {} 个 pak，不知道要装哪个：{}。请直接选其中一个文件。",
                    paks.len(),
                    names.join("、")
                ));
            }
        }
    }

    match extension(src).as_str() {
        "pak" | "zip" => {
            let fp = fingerprint(src)?;
            copy_into(src, dest_dir, &fp)
        }
        _ => Err(format!(
            "只认 .pak、装着 .pak 的 .zip，或者解包后的文件夹；这个不是：{}",
            src.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试目录放在构建产物下面，不写 C 盘。
    fn test_dir(name: &str) -> PathBuf {
        let base = std::env::var("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("target"));
        let dir = base.join("rock-sprout-tests").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建测试目录");
        dir
    }

    #[test]
    fn 同一个文件从两个不同路径导入算出来的id一样() {
        let dir = test_dir("local-id-content");
        let a = dir.join("下载目录").join("某个mod.pak");
        let b = dir.join("桌面").join("某个mod 的副本.pak");
        std::fs::create_dir_all(a.parent().unwrap()).expect("建目录");
        std::fs::create_dir_all(b.parent().unwrap()).expect("建目录");
        std::fs::write(&a, b"pak-contents").expect("写文件");
        std::fs::write(&b, b"pak-contents").expect("写副本");

        let fa = fingerprint(&a).expect("算指纹");
        let fb = fingerprint(&b).expect("算指纹");
        assert_eq!(fa.id, fb.id, "内容一样就该是同一个 mod，不管放在哪儿、叫什么");
        assert_eq!(fa.hash, fb.hash);
        assert!(fa.id >= LOCAL_ID_FLOOR, "要抬到 2^52 以上，跟 mod.io 的 id 分开放");
        assert!(fa.id < 0x0020_0000_0000_0000, "不能超过 2^53，不然界面上的数字会丢精度");

        let other = dir.join("另一个.pak");
        std::fs::write(&other, b"pak-contents-but-different").expect("写文件");
        assert_ne!(fingerprint(&other).expect("算指纹").id, fa.id, "内容不同就是两个 mod");
    }

    #[test]
    fn 文件夹的内容变了id就跟着变() {
        let dir = test_dir("local-id-dir");
        let folder = dir.join("散装");
        std::fs::create_dir_all(&folder).expect("建目录");
        std::fs::write(folder.join("a.txt"), b"one").expect("写文件");
        let before = fingerprint(&folder).expect("算指纹").id;

        std::fs::write(folder.join("a.txt"), b"one-changed").expect("改文件");
        let after = fingerprint(&folder).expect("算指纹").id;
        assert_ne!(before, after, "文件内容变了，指纹要变");
    }

    #[test]
    fn 单个pak会被复制到我们的目录里() {
        let dir = test_dir("local-pak");
        let src = dir.join("我的 美化.pak");
        std::fs::write(&src, b"not really a pak").expect("造一个假 pak");

        let got = resolve(&src, &dir.join("local")).expect("应该认得 .pak");
        assert_eq!(got.name, "我的 美化");
        assert_ne!(got.path, src, "要复制一份，不能直接用原文件");
        assert!(got.path.exists(), "复制出来的文件得真的在");
        assert_eq!(got.path.extension().unwrap(), "pak");
    }

    #[test]
    fn 装着pak的zip也认() {
        let dir = test_dir("local-zip");
        let src = dir.join("别人给的.zip");
        std::fs::write(&src, b"PK\x05\x06 ----").expect("造一个假 zip");

        let got = resolve(&src, &dir.join("local")).expect("应该认得 .zip");
        assert_eq!(got.name, "别人给的");
        assert_eq!(got.path.extension().unwrap(), "zip");
    }

    #[test]
    fn 文件夹里只有一个pak就装它() {
        let dir = test_dir("local-dir-one-pak");
        let folder = dir.join("某个 mod 1.2.3");
        std::fs::create_dir_all(folder.join("FSD")).expect("建子目录");
        std::fs::write(folder.join("FSD").join("内容.pak"), b"pak").expect("放假 pak");

        let got = resolve(&folder, &dir.join("local")).expect("应该认得文件夹里的 pak");
        assert_eq!(got.path.extension().unwrap(), "pak");
        assert!(got.path.exists());
    }

    #[test]
    fn 文件夹里有多个pak时要求用户自己选() {
        let dir = test_dir("local-dir-many-pak");
        let folder = dir.join("一堆");
        std::fs::create_dir_all(&folder).expect("建目录");
        std::fs::write(folder.join("a.pak"), b"pak").expect("放假 pak");
        std::fs::write(folder.join("b.pak"), b"pak").expect("放假 pak");

        let err = resolve(&folder, &dir.join("local")).expect_err("多个 pak 应该报错让用户选");
        assert!(err.contains("2 个 pak"), "错误里要告诉用户有几个：{err}");
    }

    #[test]
    fn 文件夹里没有pak就明说不是数据包() {
        let dir = test_dir("local-dir-no-pak");
        let folder = dir.join("散装 mod");
        std::fs::create_dir_all(folder.join("FSD").join("Content")).expect("建子目录");
        std::fs::write(
            folder.join("FSD").join("Content").join("某个.uasset"),
            b"asset-bytes",
        )
        .expect("放假资源");

        // 以前这里会把整个文件夹悄悄打成 pak，等于把脚本类 mod 静默塞进合并包；
        // 现在只认真正的 .pak，别的样子直接说清楚。
        let err = resolve(&folder, &dir.join("local")).expect_err("没有 pak 应该说清楚");
        assert!(err.contains("没有 .pak"), "错误里要说明白：{err}");
    }

    #[test]
    fn 不认的格式会明确说不认什么() {
        let dir = test_dir("local-bad-ext");
        let src = dir.join("说明.txt");
        std::fs::write(&src, b"hello").expect("造个文件");
        let err = resolve(&src, &dir.join("local")).expect_err("txt 应该被拒");
        assert!(err.contains(".pak"), "错误里要说清楚只认什么：{err}");
    }

    #[test]
    fn 路径不存在时说的是找不到而不是别的错() {
        let dir = test_dir("local-missing");
        let err = resolve(&dir.join("根本没有.pak"), &dir.join("local")).expect_err("应该报错");
        assert!(err.contains("找不到"), "{err}");
    }
}
