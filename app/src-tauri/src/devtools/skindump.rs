// 读一批资产（皮肤/十字镐/外观），把里面 TextProperty 的「表 id + 键 + 英文原文」打出来，
// 用来回答一个问题：这些名字到底是「每条一个键」，还是「指向少数几张字符串表」。
//
// 另外打出 `SkinSet` 指向的对象名——皮肤的名字挂在那个对象上，不在这份资产里。
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

/// `SkinSet` 这个 ObjectProperty 指向哪个对象（Import 里取名字）
fn set_ref<R: std::io::Read + std::io::Seek>(props: &[Property], asset: &Asset<R>) -> Option<String> {
    for p in props {
        let Property::ObjectProperty(o) = p else {
            continue;
        };
        if o.get_name().get_owned_content() != "SkinSet" {
            continue;
        }
        let idx = o.value.index;
        if idx >= 0 {
            continue;
        }
        let i = (-idx - 1) as usize;
        if let Some(imp) = asset.imports.get(i) {
            return Some(imp.object_name.get_owned_content());
        }
    }
    None
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

    let mut printed = false;
    let mut set_name = String::new();
    for e in &asset.asset_data.exports {
        let Some(normal) = cast!(Export, NormalExport, e) else {
            continue;
        };
        if set_name.is_empty() {
            set_name = set_ref(&normal.properties, &asset).unwrap_or_default();
        }
        for p in &normal.properties {
            let Some((table, key, source)) = text_bits(p) else {
                continue;
            };
            if table.is_empty() && key.is_empty() && source.is_empty() {
                continue;
            }
            println!(
                "{{\"asset\":\"{}\",\"prop\":\"{}\",\"table\":\"{}\",\"key\":\"{}\",\"en\":\"{}\",\"set\":\"{}\"}}",
                asset_name,
                p.get_name().get_owned_content(),
                json_escape(&table),
                json_escape(&key),
                json_escape(&source),
                json_escape(&set_name),
            );
            printed = true;
        }
    }

    // 没有名字属性、但有 SkinSet 的，也打一行，好统计「换了入口能盖住多少」
    if !printed && !set_name.is_empty() {
        println!(
            "{{\"asset\":\"{}\",\"prop\":\"\",\"table\":\"\",\"key\":\"\",\"en\":\"\",\"set\":\"{}\"}}",
            asset_name,
            json_escape(&set_name),
        );
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
