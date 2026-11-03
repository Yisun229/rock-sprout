//! 读取器的单元测试。
//!
//! 前半部分用「手搓的小存档」测每种类型和坏输入；最后一个测真实存档，
//! 样本不在本机时自动跳过（样本不进仓库）。

use super::gvas::{binary_struct_size, parse as raw_parse};
use super::{parse, ErrorKind, Value};

// ---------- 造字节的小工具 ----------

fn w_u16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn w_u32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn w_i32(v: &mut Vec<u8>, x: i32) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn w_fstring(v: &mut Vec<u8>, s: &str) {
    let mut bytes = s.as_bytes().to_vec();
    bytes.push(0);
    w_i32(v, bytes.len() as i32);
    v.extend_from_slice(&bytes);
}

/// 容器头：与真实存档同形，但自定义版本留空。
fn header() -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(b"GVAS");
    w_i32(&mut v, 2); // saveGameFileVersion
    w_i32(&mut v, 522); // packageUEVersion
    w_u16(&mut v, 4);
    w_u16(&mut v, 27);
    w_u16(&mut v, 2);
    w_u32(&mut v, 2147578044);
    w_fstring(&mut v, "++UE4+Release-4.27");
    w_i32(&mut v, 3); // customVersionFormat
    w_i32(&mut v, 0); // 自定义版本条数
    w_fstring(&mut v, "/Script/FSD.FSDSaveGame");
    v
}

/// 普通类型的标签：名字 / 类型 / 大小 / 下标 / 无属性 GUID
fn tag_plain(v: &mut Vec<u8>, name: &str, ty: &str, size: i32) {
    w_fstring(v, name);
    w_fstring(v, ty);
    w_i32(v, size);
    w_i32(v, 0);
    v.push(0);
}

/// 布尔标签：值那 1 个字节写在标签里，体是 0 字节
fn tag_bool(v: &mut Vec<u8>, name: &str, value: bool) {
    w_fstring(v, name);
    w_fstring(v, "BoolProperty");
    w_i32(v, 0);
    w_i32(v, 0);
    v.push(u8::from(value));
    v.push(0);
}

/// 结构体标签：多一个结构体名 + 16 字节 struct GUID（见 gvas.rs 模块文档第 1 条）
fn tag_struct(v: &mut Vec<u8>, name: &str, size: i32, struct_name: &str) {
    w_fstring(v, name);
    w_fstring(v, "StructProperty");
    w_i32(v, size);
    w_i32(v, 0);
    w_fstring(v, struct_name);
    v.extend_from_slice(&[0u8; 16]);
    v.push(0);
}

/// 数组标签：多一个元素类型
fn tag_array(v: &mut Vec<u8>, name: &str, size: i32, inner: &str) {
    w_fstring(v, name);
    w_fstring(v, "ArrayProperty");
    w_i32(v, size);
    w_i32(v, 0);
    w_fstring(v, inner);
    v.push(0);
}

fn tag_set(v: &mut Vec<u8>, name: &str, size: i32, inner: &str) {
    w_fstring(v, name);
    w_fstring(v, "SetProperty");
    w_i32(v, size);
    w_i32(v, 0);
    w_fstring(v, inner);
    v.push(0);
}

fn tag_map(v: &mut Vec<u8>, name: &str, size: i32, key_ty: &str, value_ty: &str) {
    w_fstring(v, name);
    w_fstring(v, "MapProperty");
    w_i32(v, size);
    w_i32(v, 0);
    w_fstring(v, key_ty);
    w_fstring(v, value_ty);
    v.push(0);
}

fn tag_none(v: &mut Vec<u8>) {
    w_fstring(v, "None");
}

fn int_prop(v: &mut Vec<u8>, name: &str, x: i32) {
    tag_plain(v, name, "IntProperty", 4);
    w_i32(v, x);
}

// ---------- 测试 ----------

