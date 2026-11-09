// 读一个资产，按 export 分节把每个属性连同取值摊成 JSON——比 namedump 更细，
// 用于「曲线怎么存」「奖励树节点长什么样」这类结构调查。
//
//   cargo run --release --features devtools --bin propdump -- <文件或目录...>
//
// 只读，不写回任何资产。

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

fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

fn text_json(table: &str, key: &str, en: &str) -> String {
    format!(
        "{{\"table\":{},\"key\":{},\"en\":{}}}",
        json_str(table),
        json_str(key),
        json_str(en)
    )
}

fn value_json(imports: &[unreal_asset::Import], p: &Property) -> Option<String> {
    match p {
        Property::BoolProperty(b) => Some(format!("{}", b.value)),
        Property::IntProperty(i) => Some(format!("{}", i.value)),
        Property::Int64Property(i) => Some(format!("{}", i.value)),
        Property::Int16Property(i) => Some(format!("{}", i.value)),
        Property::Int8Property(i) => Some(format!("{}", i.value)),
        Property::ByteProperty(b) => Some(format!("\"{:?}\"", b.value)),
        Property::FloatProperty(f) => Some(format!("{}", f.value)),
        Property::DoubleProperty(d) => Some(format!("{}", d.value)),
        Property::StrProperty(s) => Some(json_str(s.value.as_deref().unwrap_or(""))),
        Property::NameProperty(n) => Some(json_str(&n.value.get_owned_content())),
        Property::TextProperty(t) => {
            let table = t
                .table_id
                .as_ref()
                .map(|f| f.get_owned_content())
                .unwrap_or_default();
            let key = t.value.clone().unwrap_or_default();
            let en = t.culture_invariant_string.clone().unwrap_or_default();
            Some(text_json(&table, &key, &en))
        }
        Property::ObjectProperty(o) => {
            let idx = o.value.index;
            let target = if idx < 0 {
                imports
                    .get((-idx - 1) as usize)
                    .map(|i| i.object_name.get_owned_content())
                    .unwrap_or_default()
            } else {
                String::new()
            };
            Some(format!(
                "{{\"index\":{},\"target\":{}}}",
                idx,
                json_str(&target)
            ))
        }
        Property::RichCurveKeyProperty(k) => Some(format!(
            "{{\"time\":{},\"value\":{}}}",
            k.time.0, k.value.0
        )),
        _ => None,
    }
}

fn type_name(p: &Property) -> &'static str {
    match p {
        Property::ArrayProperty(_) => "Array",
        Property::StructProperty(_) => "Struct",
        Property::ObjectProperty(_) => "Object",
        Property::SoftObjectProperty(_) => "SoftObject",
        Property::TextProperty(_) => "Text",
        Property::StrProperty(_) => "Str",
        Property::NameProperty(_) => "Name",
        Property::IntProperty(_) => "Int",
        Property::Int64Property(_) => "Int64",
        Property::Int16Property(_) => "Int16",
        Property::Int8Property(_) => "Int8",
        Property::BoolProperty(_) => "Bool",
        Property::ByteProperty(_) => "Byte",
        Property::FloatProperty(_) => "Float",
        Property::DoubleProperty(_) => "Double",
        Property::RichCurveKeyProperty(_) => "RichCurveKey",
        _ => "Other",
    }
}

fn walk(
    imports: &[unreal_asset::Import],
    props: &[Property],
    path: &str,
    out: &mut Vec<String>,
    depth: usize,
) {
    if depth > 12 {
        return;
    }
    for p in props {
        let fname = p.get_name().get_owned_content();
        let full = if path.is_empty() {
            fname
        } else {
            format!("{path}.{fname}")
        };
        match p {
            Property::ArrayProperty(a) => {
                out.push(format!(
                    "{{\"path\":{},\"type\":\"Array<{}>\",\"count\":{}}}",
                    json_str(&full),
                    a.value.first().map(type_name).unwrap_or("?"),
                    a.value.len()
                ));
                for (i, item) in a.value.iter().enumerate() {
                    walk(
                        imports,
                        std::slice::from_ref(item),
                        &format!("{full}[{i}]"),
                        out,
                        depth + 1,
                    );
                }
            }
            Property::StructProperty(s) => {
                walk(imports, &s.value, &full, out, depth + 1);
            }
            _ => {
                let mut line = format!(
                    "{{\"path\":{},\"type\":\"{}\"",
                    json_str(&full),
                    type_name(p)
                );
                if let Some(v) = value_json(imports, p) {
                    line.push_str(&format!(",\"value\":{v}"));
                }
                line.push('}');
                out.push(line);
            }
        }
    }
}

fn dump(path: &Path) {
    let Ok(bytes) = fs::read(path) else {
        return;
    };
    let exp = fs::read(path.with_extension("uexp")).ok();
    let name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let parsed = Asset::new(
        Cursor::new(bytes),
        exp.map(Cursor::new),
        EngineVersion::VER_UE4_27,
        None,
        false,
    );
    let Ok(asset) = parsed else {
        println!("{{\"asset\":{},\"error\":\"解析失败\"}}", json_str(&name));
        return;
    };

    let mut sections: Vec<String> = Vec::new();
    for e in &asset.asset_data.exports {
        let Some(normal) = cast!(Export, NormalExport, e) else {
            continue;
        };
        let ename = normal.base_export.object_name.get_owned_content();
        let mut lines: Vec<String> = Vec::new();
        walk(&asset.imports, &normal.properties, "", &mut lines, 0);
        sections.push(format!(
            "{{\"export\":{},\"props\":[{}]}}",
            json_str(&ename),
            lines.join(",")
        ));
    }
    println!(
        "{{\"asset\":{},\"exports\":[{}]}}",
        json_str(&name),
        sections.join(",")
    );
}

fn walk_dir(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk_dir(&p, out);
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
            walk_dir(&p, &mut files);
        } else {
            files.push(p);
        }
    }
    for f in files {
        dump(&f);
    }
}
