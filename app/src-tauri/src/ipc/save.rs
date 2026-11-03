//! 「我的数据」页要的东西：挑一份存档，从里面取几个数给概览用。
//!
//! 只读：调用 [`crate::save::store::load`]，原档不打开成可写，也不复制到别处去。

use std::time::UNIX_EPOCH;

use serde::Serialize;

use super::ApiResult;
use crate::paths;
use crate::save::locate::{self, SaveFile};
use crate::save::store::{self, LoadClass};

/// 四个数字 + 角色槽的分布。
#[derive(Debug, Serialize)]
pub struct Numbers {
    /// `Credits`：金币
    pub credits: i64,
    /// `TotalPlayTimeSeconds`：总时长（秒，界面自己换算成小时）
    pub play_seconds: f64,
    /// `NumberOfGamesPlayed`：局数
    pub games_played: i64,
    /// 角色槽总数
    pub slots_total: usize,
    /// 有进度的槽（经验 > 0）
    pub slots_active: usize,
    /// 还没进度的槽（经验 = 0）
    pub slots_idle: usize,
}

/// 选择器里的一项。
#[derive(Debug, Serialize)]
pub struct SaveCandidate {
    /// 存档文件名，也是这一项的 id（账号不同、文件名就不同）
    pub id: String,
    /// 账号短号：文件名里的 Steam ID 取末 6 位，够分辨、又不至于糊一屏数字
    pub account: String,
    /// 局数；读不动时是 -1（界面据这个说「读不了」）
    pub games: i64,
    /// 修改时间（unix 毫秒），界面自己排版
    pub modified_ms: u64,
    /// 是不是当前选中的那份
    pub active: bool,
}

/// 选择器要的清单。
#[derive(Debug, Serialize)]
pub struct SaveList {
    pub items: Vec<SaveCandidate>,
    /// 当前选中的是哪个 id；一份都没有时为 null
    pub selected: Option<String>,
}

/// 一种资源的数量。
#[derive(Debug, Clone, Serialize)]
pub struct ResourceCount {
    /// GUID 的小写 hex（32 位）
    pub id: String,
    pub count: i64,
}

/// 概览页的全部数据。
#[derive(Debug, Serialize)]
pub struct SaveOverview {
    /// `ok` / `not_found` / `unknown_version` / `bad_structure`
    pub state: &'static str,
    /// 给用户看的一句话（三种失败各说各的）
    pub note: String,
    /// 读的是哪份存档（只有文件名，不给完整路径——路径在设置页的数据位置里有）
    pub file_name: Option<String>,
    /// 命中缓存时为 true（第二次打开会快，结果一样）
    pub from_cache: bool,
    pub numbers: Option<Numbers>,
    /// 存档里有几种资源（界面那块要不要显示、要不要起名字由页面决定）
    pub resources_total: usize,
    /// 每种资源一条：id + 数量，按数量从多到少。id 是 GUID 的小写 hex，
    /// 起名字的对照表在前端（`app/src/data/names.json`），这里只给 id。
    pub resources: Vec<ResourceCount>,
    /// 一共读出来几块结构（七块）
    pub blocks_read: usize,
    /// 其中还没做界面的块数
    pub blocks_pending: usize,
}

impl SaveOverview {
    fn with_state(state: &'static str, note: impl Into<String>) -> Self {
        Self {
            state,
            note: note.into(),
            file_name: None,
            from_cache: false,
            numbers: None,
            resources_total: 0,
            resources: Vec::new(),
            blocks_read: 0,
            blocks_pending: 0,
        }
    }
}

/// 账号短号：`76561199218191195_Player.sav` → `…191195`
fn account_of(name: &str) -> String {
    let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() <= 6 {
        return digits;
    }
    format!("…{}", &digits[digits.len() - 6..])
}