#[test]
fn parses_plain_types() {
    let mut v = header();
    int_prop(&mut v, "Credits", 21216);
    tag_plain(&mut v, "PlayTime", "FloatProperty", 4);
    v.extend_from_slice(&346598.531f32.to_le_bytes());
    tag_plain(&mut v, "Nick", "StrProperty", 0);
    w_fstring(&mut v, "sprout");
    tag_bool(&mut v, "Flag", true);
    tag_none(&mut v);

    let out = parse(&v).expect("应该能读");
    assert_eq!(out.header.save_class, "/Script/FSD.FSDSaveGame");
    assert_eq!(out.header.engine.major, 4);
    assert_eq!(out.header.engine.minor, 27);

    let credits = super::find(&out.props, "Credits").unwrap();
    assert_eq!(credits.value, Value::Int(21216));

    let play = super::find(&out.props, "PlayTime").unwrap();
    match play.value {
        Value::Float(f) => assert!((f - 346598.531).abs() < 0.01),
        ref other => panic!("PlayTime 应该是 Float，实际 {other:?}"),
    }

    let nick = super::find(&out.props, "Nick").unwrap();
    assert_eq!(nick.value, Value::Str("sprout".to_string()));

    let flag = super::find(&out.props, "Flag").unwrap();
    assert_eq!(flag.value, Value::Bool(true));
    assert_eq!(out.stats.skipped, 0);
    assert!(out.notes.is_empty(), "不该有说明：{:?}", out.notes);
}

#[test]
fn parses_struct_and_arrays() {
    let mut chars = Vec::new();
    int_prop(&mut chars, "Xp", 8121);
    tag_none(&mut chars);

    let mut v = header();
    let body_len = chars.len() as i32;
    tag_struct(&mut v, "Player", body_len, "CharacterSave");
    v.extend_from_slice(&chars);

    // 3 个 Int：整数数组没有元素标签，元素裸值首尾相接
    tag_array(&mut v, "Numbers", 16, "IntProperty");
    w_i32(&mut v, 3);
    w_i32(&mut v, 11);
    w_i32(&mut v, 22);
    w_i32(&mut v, 33);

    // 2 个裸 Guid 结构体：元素 size 是总和 32
    tag_array(&mut v, "Ids", 97, "StructProperty");
    w_i32(&mut v, 2);
    tag_struct(&mut v, "Ids", 32, "Guid");
    v.extend_from_slice(&[1u8; 16]);
    v.extend_from_slice(&[2u8; 16]);

    tag_none(&mut v);

    let out = parse(&v).expect("应该能读");
    let player = super::find(&out.props, "Player").unwrap();
    match &player.value {
        Value::Struct(s) => {
            assert_eq!(s.name.as_deref(), Some("CharacterSave"));
            let xp = super::find(&s.props, "Xp").unwrap();
            assert_eq!(xp.value, Value::Int(8121));
        }
        other => panic!("Player 应该是结构体，实际 {other:?}"),
    }

    let numbers = super::find(&out.props, "Numbers").unwrap();
    match &numbers.value {
        Value::Array(a) => {
            assert_eq!(a.count, 3);
            assert_eq!(a.items, vec![Value::Int(11), Value::Int(22), Value::Int(33)]);
        }
        other => panic!("Numbers 应该是数组，实际 {other:?}"),
    }

    let ids = super::find(&out.props, "Ids").unwrap();
    match &ids.value {
        Value::Array(a) => {
            assert_eq!(a.count, 2);
            assert_eq!(a.items.len(), 2);
            assert!(matches!(a.items[0], Value::Binary(_)));
        }
        other => panic!("Ids 应该是数组，实际 {other:?}"),
    }
    assert!(out.notes.is_empty(), "不该有说明：{:?}", out.notes);
}

