//! 批量检查存档读取器：把一批 `.sav` 全读一遍，逐条报「新解析 / 命中缓存 + 节点数 + 收尾字节」。
//!
//! 每份存档都按只读四步读**两次**：第一次新解析并写缓存，第二次必须命中缓存，
//! 两次渲染出来的字节必须完全一样。行里同时贴原档读取前后的两个 sha256。
//!
//! 用法：
//! ```text
//! cargo run --release --features devtools --bin savcheck            # 默认读 _tools/save-samples
//! cargo run --release --features devtools --bin savcheck -- <文件或目录>... [--cache <缓存目录>]
//! ```
//!
//! 退出码：有「失败」的档就 1，全是「通」或「跳过」就 0。

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rock_sprout::save::store::{self, LoadClass};

fn main() -> ExitCode {
    let mut targets: Vec<PathBuf> = Vec::new();
    let mut cache_root = default_cache();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--cache" {
            if let Some(v) = args.next() {
                cache_root = PathBuf::from(v);
            }
            continue;
        }
        targets.push(PathBuf::from(a));
    }
    if targets.is_empty() {
        targets.push(default_dir());
    }

    let mut files = Vec::new();
    for t in &targets {
        collect(t, &mut files);
    }
    files.sort();
    files.dedup();

    if files.is_empty() {
        eprintln!(
            "没找到 .sav 文件：{}",
            targets.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
        );
        return ExitCode::from(2);
    }

    let mut ok = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;
    let mut total_nodes = 0usize;
    let mut cache_bad = 0usize;

    for path in &files {
        let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();

        // 第一次：新解析（会写缓存）
        let (doc1, r1) = match store::load(path, &cache_root) {
            Ok(v) => v,
            Err(e) => {
                // 「不是这个游戏的存档」算跳过（老备份里混了别的东西），其余算是真失败
                if e.class == LoadClass::NotSave {
                    skipped += 1;
                    println!("跳过 {name}  {} B  跳过：{}", e.class.human(), e.detail);
                } else {
                    failed += 1;
                    let msg = format!("{}（{}）：{}", e.class.human(), e.class.code(), e.detail);
                    println!("失败 {name}  失败：{msg}");
                }
                continue;
            }
        };

        // 第二次：必须命中缓存，且渲染字节一模一样
        let (doc2, r2) = match store::load(path, &cache_root) {
            Ok(v) => v,
            Err(e) => {
                failed += 1;
                println!("失败 {name}  第二次读取出错：{e}");
                continue;
            }
        };
        let b1 = store::render(&doc1).unwrap_or_default();
        let b2 = store::render(&doc2).unwrap_or_default();
        let same_bytes = b1 == b2;
        let untouched = r1.untouched() && r2.untouched();
        if !(r2.from_cache && same_bytes && untouched) {
            cache_bad += 1;
        }

        ok += 1;
        total_nodes += doc1.nodes;
        println!("通   {name}  {} B  节点 {}  收尾 {}/{}  跳过 {}  说明 {}",
            doc1.file_bytes, doc1.nodes, doc1.consumed, doc1.file_bytes, doc1.skipped, doc1.notes);
        println!("     第 1 次 {}  sha256 前 {}… → 后 {}…  副本 {}…",
            r1.kind_text(),
            &r1.sha256_before[..16],
            &r1.sha256_after[..16],
            r1.sha256_copy.as_deref().map(|s| &s[..16]).unwrap_or("-"));
        println!("     第 2 次 {}  结果字节{}相同  原档{}",
            r2.kind_text(),
            if same_bytes { "完全" } else { "不" },
            if untouched { "未被改动" } else { "被改动！" });
    }

    println!(
        "—— 共 {} 个：通 {ok}，跳过 {skipped}，失败 {failed}；节点合计 {total_nodes}；缓存核对不符 {cache_bad}",
        files.len()
    );
    println!("   缓存目录：{}", cache_root.display());
    if failed == 0 && cache_bad == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn default_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../_tools/save-samples")
}

fn default_cache() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../_tools/_savcheck-cache")
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect(&p, out);
            } else if is_save(&p) {
                out.push(p);
            }
        }
    } else if is_save(path) {
        out.push(path.to_path_buf());
    }
}

fn is_save(p: &Path) -> bool {
    p.extension().map(|e| e.eq_ignore_ascii_case("sav")).unwrap_or(false)
}
