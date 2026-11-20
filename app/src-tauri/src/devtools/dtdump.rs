// 试读 DataTable：把每行的 struct 属性打出来，看能否不靠猜偏移读出数值。
//
//   cargo run --release --features devtools --bin dtdump -- <资产...>
//
// 只读：不写回任何资产。

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use unreal_asset::{
    engine_version::EngineVersion,
    exports::Export,
    properties::{Property, PropertyDataTrait},
    Asset,
};

fn show(p: &Property, depth: usize) -> String {
    let pad = "  ".repeat(depth);
    let name = p.get_name().get_owned_content();
    let tail = match p {
        Property::IntProperty(v) => format!("Int {}", v.value),
        Property::Int64Property(v) => format!("Int64 {}", v.value),
        Property::FloatProperty(v) => format!("Float {}", v.value.0),
        Property::ByteProperty(v) => format!("Byte {v:?}"),
        Property::StructProperty(s) => {
            let mut out = format!("Struct({:?}) [", s.struct_type.as_ref().map(|x| x.get_owned_content()));
            for inner in &s.value {
                out.push_str(&format!("\n{pad}  {}", show(inner, depth + 1)));
            }
            out.push(']');
            format!("{out}")
        }
        Property::ArrayProperty(a) => {
            let inner: Vec<String> = a
                .value
                .iter()
                .take(6)
                .map(|x| show(x, depth + 1))
                .collect();
            format!("Array{}[{}]", a.value.len(), inner.join(", "))
        }
        other => format!("{other:?}"),
    };
    format!("{pad}{name} = {tail}")
}

fn dump(path: &Path) {
    let name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let Ok(bytes) = fs::read(path) else { return };
    let exp = fs::read(path.with_extension("uexp")).ok();
    let parsed = Asset::new(
        Cursor::new(bytes),
        exp.map(Cursor::new),
        EngineVersion::VER_UE4_27,
        None,
        false,
    );
    let Ok(asset) = parsed else {
        println!("{{\"asset\":\"{name}\",\"error\":\"解析失败\"}}");
        return;
    };
    println!("{name}: {} exports", asset.asset_data.exports.len());
    for (i, e) in asset.asset_data.exports.iter().enumerate() {
        match e {
            Export::DataTableExport(dt) => {
                let obj = dt.normal_export.base_export.object_name.get_owned_content();
                println!("  [{i}] {obj} : DataTable");
            println!("    rows = {}", dt.table.data.len());
            for row in dt.table.data.iter() {
                let rn = format!(
                    "{}#{}",
                    row.name.get_owned_content(),
                    row.name.get_number()
                );
                let st = row
                    .struct_type
                    .as_ref()
                    .map(|x| x.get_owned_content())
                    .unwrap_or_default();
                println!("    row {rn} <{st}> {} props", row.value.len());
                for p in &row.value {
                    println!("      {}", show(p, 3));
                }
            }
            }
            _ => println!("  [{i}] (other export type)"),
        }
    }
}

fn main() {
    let mut files: Vec<PathBuf> = Vec::new();
    for arg in std::env::args().skip(1) {
        let p = PathBuf::from(arg);
        if p.is_dir() {
            for entry in fs::read_dir(&p).into_iter().flatten().flatten() {
                let q = entry.path();
                if q.extension().is_some_and(|e| e == "uasset") {
                    files.push(q);
                }
            }
        } else {
            files.push(p);
        }
    }
    for f in files {
        dump(&f);
    }
}
