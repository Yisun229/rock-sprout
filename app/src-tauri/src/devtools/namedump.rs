// 读一批资产，把「可能当名字用的字段」全部摊出来：每个资产一行 JSON，
// 带上 SaveGameID 与所有 TextProperty / StrProperty / NameProperty。
//
//   cargo run --release --features devtools --bin namedump -- <目录或文件...>
//
// 用途：先看资产里到底有哪几个名字字段、FText 的 table_id 与键长什么样，
// 再决定五张表各取哪个字段。只读，不写回任何资产。

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

fn guid_hex(bytes: [u8; 16]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(32);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn num(p: &Property) -> Option<i64> {
    match p {
        Property::IntProperty(v) => Some(i64::from(v.value)),
        Property::Int64Property(v) => Some(v.value),
        Property::ByteProperty(v) => match &v.value {
            unreal_asset::properties::int_property::BytePropertyValue::Byte(b) => {
                Some(i64::from(*b))
            }
            _ => None,
        },
        _ => None,
    }
}

/// SaveGameID 常见存法：GuidProperty 本身，或 struct_type=Guid 的 StructProperty 里包一个。
fn guid_of(props: &[Property]) -> Option<String> {
    let p = props.iter().find(|p| p.get_name() == "SaveGameID")?;
    match p {
        Property::GuidProperty(g) => Some(guid_hex(g.value.0)),
        Property::StructProperty(st) => match st.value.first() {
            Some(Property::GuidProperty(g)) => Some(guid_hex(g.value.0)),
            _ => {
                let get = |n: &str| {
                    st.value
                        .iter()
                        .find(|p| p.get_name() == n)
                        .and_then(num)
                        .and_then(|v| u32::try_from(v).ok())
                };
                match (get("A"), get("B"), get("C"), get("D")) {
                    (Some(a), Some(b), Some(c), Some(d)) => {
                        Some(format!("{a:08x}{b:08x}{c:08x}{d:08x}"))
                    }
                    _ => None,
                }
            }
        },
        _ => None,
    }
}

fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

/// 把一条属性渲染成 JSON 描述；不认识的类型返回 None。
fn field_json(p: &Property) -> Option<String> {
    let name = p.get_name().get_owned_content();
    match p {
        Property::TextProperty(t) => {
            let table = t
                .table_id
                .as_ref()
                .map(|f| f.get_owned_content())
                .unwrap_or_default();
            let key = t.value.clone().unwrap_or_default();
            let en = t.culture_invariant_string.clone().unwrap_or_default();
            Some(format!(
                "{{\"prop\":{},\"kind\":\"text\",\"table\":{},\"key\":{},\"en\":{}}}",
                json_str(&name),
                json_str(&table),
                json_str(&key),
                json_str(&en)
            ))
        }
        Property::StrProperty(s) => Some(format!(
            "{{\"prop\":{},\"kind\":\"str\",\"value\":{}}}",
            json_str(&name),
            json_str(s.value.as_deref().unwrap_or(""))
        )),
        Property::NameProperty(n) => Some(format!(
            "{{\"prop\":{},\"kind\":\"name\",\"value\":{}}}",
            json_str(&name),
            json_str(&n.value.get_owned_content())
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
        Property::BoolProperty(_) => "Bool",
        Property::ByteProperty(_) => "Byte",
        Property::FloatProperty(_) => "Float",
        _ => "Other",
    }
}

/// 递归摊开属性：`shape` 记"字段名:类型"（数组写出元素类型），
/// `refs` 记对象引用按出现顺序解析出的 import 名——挑战资产的 StatIndices
/// 就是下标进这个顺序。
fn walk_props(
    imports: &[unreal_asset::Import],
    props: &[Property],
    path: &str,
    shape: &mut Vec<String>,
    refs: &mut Vec<String>,
    texts: &mut Vec<String>,
) {
    for p in props {
        let fname = p.get_name().get_owned_content();
        let full = if path.is_empty() {
            fname
        } else {
            format!("{path}.{fname}")
        };
        match p {
            Property::ArrayProperty(a) => {
                let inner = a.value.first().map(type_name).unwrap_or("?");
                shape.push(format!("{full}:Array<{inner}>"));
                for (i, item) in a.value.iter().enumerate() {
                    walk_props(
                        imports,
                        std::slice::from_ref(item),
                        &format!("{full}[{i}]"),
                        shape,
                        refs,
                        texts,
                    );
                }
            }
            Property::StructProperty(s) => {
                shape.push(format!("{full}:Struct"));
                walk_props(imports, &s.value, &full, shape, refs, texts);
            }
            Property::ObjectProperty(o) => {
                let idx = o.value.index;
                // 这个 crate 的 PackageIndex：负数是 import（import 下标 = -idx - 1），
                // 正数是本包 export，0 是 null。
                let target = if idx < 0 {
                    imports
                        .get((-idx - 1) as usize)
                        .map(|i| i.object_name.get_owned_content())
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                refs.push(format!(
                    "{{\"path\":{},\"index\":{},\"target\":{}}}",
                    json_str(&full),
                    idx,
                    json_str(&target)
                ));
            }
            Property::TextProperty(t) => {
                shape.push(format!("{full}:Text"));
                let table = t
                    .table_id
                    .as_ref()
                    .map(|f| f.get_owned_content())
                    .unwrap_or_default();
                let key = t.value.clone().unwrap_or_default();
                let en = t.culture_invariant_string.clone().unwrap_or_default();
                texts.push(format!(
                    "{{\"path\":{},\"table\":{},\"key\":{},\"en\":{}}}",
                    json_str(&full),
                    json_str(&table),
                    json_str(&key),
                    json_str(&en)
                ));
            }
            _ => shape.push(format!("{full}:{}", type_name(p))),
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

    let mut guid: Option<String> = None;
    let mut fields: Vec<String> = Vec::new();
    let mut shape: Vec<String> = Vec::new();
    let mut refs: Vec<String> = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    for e in &asset.asset_data.exports {
        let Some(normal) = cast!(Export, NormalExport, e) else {
            continue;
        };
        if guid.is_none() {
            guid = guid_of(&normal.properties);
        }
        walk_props(
            &asset.imports,
            &normal.properties,
            "",
            &mut shape,
            &mut refs,
            &mut texts,
        );
        for p in &normal.properties {
            if let Some(j) = field_json(p) {
                fields.push(j);
            }
        }
    }
    println!(
        "{{\"asset\":{},\"guid\":{},\"fields\":[{}],\"shape\":[{}],\"refs\":[{}],\"texts\":[{}]}}",
        json_str(&name),
        guid.map(|g| json_str(&g)).unwrap_or_else(|| "null".into()),
        fields.join(","),
        shape.iter().map(|x| json_str(x)).collect::<Vec<_>>().join(","),
        refs.join(","),
        texts.join(",")
    );
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