fn modified_ms(file: &SaveFile) -> u64 {
    file.modified
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 挑当前该看哪一份：调用方指定的 > 上次记住的 > 最新改过的。
fn pick(selected: Option<&str>) -> Option<SaveFile> {
    let all = locate::candidates();
    if all.is_empty() {
        return None;
    }
    let remembered = selected
        .map(|s| s.to_owned())
        .or_else(|| crate::ui_state::load(&paths::data_root()).save_file);
    if let Some(want) = remembered {
        if let Some(hit) = all.iter().find(|f| f.path.file_name().and_then(|s| s.to_str()) == Some(want.as_str())) {
            return Some(hit.clone());
        }
    }
    all.into_iter().next()
}

/// 选择器清单：这台机器上有几份账号主档，各自打了多少局、什么时候改的。
///
/// 为了显示局数，每份都要读一遍——读过的走缓存，所以第二次打开很快。
#[tauri::command]
pub fn save_list() -> ApiResult<SaveList> {
    let data_root = paths::data_root();
    let cache_root = data_root.join("cache");
    let chosen = pick(None);
    let chosen_name = chosen
        .as_ref()
        .and_then(|f| f.path.file_name().map(|s| s.to_string_lossy().to_string()));

    let items = locate::candidates()
        .into_iter()
        .map(|f| {
            let name = f.path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let games = store::load(&f.path, &cache_root)
                .map(|(doc, _)| i64::from(doc.data.NumberOfGamesPlayed))
                .unwrap_or(-1);
            SaveCandidate {
                account: account_of(&name),
                active: chosen_name.as_deref() == Some(name.as_str()),
                id: name,
                games,
                modified_ms: modified_ms(&f),
            }
        })
        .collect();

    ApiResult::ok(SaveList { items, selected: chosen_name })
}

/// 记住「用哪份存档」——下次打开还是这份。
#[tauri::command]
pub fn set_save_file(file: Option<String>) -> ApiResult<bool> {
    let data_root = paths::data_root();
    let mut state = crate::ui_state::load(&data_root);
    state.save_file = file;
    match crate::ui_state::save(&data_root, &state) {
        Ok(()) => ApiResult::ok(true),
        Err(message) => {
            tracing::warn!(%message, "选的存档没记住（这一次仍然生效）");
            ApiResult::err("io", message)
        }
    }
}

/// 概览：读出选中那份账号主档的四个数。
#[tauri::command]
pub fn save_overview(selected: Option<String>) -> ApiResult<SaveOverview> {
    let Some(file) = pick(selected.as_deref()) else {
        return ApiResult::ok(SaveOverview::with_state(
            "not_found",
            "没找到存档：这台机器上还没存过档，或者装的是别的版本（目前只认 Steam 版）",
        ));
    };

    let cache_root = paths::data_root().join("cache");
    match store::load(&file.path, &cache_root) {
        Ok((doc, report)) => {
            let data = &doc.data;
            let slots_total = data.CharacterSaves.len();
            let slots_active = data.CharacterSaves.iter().filter(|c| c.XP > 0).count();
            let numbers = Numbers {
                credits: i64::from(data.Credits),
                play_seconds: f64::from(data.TotalPlayTimeSeconds),
                games_played: i64::from(data.NumberOfGamesPlayed),
                slots_total,
                slots_active,
                // 经验 0 就是「还没进度」。存档里没有「这个槽建没建过人」的可靠标记
                // （SavegameID 在这份存档里五个槽全是 0），所以不猜，只报进度。
                slots_idle: slots_total.saturating_sub(slots_active),
            };
            ApiResult::ok(SaveOverview {
                state: "ok",
                note: String::new(),
                file_name: file.path.file_name().map(|s| s.to_string_lossy().to_string()),
                from_cache: report.from_cache,
                numbers: Some(numbers),
                resources_total: data.Resources.OwnedResources.len(),
                resources: {
                    let mut list: Vec<ResourceCount> = data
                        .Resources
                        .OwnedResources
                        .iter()
                        .map(|(id, n)| ResourceCount {
                            id: id.to_lowercase(),
                            count: (f64::from(*n)).round() as i64,
                        })
                        .collect();
                    list.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.id.cmp(&b.id)));
                    list
                },
                blocks_read: 7,
                blocks_pending: 6,
            })
        }
        Err(e) => {
            let state = match e.class {
                LoadClass::NotSave => "bad_structure",
                LoadClass::UnknownVersion => "unknown_version",
                LoadClass::BadStructure => "bad_structure",
            };
            let mut out = SaveOverview::with_state(state, e.class.human().to_string());
            out.file_name = file.path.file_name().map(|s| s.to_string_lossy().to_string());
            ApiResult::ok(out)
        }
    }
}

