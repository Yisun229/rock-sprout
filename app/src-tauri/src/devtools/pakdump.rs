//! 只读：看游戏 pak 里有什么、把指定文件抽出来。
//!
//! 用法：
//!   pakdump list  <pak> [关键词]
//!   pakdump extract <pak> <关键词> <输出目录>

use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("用法：pakdump list <pak> [关键词] / pakdump extract <pak> <关键词> <输出目录>");
        std::process::exit(2);
    }
    let mode = args[0].as_str();
    let pak_path = PathBuf::from(&args[1]);
    let result = match mode {
        "list" => list(&pak_path, args.get(2).map(String::as_str)),
        "extract" => {
            let Some(pattern) = args.get(2) else {
                eprintln!("extract 需要关键词");
                std::process::exit(2);
            };
            let Some(out) = args.get(3) else {
                eprintln!("extract 需要输出目录");
                std::process::exit(2);
            };
            extract(&pak_path, pattern, Path::new(out))
        }
        "extract-list" => {
            let Some(list) = args.get(2) else {
                eprintln!("extract-list 需要清单文件");
                std::process::exit(2);
            };
            let Some(out) = args.get(3) else {
                eprintln!("extract-list 需要输出目录");
                std::process::exit(2);
            };
            extract_list(&pak_path, Path::new(list), Path::new(out))
        }
        other => Err(format!("不认识的模式：{other}")),
    };
    if let Err(e) = result {
        eprintln!("失败：{e}");
        std::process::exit(1);
    }
}

fn list(pak_path: &Path, pattern: Option<&str>) -> Result<(), String> {
    let file = std::fs::File::open(pak_path).map_err(|e| format!("打开失败：{e}"))?;
    let mut reader = BufReader::new(file);
    let pak = repak::PakBuilder::new()
        .reader(&mut reader)
        .map_err(|e| format!("不是能识别的 pak：{e}"))?;

    let mut names: Vec<String> = pak.files().into_iter().collect();
    names.sort();
    let mut shown = 0usize;
    for name in &names {
        if let Some(p) = pattern {
            if !name.to_lowercase().contains(&p.to_lowercase()) {
                continue;
            }
        }
        println!("{name}");
        shown += 1;
    }
    eprintln!("—— 共 {} 个条目，命中 {shown} 个", names.len());
    Ok(())
}

fn extract(pak_path: &Path, pattern: &str, out_dir: &Path) -> Result<(), String> {
    let file = std::fs::File::open(pak_path).map_err(|e| format!("打开失败：{e}"))?;
    let mut reader = BufReader::new(file);
    let pak = repak::PakBuilder::new()
        .reader(&mut reader)
        .map_err(|e| format!("不是能识别的 pak：{e}"))?;

    let mut names: Vec<String> = pak.files().into_iter().collect();
    names.sort();
    let mut written = 0usize;
    let needle = pattern.to_lowercase();
    for name in &names {
        if !name.to_lowercase().contains(&needle) {
            continue;
        }
        let data = pak
            .get(name, &mut reader)
            .map_err(|e| format!("读 {name} 失败：{e}"))?;
        let path = out_dir.join(name.trim_start_matches('/'));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("建目录失败：{e}"))?;
        }
        let mut f = std::fs::File::create(&path).map_err(|e| format!("写 {name} 失败：{e}"))?;
        f.write_all(&data).map_err(|e| format!("写 {name} 失败：{e}"))?;
        written += 1;
    }
    eprintln!("—— 写出 {written} 个文件到 {}", out_dir.display());
    Ok(())
}

/// 按清单里的**完整条目名**抽（一个名字一行），给 GUID 定位出来的那批文件用。
fn extract_list(pak_path: &Path, list_path: &Path, out_dir: &Path) -> Result<(), String> {
    let file = std::fs::File::open(pak_path).map_err(|e| format!("打开失败：{e}"))?;
    let mut reader = BufReader::new(file);
    let pak = repak::PakBuilder::new()
        .reader(&mut reader)
        .map_err(|e| format!("不是能识别的 pak：{e}"))?;

    let wanted: Vec<String> = std::fs::read_to_string(list_path)
        .map_err(|e| format!("读清单失败：{e}"))?
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();

    let mut written = 0usize;
    let mut missing = 0usize;
    for name in &wanted {
        let Ok(data) = pak.get(name, &mut reader) else {
            missing += 1;
            continue;
        };
        let path = out_dir.join(name.trim_start_matches('/'));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("建目录失败：{e}"))?;
        }
        let mut f = std::fs::File::create(&path).map_err(|e| format!("写 {name} 失败：{e}"))?;
        f.write_all(&data).map_err(|e| format!("写 {name} 失败：{e}"))?;
        written += 1;
    }
    eprintln!(
        "—— 清单 {} 条，写出 {written} 个，找不到 {missing} 个；目录 {}",
        wanted.len(),
        out_dir.display()
    );
    Ok(())
}
