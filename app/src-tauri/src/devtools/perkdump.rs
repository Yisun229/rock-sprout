// 读 PERK_*.uasset：把 SaveGameID 拼成 32 位小写 hex（与存档 PerkID 逐字相同），
// 再把 Ranks[] 逐级打出来（每级 Tier + Cost），每资产一行 JSON。
//
//   cargo run --release --features devtools --bin perkdump -- <目录或文件...>
//
// 只读：不写回任何资产。
//
// Ranks 的字节排布没有上游文档，这里不做任何手工对齐——全部交给 unreal_asset 的
// 属性解析器按资产自带的 tag 读；读出来的级数要靠存档 rank 分布交叉验证。

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

/// 16 字节 guid 按文件里的字节顺序拼成 32 位小写 hex。
///
/// 存档里 PerkID 就是这 16 个字节原样写进去的（`_tools/_gvas.mjs` 的 `hex()` 同理），
/// 所以要按字节顺序拼，不能按 FGuid 的 A/B/C/D 四个 uint32 值格式化——那会把每组
/// 四个字节颠倒，和存档里的 id 对不上（21/21 全对不上就是这么撞出来的）。
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
            unreal_asset::properties::int_property::BytePropertyValue::FName(_) => None,
        },
        _ => None,
    }
}

/// SaveGameID 是 struct_type="Guid" 的 StructProperty：FGuid 走自定义序列化，
/// 里面只包一个 GuidProperty。也兼容 A/B/C/D 四段的存法。
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

/// 一个 rank 结构体里挑出 Tier 和 Cost（字段名若不同，把全部字段名带回去）。
fn rank_of(props: &[Property]) -> (Option<i64>, Option<i64>, Vec<String>) {
    let names: Vec<String> = props
        .iter()
        .map(|p| p.get_name().get_owned_content())
        .collect();
    let tier = props
        .iter()
        .find(|p| p.get_name() == "Tier" || p.get_name() == "UnlockTier")
        .and_then(num);
    let cost = props
        .iter()
        .find(|p| p.get_name() == "Cost" || p.get_name() == "PerkPoints")
        .and_then(num);
    (tier, cost, names)
}

fn dump(path: &Path) {
    let Ok(bytes) = fs::read(path) else {
        return;
    };
    let uexp = path.with_extension("uexp");
    let exp = fs::read(&uexp).ok();
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
        println!("{{\"asset\":\"{name}\",\"error\":\"解析失败\"}}");
        return;
    };

    // 一个资产里有多个 export：带 Ranks 的那个才是天赋定义。
    // 取 Ranks 最多的那一条，每资产只打一行。
    let mut guid: Option<String> = None;
    let mut best: Option<(Vec<(Option<i64>, Option<i64>)>, Vec<String>)> = None;
    for e in &asset.asset_data.exports {
        let Some(normal) = cast!(Export, NormalExport, e) else {
            continue;
        };
        let props = &normal.properties;
        if guid.is_none() {
            guid = guid_of(props);
        }
        let mut ranks: Vec<(Option<i64>, Option<i64>)> = Vec::new();
        let mut fields: Vec<String> = Vec::new();
        if let Some(Property::ArrayProperty(arr)) = props.iter().find(|p| p.get_name() == "Ranks") {
            for item in &arr.value {
                let Property::StructProperty(st) = item else {
                    continue;
                };
                let (tier, cost, names) = rank_of(&st.value);
                if fields.is_empty() {
                    fields = names;
                }
                ranks.push((tier, cost));
            }
        }
        if best.as_ref().is_none_or(|(b, _)| ranks.len() > b.len()) {
            best = Some((ranks, fields));
        }
    }

    // 标题：TextProperty 的「表 id + 键 + 英文原文」
    let mut title = String::from("null");
    for e in &asset.asset_data.exports {
        let Some(normal) = cast!(Export, NormalExport, e) else {
            continue;
        };
        if let Some(Property::TextProperty(t)) =
            normal.properties.iter().find(|p| p.get_name() == "Title")
        {
            let table = t
                .table_id
                .as_ref()
                .map(|f| f.get_owned_content())
                .unwrap_or_default();
            let key = t.value.clone().unwrap_or_default();
            let src = t.culture_invariant_string.clone().unwrap_or_default();
            title = format!(
                "{{\"table\":\"{}\",\"key\":\"{}\",\"en\":{}}}",
                table,
                key,
                serde_json::to_string(&src).unwrap_or_else(|_| "null".into())
            );
            break;
        }
    }

    let (ranks, fields) = best.unwrap_or_default();
    let ranks_json = ranks
        .iter()
        .map(|(t, c)| {
            format!(
                "{{\"tier\":{},\"cost\":{}}}",
                t.map(|v| v.to_string()).unwrap_or_else(|| "null".into()),
                c.map(|v| v.to_string()).unwrap_or_else(|| "null".into())
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    println!(
        "{{\"asset\":\"{name}\",\"guid\":{},\"rankCount\":{},\"ranks\":[{}],\"rankFields\":[{}],\"title\":{}}}",
        guid.map(|g| format!("\"{g}\""))
            .unwrap_or_else(|| "null".into()),
        ranks.len(),
        ranks_json,
        fields
            .iter()
            .map(|f| format!("\"{f}\""))
            .collect::<Vec<_>>()
            .join(","),
        title
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