/// 开发者视图的一行：字段路径 / 类型 / 值 / 这个字段有没有被我们读进结构体。
#[derive(Debug, Serialize)]
pub struct FieldRow {
    pub path: String,
    pub ty: String,
    pub value: String,
    /// `true` 表示这一支在 `model.rs` 里有对应的结构体字段（字段名与含义已确认）
    pub known: bool,
}

/// 开发者视图：七块的字段探查表 + 这一档的哈希与解析统计。
///
/// 不写缓存、不复制文件——只是把同一份存档再读一遍给人看。
#[derive(Debug, Serialize)]
pub struct SaveDebug {
    pub state: &'static str,
    pub note: String,
    pub file_name: Option<String>,
    pub sha256: String,
    pub file_bytes: usize,
    pub nodes: usize,
    pub consumed: usize,
    pub skipped: usize,
    pub notes: Vec<String>,
    pub rows: Vec<FieldRow>,
    /// 表里没列出来的行数（数组只摊开第一项，其余按条数收着）
    pub rows_hidden: usize,
}

/// `model.rs` 真正读进结构体的顶层字段。其余字段在开发者视图里标「未确认」。
const KNOWN_ROOTS: &[&str] = &[
    "VersionNumber",
    "SaveCreatedInPatch",
    "Credits",
    "PerkPoints",
    "TotalPlayTimeSeconds",
    "NumberOfGamesPlayed",
    "LastPlayedCharacter",
    "CharacterSaves",
    "Resources",
    "SchematicSave",
    "ItemUpgradeLoadouts",
    "PurchasedItemUpgrades",
    "WeaponMaintenance",
    "VanityMasterySave",
    "UnlockedItemSkins",
    "UnlockedPickaxeParts",
    "Milestones",
    "SeasonSave",
    "CampaignSave",
    "DeepDiveSave",
    "OwnedPerks",
    "EquippedPerkLoadouts",
    "RandomEquippedPerkLoadout",
];

/// 一行值的短预览。只给人扫一眼用，完整内容留给后面的版本。
fn preview(value: &crate::save::Value) -> String {
    use crate::save::Value;
    match value {
        Value::Int(v) => v.to_string(),
        Value::Int64(v) => v.to_string(),
        Value::UInt32(v) => v.to_string(),
        Value::UInt64(v) => v.to_string(),
        Value::Float(v) => format!("{v}"),
        Value::Double(v) => format!("{v}"),
        Value::Bool(v) => v.to_string(),
        Value::Str(s) | Value::Object(s) | Value::Enum(s) => {
            let cut: String = s.chars().take(80).collect();
            format!("\"{cut}\"")
        }
        Value::Byte(b) => format!("{} ({})", b.number, b.name.clone().unwrap_or_default()),
        Value::Array(a) => format!("{} 项 / {}", a.count, a.elem_type),
        Value::Map(m) => format!("{} 条 / {} → {}", m.count, m.key_type, m.value_type),
        Value::Set(s) => format!("{} 条 / {}", s.count, s.elem_type),
        Value::Struct(s) => format!("{} 个字段", s.props.len()),
        Value::Binary(b) => {
            let head: String = b.bytes.iter().take(8).map(|x| format!("{x:02x}")).collect();
            format!("{} {head}…", b.ty)
        }
        Value::Skipped(s) => format!("跳过 {}（{} 字节）", s.ty, s.size),
    }
}