#[test]
fn parses_map_and_set() {
    let mut v = header();

    // Set：前导 4 字节恒 0 + 数量 2 + 2 个 Guid
    tag_set(&mut v, "Seen", 8 + 32, "StructProperty");
    w_i32(&mut v, 0);
    w_i32(&mut v, 2);
    v.extend_from_slice(&[7u8; 16]);
    v.extend_from_slice(&[8u8; 16]);

    // Map：前导 4 字节恒 0 + 数量 1 + Guid 键 + 一段属性表
    let mut value_body = Vec::new();
    int_prop(&mut value_body, "Count", 69);
    tag_none(&mut value_body);
    let map_size = 8 + 16 + value_body.len();
    tag_map(&mut v, "Kills", map_size as i32, "StructProperty", "StructProperty");
    w_i32(&mut v, 0);
    w_i32(&mut v, 1);
    v.extend_from_slice(&[9u8; 16]);
    v.extend_from_slice(&value_body);

    tag_none(&mut v);

    let out = parse(&v).expect("应该能读");
    let seen = super::find(&out.props, "Seen").unwrap();
    match &seen.value {
        Value::Set(s) => {
            assert_eq!(s.count, 2);
            assert_eq!(s.items.len(), 2);
        }
        other => panic!("Seen 应该是集合，实际 {other:?}"),
    }

    let kills = super::find(&out.props, "Kills").unwrap();
    match &kills.value {
        Value::Map(m) => {
            assert_eq!(m.count, 1);
            assert_eq!(m.entries.len(), 1);
            match &m.entries[0].1 {
                Value::Struct(s) => {
                    let c = super::find(&s.props, "Count").unwrap();
                    assert_eq!(c.value, Value::Int(69));
                }
                other => panic!("映射值应该是结构体，实际 {other:?}"),
            }
        }
        other => panic!("Kills 应该是映射，实际 {other:?}"),
    }
    assert!(out.notes.is_empty(), "不该有说明：{:?}", out.notes);
}

#[test]
fn unknown_type_is_skipped_with_note() {
    let mut v = header();
    int_prop(&mut v, "Before", 1);
    tag_plain(&mut v, "Mystery", "QuantumProperty", 6);
    v.extend_from_slice(&[0xAA; 6]);
    int_prop(&mut v, "After", 2);
    tag_none(&mut v);

    let out = parse(&v).expect("不认识也要读得下去");
    assert_eq!(out.stats.skipped, 1);
    let note = out.notes.iter().find(|n| n.path == "Mystery").expect("要留一条说明");
    assert_eq!(note.kind.code(), "skipped");
    assert!(note.detail.contains("跳过：Mystery"), "说明要写清路径：{}", note.detail);

    let mystery = super::find(&out.props, "Mystery").unwrap();
    assert!(matches!(mystery.value, Value::Skipped(_)));

    // 跳过之后紧跟的属性必须还能读对
    let after = super::find(&out.props, "After").unwrap();
    assert_eq!(after.value, Value::Int(2));
}

#[test]
fn truncated_file_is_error_not_panic() {
    let mut v = header();
    int_prop(&mut v, "Credits", 21216);
    tag_plain(&mut v, "Nick", "StrProperty", 0);
    w_fstring(&mut v, "sprout");
    tag_none(&mut v);

    for cut in [v.len() - 2, v.len() / 2, 20] {
        let err = parse(&v[..cut]).expect_err("截断要报错");
        assert!(
            matches!(err.kind, ErrorKind::Truncated | ErrorKind::BadString),
            "第 {cut} 字节处应是截断类错误，实际 {:?}",
            err.kind
        );
        assert!(!err.detail.is_empty());
    }
}

#[test]
fn not_a_save_reports_not_gvas() {
    let err = parse(b"XXXX0000").expect_err("不是 GVAS 要报错");
    assert_eq!(err.kind, ErrorKind::NotGvas);
    assert_eq!(err.code(), "not_gvas");
    assert!(err.kind.human().contains("GVAS"));
}

#[test]
fn absurd_count_stops_instead_of_spinning() {
    let mut v = header();
    tag_array(&mut v, "Huge", 4, "IntProperty");
    w_i32(&mut v, 50_000_000);
    tag_plain(&mut v, "Huge", "IntProperty", 4);
    w_i32(&mut v, 1);
    tag_none(&mut v);

    let err = parse(&v).expect_err("数量离谱要停下");
    assert!(
        matches!(err.kind, ErrorKind::Truncated | ErrorKind::Budget | ErrorKind::BadCount),
        "应该是截断/预算/数量之一，实际 {:?}",
        err.kind
    );
}

