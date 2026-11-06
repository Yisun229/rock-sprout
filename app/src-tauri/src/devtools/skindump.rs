// 读一批资产（皮肤/十字镐/外观），把里面 TextProperty 的「表 id + 键 + 英文原文」打出来，
// 用来回答一个问题：这些名字到底是「每条一个键」，还是「指向少数几张字符串表」。
//
//   cargo run --release --features devtools --bin skindump -- <目录或文件...>
//
// 只读：不写回任何资产。

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use unreal_asset::{
    cast,
    engine_version::EngineVersion,
    exports::Export,
    properties::{Property, PropertyDataTrait},
    Asset,
};

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
        .replace('\r', " ")
        .replace('\t', " ")
}

/// 一条 TextProperty 能给出的三样东西：表 id、键、英文原文（有的资产只给后面两样）
fn text_bits(p: &Property) -> Option<(String, String, String)> {
    let Property::TextProperty(t) = p else {
        return None;
    };
    let table = t
        .table_id
        .as_ref()
        .map(|f| f.get_owned_content())
        .unwrap_or_default();
    let key = t.value.clone().unwrap_or_default();
    let source = t.culture_invariant_string.clone().unwrap_or_default();
    Some((table, key, source))
}

fn dump(path: &Path) {
    let Ok(bytes) = fs::read(path) else {
        return;
    };
    let exp = fs::read(path.with_extension("uexp")).ok();
    let parsed = Asset::new(
        Cursor::new(bytes),
        exp.map(Cursor::new),
        EngineVersion::VER_UE4_27,
        None,
        false,
    );
    let Ok(asset) = parsed else {
        return;
    };
    let asset_name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    for e in &asset.asset_data.exports {
        let Some(normal) = cast!(Export, NormalExport, e) else {
            continue;
        };
        for p in &normal.properties {
            let Some((table, key, source)) = text_bits(p) else {
                continue;
            };
            if table.is_empty() && key.is_empty() && source.is_empty() {
                continue;
            }
            println!(
                "{{\"asset\":\"{}\",\"prop\":\"{}\",\"table\":\"{}\",\"key\":\"{}\",\"en\":\"{}\"}}",
                asset_name,
                p.get_name().get_owned_content(),
                json_escape(&table),
                json_escape(&key),
                json_escape(&source),
            );
        }
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|e| e == "uasset") {
            out.push(p);
        }
    }
}

fn main() {
    let mut files: Vec<PathBuf> = Vec::new();
    for arg in std::env::args().skip(1) {
        let p = PathBuf::from(arg);
        if p.is_dir() {
            walk(&p, &mut files);
        } else {
            files.push(p);
        }
    }
    files.sort();
    for f in &files {
        dump(f);
    }
}
