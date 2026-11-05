// 读一批改装资产（UPG_*.uasset），把每条的存档 ID、名称/说明的本地化键与英文原文、
// 以及升级层级打出来，每行一个 JSON。
//
//   cargo run --release --features devtools --bin upgdump -- <目录或文件...>
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

fn text_of(p: &Property) -> Option<(String, String)> {
    let Property::TextProperty(t) = p else {
        return None;
    };
    let key = t.value.clone().unwrap_or_default();
    let source = t.culture_invariant_string.clone().unwrap_or_default();
    if key.is_empty() {
        return None;
    }
    Some((key, source))
}

fn guid_of(p: &Property) -> Option<String> {
    let Property::StructProperty(s) = p else {
        return None;
    };
    let inner = s.value.first()?;
    let Property::GuidProperty(g) = inner else {
        return None;
    };
    let v = g.value;
    Some(format!("{v:?}"))
}

fn int_of(p: &Property) -> Option<i64> {
    match p {
        Property::IntProperty(v) => Some(i64::from(v.value)),
        Property::Int64Property(v) => Some(v.value),
        Property::ByteProperty(v) => match &v.value {
            unreal_asset::properties::int_property::BytePropertyValue::Byte(b) => Some(i64::from(*b)),
            _ => None,
        },
        _ => None,
    }
}

fn enum_name(p: &Property) -> Option<String> {
    let Property::EnumProperty(e) = p else {
        return None;
    };
    e.value.as_ref().map(|f| f.get_owned_content()).unwrap_or_default().into()
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', " ")
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
    for e in &asset.asset_data.exports {
        let Some(normal) = cast!(Export, NormalExport, e) else {
            continue;
        };
        let props = &normal.properties;
        let find = |name: &str| props.iter().find(|p| p.get_name() == name);
        let Some(save) = find("SaveGameID").and_then(guid_of) else {
            continue;
        };
        let name = find("Name").and_then(text_of).unwrap_or_default();
        let desc = find("Description").and_then(text_of).unwrap_or_default();
        let tier = find("UpgradeTier").and_then(enum_name).unwrap_or_default();
        let class = find("UpgradeClass").and_then(enum_name).unwrap_or_default();
        let amount = find("Amount").and_then(int_of);
        println!(
            "{{\"asset\":\"{}\",\"save\":\"{}\",\"nameKey\":\"{}\",\"nameEn\":\"{}\",\"descKey\":\"{}\",\"tier\":\"{}\",\"class\":\"{}\",\"amount\":{}}}",
            path.file_stem().unwrap_or_default().to_string_lossy(),
            save,
            name.0,
            json_escape(&name.1),
            desc.0,
            tier,
            class,
            amount.map(|v| v.to_string()).unwrap_or_else(|| "null".into()),
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