#[test]
fn binary_struct_table_matches_doc() {
    assert_eq!(binary_struct_size("Guid"), Some(16));
    assert_eq!(binary_struct_size("Vector"), Some(12));
    assert_eq!(binary_struct_size("DateTime"), Some(8));
    assert_eq!(binary_struct_size("NotAStruct"), None);
}

// ---------- 真实存档 ----------

fn sample_path() -> Option<std::path::PathBuf> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../_tools/save-samples/76561199027062409_Player.sav");
    if p.is_file() {
        Some(p)
    } else {
        None
    }
}

#[test]
fn real_sample_parses_with_expected_numbers() {
    let Some(path) = sample_path() else {
        eprintln!("样本不在本机，跳过（样本不进仓库）");
        return;
    };
    let bytes = std::fs::read(&path).expect("读样本");
    let out = raw_parse(&bytes).expect("真实存档应该能读");

    assert_eq!(out.header.save_game_file_version, 2);
    assert_eq!(out.header.package_ue_version, 522);
    assert_eq!(out.header.save_class, "/Script/FSD.FSDSaveGame");
    assert_eq!(out.header.custom_versions.len(), 51);

    // M0 立下的自检口径：收尾落在「文件尾 − 4」，且一个属性都没跳过
    assert_eq!(out.consumed, bytes.len() - 4, "收尾位置不对");
    assert_eq!(out.stats.skipped, 0, "不该跳过任何属性");
    assert!(out.notes.is_empty(), "不该有说明：{:?}", out.notes);
    // 与 JS 原型对表：JS 数出 6560，这里 6562，差 2 个是计次口径不同
    // （数组/集合元素在两边记法不一致）；数值、收尾、跳过条数完全一致
    assert_eq!(out.stats.nodes, 6562, "节点数要和 JS 原型对得上（差 2 个计次）");

    let credits = super::find(&out.props, "Credits").unwrap();
    assert_eq!(credits.value, Value::Int(21216));

    let games = super::find(&out.props, "NumberOfGamesPlayed").unwrap();
    assert_eq!(games.value, Value::Int(199));

    let play = super::find(&out.props, "TotalPlayTimeSeconds").unwrap();
    match play.value {
        Value::Float(f) => assert!((f - 346598.531).abs() < 1.0, "总时长是 {f}"),
        ref other => panic!("TotalPlayTimeSeconds 应该是 Float，实际 {other:?}"),
    }

    let chars = super::find(&out.props, "CharacterSaves").unwrap();
    match &chars.value {
        Value::Array(a) => assert_eq!(a.count, 5),
        other => panic!("CharacterSaves 应该是数组，实际 {other:?}"),
    }

    let res = super::find_path(&out.props, "Resources.OwnedResources").unwrap();
    match &res.value {
        Value::Map(m) => {
            assert_eq!(m.key_type, "StructProperty");
            assert_eq!(m.value_type, "FloatProperty");
            assert_eq!(m.entries.len(), 14);
        }
        other => panic!("OwnedResources 应该是映射，实际 {other:?}"),
    }
}

// ---------- 只读四步 + 缓存 ----------

