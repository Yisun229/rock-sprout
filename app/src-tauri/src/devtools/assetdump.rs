// 读一个 .uasset（连同同名 .uexp），把里程碑的 Tiers[] 里每级的 CompletionCount 和
// PerkPoints 打出来，每行一个 JSON，方便外面直接拼表。
//
//   cargo run --release --features devtools --bin assetdump -- <目录或文件...>
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

fn num(p: &Property) -> Option<f64> {
    match p {
        Property::IntProperty(v) => Some(f64::from(v.value)),
        Property::Int64Property(v) => Some(v.value as f64),
        Property::FloatProperty(v) => Some(f64::from(v.value.0)),
        Property::DoubleProperty(v) => Some(v.value.0),
        Property::ByteProperty(v) => match &v.value {
            unreal_asset::properties::int_property::BytePropertyValue::Byte(b) => Some(f64::from(*b)),
            unreal_asset::properties::int_property::BytePropertyValue::FName(_) => None,
        },
        _ => None,
    }
}

/// Tiers 的结构体数组里，每级的两个数
fn tiers_of(props: &[Property]) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    let Some(tiers) = props.iter().find(|p| p.get_name() == "Tiers") else {
        return out;
    };
    let Property::ArrayProperty(arr) = tiers else {
        return out;
    };
    for item in &arr.value {
        let Property::StructProperty(st) = item else {
            continue;
        };
        let count = st
            .value
            .iter()
            .find(|p| p.get_name() == "CompletionCount")
            .and_then(num)
            .unwrap_or(f64::NAN);
        let points = st
            .value
            .iter()
            .find(|p| p.get_name() == "PerkPoints")
            .and_then(num)
            .unwrap_or(f64::NAN);
        out.push((count as i64, points as i64));
    }
    out
}

fn dump(path: &Path) {
    let Ok(bytes) = fs::read(path) else {
        return;
    };
    let uexp = path.with_extension("uexp");
    let exp = fs::read(&uexp).ok();
    let parsed = Asset::new(
        Cursor::new(bytes),
        exp.map(Cursor::new),
        EngineVersion::VER_UE4_27,
        None,
        false,
    );
    let Ok(asset) = parsed else {
        println!("{{\"asset\":\"{}\",\"error\":\"解析失败\"}}", path.display());
        return;
    };
    for e in &asset.asset_data.exports {
        let Some(normal) = cast!(Export, NormalExport, e) else {
            continue;
        };
        let tiers = tiers_of(&normal.properties);
        if tiers.is_empty() {
            continue;
        }
        let list: Vec<String> = tiers
            .iter()
            .map(|(c, p)| format!("{{\"count\":{c},\"perkPoints\":{p}}}"))
            .collect();
        println!(
            "{{\"asset\":\"{}\",\"tiers\":[{}]}}",
            path.file_stem().unwrap_or_default().to_string_lossy(),
            list.join(",")
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
