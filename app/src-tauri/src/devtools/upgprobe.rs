// 一次性探针：把一个资产里的 Text / Object / Struct / Enum 属性按「名字 || 值」打出来，
// 用来确认 FText 里带不带 namespace+key、以及引用字段长什么样。
//
//   cargo run --release --features devtools --bin upgprobe -- <x.uasset>
//
// 只读：不写回任何资产。

use std::fs;
use std::io::Cursor;
use std::path::Path;

use unreal_asset::{
    cast,
    engine_version::EngineVersion,
    exports::Export,
    properties::{Property, PropertyDataTrait},
    Asset,
};

fn main() {
    let path = std::env::args().nth(1).expect("用法: upgprobe <x.uasset>");
    let path = Path::new(&path);
    let Ok(bytes) = fs::read(path) else {
        eprintln!("读不到文件");
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
        eprintln!("解析失败");
        return;
    };
    for e in &asset.asset_data.exports {
        let Some(normal) = cast!(Export, NormalExport, e) else {
            continue;
        };
        for p in &normal.properties {
            let name = p.get_name().get_owned_content();
            match p {
                Property::TextProperty(t) => println!("{name} || {t:?}"),
                Property::ObjectProperty(o) => println!("{name} || {o:?}"),
                Property::StructProperty(s) => println!("{name} || {s:?}"),
                Property::EnumProperty(en) => println!("{name} || {en:?}"),
                _ => {}
            }
        }
    }
}
