//! 「装备」页的数据：每个角色槽三套配装，每套八个固定格子。
//!
//! 只读：存档怎么读、怎么缓存都走 `save::store`，这里只把读出来的 GUID 换成名字。
//! 认不出名字的格子显示 `#编号前 8 位`，不猜。

use serde::Serialize;

use super::ApiResult;
use crate::paths;
use crate::save::store::{self, LoadClass};

use super::dwarf::{class_from_guids, lookup_weapon};
use super::save::pick;

/// 格子的固定顺序：主武器、副武器、移动、职业工具、护甲、照明弹、镐、手雷。
const SLOTS: [(&str, &str); 8] = [
    ("PrimaryWeapon", "primary"),
    ("SecondaryWeapon", "secondary"),
    ("TraversalTool", "traversal"),
    ("ClassTool", "classTool"),
    ("Armor", "armor"),
    ("Flare", "flare"),
    ("MiningTool", "pickaxe"),
    ("Grenade", "grenade"),
];

#[derive(Serialize)]
pub struct EquipmentItem {
    /// 配件在存档里的字段名，前端据此排位置
    pub key: String,
    /// 存档里的 GUID；空格子是空串
    pub id: String,
    /// 查不到名字时是 `#编号前 8 位`；空格子是空串
    pub name: String,
    pub name_en: Option<String>,
    pub known: bool,
    /// 这件东西属于哪个职业（查不到就是 `null`）
    pub class: Option<String>,
    /// 表里登记的槽位，用来核对「这把枪该放哪一格」
    pub slot_kind: Option<String>,
}

#[derive(Serialize)]
pub struct LoadoutView {
    /// 第几套配装，从 1 起
    pub index: i64,
    pub items: Vec<EquipmentItem>,
}

#[derive(Serialize)]
pub struct CharacterEquip {
    pub slot: i64,
    /// 反推出来的职业；认不出就是 `null`
    pub class: Option<String>,
    pub basis: Option<String>,
    pub loadouts: Vec<LoadoutView>,
}

#[derive(Serialize)]
pub struct EquipmentView {
    pub state: String,
    pub note: String,
    pub file_name: Option<String>,
    pub from_cache: bool,
    pub characters: Vec<CharacterEquip>,
}

impl EquipmentView {
    fn with_state(state: &str, note: &str) -> Self {
        EquipmentView {
            state: state.to_string(),
            note: note.to_string(),
            file_name: None,
            from_cache: false,
            characters: Vec::new(),
        }
    }
}

/// 存档里没装东西的格子写的是全零 GUID（不是空串），这里一律当「没装」。
fn is_blank(id: &str) -> bool {
    id.is_empty() || id.chars().all(|c| c == '0' || c == '-')
}

fn item_of(key: &str, kind: &str, id: &str) -> EquipmentItem {
    if is_blank(id) {
        return EquipmentItem {
            key: key.to_string(),
            id: String::new(),
            name: String::new(),
            name_en: None,
            known: true,
            class: None,
            slot_kind: Some(kind.to_string()),
        };
    }
    let short = id.get(..8).unwrap_or(id);
    match lookup_weapon(id) {
        Some((class, slot_kind, zh, en)) => EquipmentItem {
            key: key.to_string(),
            id: id.to_string(),
            name: if zh.is_empty() { en.clone() } else { zh },
            name_en: Some(en),
            known: true,
            class,
            slot_kind: slot_kind.or_else(|| Some(kind.to_string())),
        },
        None => EquipmentItem {
            key: key.to_string(),
            id: id.to_string(),
            name: format!("#{short}"),
            name_en: None,
            known: false,
            class: None,
            slot_kind: Some(kind.to_string()),
        },
    }
}

/// 装备页：五个角色槽 × 三套配装 × 八个格子。
#[tauri::command]
pub fn equipment_view(selected: Option<String>) -> ApiResult<EquipmentView> {
    let Some(file) = pick(selected.as_deref()) else {
        return ApiResult::ok(EquipmentView::with_state(
            "not_found",
            "没找到存档：这台机器上还没存过档，或者装的是别的版本（目前只认 Steam 版）",
        ));
    };

    let cache_root = paths::data_root().join("cache");
    match store::load(&file.path, &cache_root) {
        Ok((doc, report)) => {
            let mut characters = Vec::new();
            for (i, c) in doc.data.CharacterSaves.iter().enumerate() {
                // 判据只看主武器与副武器；其余格子不参与「这个槽是哪个职业」
                let mut guids: Vec<String> = Vec::new();
                for loadout in c.Loadouts.iter().take(3) {
                    if !loadout.PrimaryWeapon.is_empty() {
                        guids.push(loadout.PrimaryWeapon.clone());
                    }
                    if !loadout.SecondaryWeapon.is_empty() {
                        guids.push(loadout.SecondaryWeapon.clone());
                    }
                }
                let (class, basis) =
                    class_from_guids(&guids, super::dwarf::weapon_table());

                let loadouts = c
                    .Loadouts
                    .iter()
                    .enumerate()
                    .map(|(n, loadout)| LoadoutView {
                        index: n as i64 + 1,
                        items: SLOTS
                            .iter()
                            .map(|(key, kind)| {
                                let id = match *key {
                                    "PrimaryWeapon" => loadout.PrimaryWeapon.as_str(),
                                    "SecondaryWeapon" => loadout.SecondaryWeapon.as_str(),
                                    "TraversalTool" => loadout.TraversalTool.as_str(),
                                    "ClassTool" => loadout.ClassTool.as_str(),
                                    "Armor" => loadout.Armor.as_str(),
                                    "Flare" => loadout.Flare.as_str(),
                                    "MiningTool" => loadout.MiningTool.as_str(),
                                    _ => loadout.Grenade.as_str(),
                                };
                                item_of(key, kind, id)
                            })
                            .collect(),
                    })
                    .collect();

                characters.push(CharacterEquip {
                    slot: i as i64 + 1,
                    class,
                    basis,
                    loadouts,
                });
            }

            ApiResult::ok(EquipmentView {
                state: "ok".to_string(),
                note: String::new(),
                file_name: file
                    .path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string()),
                from_cache: report.from_cache,
                characters,
            })
        }
        Err(e) => {
            let state = match e.class {
                LoadClass::UnknownVersion => "unknown_version",
                LoadClass::NotSave | LoadClass::BadStructure => "bad_structure",
            };
            let mut out = EquipmentView::with_state(state, e.class.human());
            out.file_name = file
                .path
                .file_name()
                .map(|s| s.to_string_lossy().to_string());
            ApiResult::ok(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_guid_is_an_empty_slot_not_a_number() {
        let item = item_of("Grenade", "grenade", "");
        assert!(item.id.is_empty());
        assert!(item.name.is_empty());
        assert!(item.known);
        assert_eq!(item.key, "Grenade");
    }

    #[test]
    fn unknown_guid_shows_short_number_and_says_unknown() {
        let item = item_of("Armor", "armor", "deadbeefdeadbeefdeadbeefdeadbeef");
        assert_eq!(item.name, "#deadbeef");
        assert!(!item.known);
    }

    #[test]
    fn eight_slots_are_in_the_documented_order() {
        let keys: Vec<&str> = SLOTS.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            keys,
            vec![
                "PrimaryWeapon",
                "SecondaryWeapon",
                "TraversalTool",
                "ClassTool",
                "Armor",
                "Flare",
                "MiningTool",
                "Grenade"
            ]
        );
    }
}