fn walk(props: &[crate::save::Property], prefix: &str, known: bool, rows: &mut Vec<FieldRow>, hidden: &mut usize) {
    use crate::save::Value;
    for p in props {
        let path = if prefix.is_empty() { p.name.clone() } else { format!("{prefix}.{}", p.name) };
        let root_known = known || KNOWN_ROOTS.contains(&p.name.as_str());
        rows.push(FieldRow {
            path: path.clone(),
            ty: p.tag.ty.clone(),
            value: preview(&p.value),
            known: root_known,
        });
        match &p.value {
            Value::Struct(s) => walk(&s.props, &path, root_known, rows, hidden),
            Value::Array(a) => {
                if let Some(first) = a.items.first() {
                    if let Value::Struct(s) = first {
                        walk(&s.props, &format!("{path}[0]"), root_known, rows, hidden);
                    }
                }
                *hidden += (a.count.max(0) as usize).saturating_sub(1);
            }
            Value::Map(m) => {
                if let Some((k, v)) = m.entries.first() {
                    let key_text = preview(k);
                    let kp = format!("{path}[\"{key_text}\"]");
                    rows.push(FieldRow { path: kp.clone(), ty: m.value_type.clone(), value: preview(v), known: root_known });
                    if let Value::Struct(s) = v {
                        walk(&s.props, &kp, root_known, rows, hidden);
                    }
                }
                *hidden += (m.count.max(0) as usize).saturating_sub(1);
            }
            Value::Set(s) => {
                *hidden += (s.count.max(0) as usize).saturating_sub(1);
            }
            _ => {}
        }
    }
}

/// 开发者视图：默认关着，设置页里那个开关打开才会被调用。
#[tauri::command]
pub fn save_debug(selected: Option<String>) -> ApiResult<SaveDebug> {
    let empty = |state: &'static str, note: &str| SaveDebug {
        state,
        note: note.to_owned(),
        file_name: None,
        sha256: String::new(),
        file_bytes: 0,
        nodes: 0,
        consumed: 0,
        skipped: 0,
        notes: Vec::new(),
        rows: Vec::new(),
        rows_hidden: 0,
    };

    let Some(file) = pick(selected.as_deref()) else {
        return ApiResult::ok(empty("not_found", "没找到存档，开发者视图没有东西可看"));
    };
    let name = file.path.file_name().map(|s| s.to_string_lossy().to_string());

    let bytes = match std::fs::read(&file.path) {
        Ok(b) => b,
        Err(e) => return ApiResult::ok(empty("bad_structure", &format!("读不动这个文件：{e}"))),
    };
    let sha256 = match store::sha256_file(&file.path) {
        Ok(s) => s,
        Err(e) => return ApiResult::ok(empty("bad_structure", &format!("算不出哈希：{e}"))),
    };

    match crate::save::parse(&bytes) {
        Ok(out) => {
            let mut rows = Vec::new();
            let mut hidden = 0;
            walk(&out.props, "", false, &mut rows, &mut hidden);
            ApiResult::ok(SaveDebug {
                state: "ok",
                note: String::new(),
                file_name: name,
                sha256,
                file_bytes: bytes.len(),
                nodes: out.stats.nodes,
                consumed: out.consumed,
                skipped: out.stats.skipped,
                notes: out.notes.iter().map(|n| format!("{} · {}", n.path, n.detail)).collect(),
                rows,
                rows_hidden: hidden,
            })
        }
        Err(e) => {
            let mut out = empty("bad_structure", e.kind.human());
            out.file_name = name;
            out.sha256 = sha256;
            out.file_bytes = bytes.len();
            ApiResult::ok(out)
        }
    }
}