/// 临时目录：跑完删掉，测试之间不互相影响。
struct TempDir {
    root: std::path::PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "rock-sprout-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&root).expect("建临时目录");
        Self { root }
    }

    fn path(&self, name: &str) -> std::path::PathBuf {
        self.root.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// 一份能过版本闸门的小存档。
fn small_save() -> Vec<u8> {
    let mut v = header();
    int_prop(&mut v, "Credits", 21216);
    tag_plain(&mut v, "PlayTime", "FloatProperty", 4);
    v.extend_from_slice(&346598.531f32.to_le_bytes());
    tag_none(&mut v);
    v
}

#[test]
fn store_second_read_hits_cache_and_bytes_are_identical() {
    let dir = TempDir::new("store-hit");
    let save = dir.path("Player.sav");
    std::fs::write(&save, small_save()).expect("写测试存档");
    let cache = dir.path("cache");

    let (doc1, r1) = super::store::load(&save, &cache).expect("第一次应该能读");
    assert!(!r1.from_cache, "第一次不该命中缓存");
    assert_eq!(r1.kind_text(), "新解析");
    assert!(r1.untouched(), "原档前后哈希必须一致");
    // 复制出来的副本必须和原档同一个哈希
    assert_eq!(r1.sha256_copy.as_deref(), Some(r1.sha256_before.as_str()));

    let (doc2, r2) = super::store::load(&save, &cache).expect("第二次应该能读");
    assert!(r2.from_cache, "第二次必须命中缓存");
    assert_eq!(r2.kind_text(), "命中缓存");
    assert!(r2.untouched(), "命中缓存时也要重新验一次原档哈希");

    // 硬指标一：两次结果字节级相同
    let b1 = super::store::render(&doc1).expect("渲染第一次");
    let b2 = super::store::render(&doc2).expect("渲染第二次");
    assert_eq!(b1, b2, "两次解析结果必须字节级相同");
    assert_eq!(doc1, doc2, "两次解析结果（结构）必须相等");

    // 缓存文件里存的正是这份字节
    let on_disk = std::fs::read(&r2.cache_path).expect("缓存文件应该在");
    assert_eq!(on_disk, b1, "缓存文件内容必须等于渲染结果");
    assert_eq!(on_disk.len(), r2.cache_bytes);

    // 缓存文件名是 sha256 前 16 位
    let name = std::path::Path::new(&r2.cache_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    assert_eq!(name, format!("{}.json", &r1.sha256_before[..16]));
}

#[test]
fn store_clear_cache_then_reparse_gives_same_result() {
    let dir = TempDir::new("store-clear");
    let save = dir.path("Player.sav");
    std::fs::write(&save, small_save()).expect("写测试存档");
    let cache = dir.path("cache");

    let (doc1, r1) = super::store::load(&save, &cache).expect("第一次");
    assert_eq!(super::store::clear(&cache).expect("清理缓存"), 1, "应该删掉 1 个缓存文件");
    assert!(!std::path::Path::new(&r1.cache_path).exists(), "缓存文件应该被删掉");

    let (doc2, r2) = super::store::load(&save, &cache).expect("清理后再读");
    assert!(!r2.from_cache, "缓存清掉后必须重新解析");
    assert_eq!(super::store::render(&doc1).unwrap(), super::store::render(&doc2).unwrap());
    assert_eq!(super::store::clear(&cache).expect("再清一次"), 1);
    assert_eq!(super::store::clear(&cache).expect("空目录再清"), 0);
}

#[test]
fn store_reports_three_error_classes() {
    let dir = TempDir::new("store-errors");
    let cache = dir.path("cache");

    // 一：根本不是存档
    let not_save = dir.path("random.sav");
    std::fs::write(&not_save, b"not a save at all").expect("写");
    let e = super::store::load(&not_save, &cache).expect_err("应该报错");
    assert_eq!(e.class, super::store::LoadClass::NotSave);
    assert_eq!(e.class.code(), "not_save");
    assert!(e.to_string().contains("不是《深岩银河》的存档"));

    // 二：是存档，但版本不认识（把包版本 522 改成 999）
    let mut bad_version = small_save();
    bad_version[4 + 4..4 + 8].copy_from_slice(&999i32.to_le_bytes());
    let p2 = dir.path("future.sav");
    std::fs::write(&p2, &bad_version).expect("写");
    let e = super::store::load(&p2, &cache).expect_err("应该报错");
    assert_eq!(e.class, super::store::LoadClass::UnknownVersion);
    assert_eq!(e.class.code(), "unknown_version");
    assert!(e.detail.contains("包版本是 999"), "实际：{}", e.detail);

    // 三：版本认得，但结构对不上（砍掉后半截）
    let full = small_save();
    let cut = dir.path("broken.sav");
    std::fs::write(&cut, &full[..full.len() - 20]).expect("写");
    let e = super::store::load(&cut, &cache).expect_err("应该报错");
    assert_eq!(e.class, super::store::LoadClass::BadStructure);
    assert_eq!(e.class.code(), "bad_structure");
}
