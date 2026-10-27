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

/// 认下来的一个本地 mod。
#[derive(Debug, Clone)]
pub struct LocalMod {
    /// 显示用名字：文件名去掉扩展名（文件夹打成的包用文件夹名）。
    pub name: String,
    /// 复制到我们 mods 目录之后的位置，交给整合逻辑的就是它。
    pub path: PathBuf,
}

/// 本地 mod 的记账 id。
///
/// mod.io 的 id 都在 10 位以内，这里固定把值抬到 `2^52` 以上，两边永远不会撞号；
/// 同一个路径每次算出来的值一样，所以重复导入是「覆盖同一条」而不是越攒越多。
///
/// 为什么停在 `2^53` 以下：界面是 JS，数字超过 `2^53` 会丢精度，
/// 界面上显示的 id 会跟后端的不一样，卸载时就找不着那条记录了。
pub fn id_for(path: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (hash & 0x000F_FFFF_FFFF_FFFF) | 0x0010_0000_0000_0000
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

/// 把一个解包后的文件夹打成 pak。
///
/// 游戏只认 pak，散装文件它看都不看，所以「导入文件夹」这一步必须自己打包。
/// 用和整合逻辑同一套 repak 写入器，压缩方式也一致（zlib）。
fn pack_dir(dir: &Path, out: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    collect_files(dir, dir, &mut files)?;
    if files.is_empty() {
        return Err(format!("这个文件夹里没有任何文件：{}", dir.display()));
    }

    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("建目录失败：{e}"))?;
    }
    let file = std::fs::File::create(out).map_err(|e| format!("建 pak 文件失败：{e}"))?;
    let mut writer = repak::PakBuilder::new()
        .compression([repak::Compression::Zlib])
        .writer(
            std::io::BufWriter::new(file),
            repak::Version::V11,
            "../../../".to_owned(),
            None,
        );

    for (full, relative) in files {
        let data = std::fs::read(&full).map_err(|e| format!("读 {} 失败：{e}", full.display()))?;
        writer
            .write_file(&relative, &data)
            .map_err(|e| format!("往 pak 里写 {} 失败：{e}", relative))?;
    }
    writer.write_index().map_err(|e| format!("收尾写 pak 失败：{e}"))?;
    Ok(())
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
fn copy_into(src: &Path, dest_dir: &Path) -> Result<LocalMod, String> {
    let name = stem(src);
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("建目录失败：{e}"))?;
    let ext = extension(src);
    let dest = dest_dir.join(format!(
        "{}-{}.{}",
        sanitize(&name),
        id_for(&src.to_string_lossy()) % 1_000_000,
        ext
    ));
    if dest != src {
        std::fs::copy(src, &dest).map_err(|e| format!("复制文件失败：{e}"))?;
    }
    Ok(LocalMod { name, path: dest })
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
            1 => return copy_into(&paks[0], dest_dir),
            0 => {
                // 没有现成 pak：把整个文件夹打成一个包
                let name = stem(src);
                std::fs::create_dir_all(dest_dir).map_err(|e| format!("建目录失败：{e}"))?;
                let dest = dest_dir.join(format!(
                    "{}-{}.pak",
                    sanitize(&name),
                    id_for(&src.to_string_lossy()) % 1_000_000
                ));
                pack_dir(src, &dest)?;
                return Ok(LocalMod { name, path: dest });
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
        "pak" | "zip" => copy_into(src, dest_dir),
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
    fn 同一个路径算出来的id是稳定的且不会和modio撞号() {
        let a = id_for("D:/下载/我自己下的/某个mod.pak");
        let b = id_for("D:/下载/我自己下的/某个mod.pak");
        assert_eq!(a, b, "同样的路径每次算出来要一样");
        assert!(a >= 0x0010_0000_0000_0000, "要抬到 2^52 以上，跟 mod.io 的 id 分开放");
        assert!(a < 0x0020_0000_0000_0000, "不能超过 2^53，不然界面上的数字会丢精度");
        assert_ne!(a, id_for("D:/别处/某个mod.pak"), "不同路径要给不同 id");
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
    fn 解包后的文件夹会被打成一个pak() {
        let dir = test_dir("local-dir-pack");
        let folder = dir.join("散装 mod");
        std::fs::create_dir_all(folder.join("FSD").join("Content")).expect("建子目录");
        std::fs::write(
            folder.join("FSD").join("Content").join("某个.uasset"),
            b"asset-bytes",
        )
        .expect("放假资源");

        let got = resolve(&folder, &dir.join("local")).expect("应该把文件夹打包");
        assert_eq!(got.path.extension().unwrap(), "pak");
        assert_eq!(got.name, "散装 mod");

        // 打出来的包要能被读回来，而且要带上一模一样的相对路径
        let mut file = std::fs::File::open(&got.path).expect("打开打好的包");
        let pak = repak::PakBuilder::new()
            .reader(&mut std::io::BufReader::new(&mut file))
            .expect("读打好的包");
        let names: Vec<String> = pak.files().into_iter().map(|f| f.to_string()).collect();
        assert!(
            names.iter().any(|n| n == "FSD/Content/某个.uasset"),
            "包里的路径不对：{names:?}"
        );
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
