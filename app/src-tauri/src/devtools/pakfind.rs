// 在 pak 里按 16 字节 GUID 找资产（只读，不落地）。
//
//   pakfind <pak 路径> <清单文件> [路径关键字]
//
// 清单文件每行「kind guid」，例如：weapons 0123abcd...（32 位小写 hex）。
// 只扫 .uexp / .uasset；给路径关键字时只扫名字含该关键字的条目。

use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::io::BufReader;

use repak::PakBuilder;

fn hex16(s: &str) -> Option<[u8; 16]> {
    if s.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for i in 0..16 {
        out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("用法：pakfind <pak> <清单文件> [路径关键字]");
        std::process::exit(2);
    }
    let pak_path = &args[0];
    let list_path = &args[1];
    let keyword = args.get(2).cloned();

    // 清单：16 字节 → 「kind guid」
    let mut by_guid: HashMap<[u8; 16], String> = HashMap::new();
    // 首字节 → 候选清单（先按首字节筛，再逐字节比，避免每个位置都去查哈希表）
    let mut by_first: HashMap<u8, Vec<([u8; 16], String)>> = HashMap::new();
    for line in fs::read_to_string(list_path).unwrap_or_default().lines() {
        let mut it = line.split_whitespace();
        let (Some(kind), Some(guid)) = (it.next(), it.next()) else {
            continue;
        };
        if guid.chars().all(|c| c == '0') {
            continue; // 全零不是真 GUID（空的装备槽就是它），会把所有文件都算命中
        }
        if let Some(bytes) = hex16(guid) {
            let tag = format!("{kind} {guid}");
            by_guid.insert(bytes, tag.clone());
            by_first.entry(bytes[0]).or_default().push((bytes, tag));
        }
    }
    if by_guid.is_empty() {
        eprintln!("清单里没有可用的 GUID");
        std::process::exit(2);
    }

    let file = fs::File::open(pak_path).expect("打不开 pak");
    let mut reader = BufReader::new(file);
    let pak = PakBuilder::new().reader(&mut reader).expect("pak 解析失败");

    let mut seen: HashSet<String> = HashSet::new();
    let mut scanned = 0usize;
    for name in pak.files() {
        if !(name.ends_with(".uexp") || name.ends_with(".uasset")) {
            continue;
        }
        if let Some(kw) = &keyword {
            if !name.contains(kw.as_str()) {
                continue;
            }
        }
        let Ok(bytes) = pak.get(&name, &mut reader) else {
            continue;
        };
        scanned += 1;
        if bytes.len() < 16 {
            continue;
        }
        for i in 0..=bytes.len() - 16 {
            let Some(cands) = by_first.get(&bytes[i]) else {
                continue;
            };
            for (pat, tag) in cands {
                if &bytes[i..i + 16] == pat {
                    if seen.insert(format!("{tag} {name}")) {
                        println!("{tag} {name}");
                    }
                }
            }
        }
    }
    eprintln!("扫了 {scanned} 个文件，命中组合 {} 个", seen.len());
}
