//! 「装备」页的数据：每个角色槽三套配装，每套八个固定格子。
//!
//! 只读：存档怎么读、怎么缓存都走 `save::store`，这里只把读出来的 GUID 换成名字。
//! 认不出名字的格子显示 `#编号前 8 位`，不猜。

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::Serialize;

use super::ApiResult;
use crate::paths;
use crate::save::store::{self, LoadClass};

use super::dwarf::{class_from_guids, lookup_weapon};
use super::save::pick;

/// 改装表：编号 → 中英文名 → 属于哪把武器。全部来自游戏 pak 与本地化表，见 `_tools/_upgnames.mjs`。
const UPGRADES_JSON: &str = include_str!("../../../src/data/upgrades.json");

#[derive(serde::Deserialize)]
struct UpgradeFile {
    upgrades: Vec<UpgradeRow>,
}

#[derive(serde::Deserialize)]
struct UpgradeRow {
    /// 存档里出现的那 32 位十六进制
    id: String,
    /// 英文名（资产自带）
    en: Option<String>,
    /// 简体中文名（游戏本地化表）
    zh: Option<String>,
    /// 属于哪把武器的 id（抽不到就是 `null`）
    weapon: Option<String>,
}

fn upgrade_table() -> &'static HashMap<String, UpgradeRow> {
    static TABLE: OnceLock<HashMap<String, UpgradeRow>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let parsed: UpgradeFile = serde_json::from_str(UPGRADES_JSON).unwrap_or(UpgradeFile {
            upgrades: Vec::new(),
        });
        parsed
            .upgrades
            .into_iter()
            .map(|row| (row.id.to_ascii_lowercase(), row))
            .collect()
    })
}

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
    /// 这把武器/装备的改装。没有改装表就留空（这时前端什么都不显示）
    pub upgrades: Vec<UpgradeItem>,
}

/// 一件改装：名字、买没买、有没有正装着。
#[derive(Serialize)]
pub struct UpgradeItem {
    pub id: String,
    pub name: String,
    pub name_en: Option<String>,
    pub owned: bool,
    pub equipped: bool,
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
    /// 改装那块的一句脚注（整份存档一份清单，不分武器）
    pub upgrade_note: String,
}

impl EquipmentView {
    fn with_state(state: &str, note: &str) -> Self {
        EquipmentView {
            state: state.to_string(),
            note: note.to_string(),
            file_name: None,
            from_cache: false,
            characters: Vec::new(),
            upgrade_note: String::new(),
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
            upgrades: Vec::new(),
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
            upgrades: Vec::new(),
        },
        None => EquipmentItem {
            key: key.to_string(),
            id: id.to_string(),
            name: format!("#{short}"),
            name_en: None,
            known: false,
            class: None,
            slot_kind: Some(kind.to_string()),
            upgrades: Vec::new(),
        },
    }
}

/// 某把武器的改装清单：表里属于它、且编号非空的那些；带上「买没买、装没装」。
///
/// 表里没有的（我们没能从资产里抽到归属的武器），退一步：把这把枪现在装着的改装也列出来，
/// 名字查不到就显示 `#编号前 8 位`，让「正装着什么」至少是看得见的。
fn upgrades_of(
    weapon: &str,
    owned: &HashSet<String>,
    equipped: &HashSet<String>,
) -> Vec<UpgradeItem> {
    let key = weapon.to_ascii_lowercase();
    let mut out: Vec<UpgradeItem> = upgrade_table()
        .iter()
        .filter(|(id, row)| {
            !id.is_empty()
                && row
                    .weapon
                    .as_deref()
                    .map(|w| w.eq_ignore_ascii_case(&key))
                    .unwrap_or(false)
        })
        .map(|(id, row)| UpgradeItem {
            id: id.clone(),
            name: row
                .zh
                .clone()
                .filter(|s| !s.is_empty())
                .or_else(|| row.en.clone())
                .unwrap_or_else(|| format!("#{}", id.get(..8).unwrap_or(id))),
            name_en: row.en.clone(),
            owned: owned.contains(id),
            equipped: equipped.contains(id),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}
/// 把这把枪「正装着」的改装名字查出来（不进上面的表，只用于没有改装表时的兜底）。
fn equipped_of(ids: &[String]) -> Vec<UpgradeItem> {
    let table = upgrade_table();
    let mut out: Vec<UpgradeItem> = ids
        .iter()
        .filter(|id| !is_blank(id))
        .map(|id| {
            let key = id.to_ascii_lowercase();
            let row = table.get(&key);
            UpgradeItem {
                id: key.clone(),
                name: row
                    .and_then(|r| r.zh.clone().filter(|s| !s.is_empty()).or_else(|| r.en.clone()))
                    .unwrap_or_else(|| format!("#{}", key.get(..8).unwrap_or(&key))),
                name_en: row.and_then(|r| r.en.clone()),
                owned: true,
                equipped: true,
            }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
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
            // 「已买」是整份存档一份清单；「正装着」要看每套装了哪些
            let mut owned: HashSet<String> = HashSet::new();
            for id in &doc.data.Weapons.PurchasedItemUpgrades {
                if !is_blank(id) {
                    owned.insert(id.to_ascii_lowercase());
                }
            }
            let mut equipped: HashSet<String> = HashSet::new();
            // 每把武器「现在装着哪几条」，用来在改装表缺项时兜底
            let mut equipped_of_weapon: HashMap<String, Vec<String>> = HashMap::new();
            for loadout in &doc.data.Weapons.ItemUpgradeLoadouts {
                for entry in &loadout.Loadout {
                    let weapon = entry.WeaponID.to_ascii_lowercase();
                    // 永久改装（每把枪自己的那几条）也算已解锁
                    for id in &entry.PermanentUpgrades {
                        if !is_blank(id) {
                            owned.insert(id.to_ascii_lowercase());
                        }
                    }
                    for id in &entry.EquippedUpgrades {
                        if !is_blank(id) {
                            let key = id.to_ascii_lowercase();
                            equipped.insert(key.clone());
                            owned.insert(key.clone());
                            equipped_of_weapon.entry(weapon.clone()).or_default().push(key);
                        }
                    }
                    if !is_blank(&entry.EquippedOverclock) {
                        equipped.insert(entry.EquippedOverclock.to_ascii_lowercase());
                    }
                }
            }

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
                                let mut item = item_of(key, kind, id);
                                if !item.id.is_empty() {
                                    item.upgrades = upgrades_of(&item.id, &owned, &equipped);
                                    if item.upgrades.is_empty() {
                                        // 表里没有这把武器的归属：至少把「正装着的」列出来
                                        let ids = equipped_of_weapon
                                            .get(&item.id.to_ascii_lowercase())
                                            .cloned()
                                            .unwrap_or_default();
                                        item.upgrades = equipped_of(&ids);
                                    }
                                }
                                item
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
                upgrade_note: "改装解不解锁是整份存档一份清单、不分武器——这里只回答这一项解锁没有。".to_string(),
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
    fn upgrades_are_listed_for_the_weapon_they_belong_to() {
        let mut owned = HashSet::new();
        let mut equipped = HashSet::new();
        // 找一把表里真有改装的武器
        let weapon = upgrade_table()
            .values()
            .find_map(|row| row.weapon.clone())
            .expect("表里至少要有一把武器有改装");
        let list = upgrades_of(&weapon, &owned, &equipped);
        assert!(!list.is_empty(), "这把武器应该有改装");
        assert!(list.iter().all(|u| !u.owned && !u.equipped));

        // 把第一条标成「买了、装着」
        let first = list[0].id.clone();
        owned.insert(first.clone());
        equipped.insert(first.clone());
        let again = upgrades_of(&weapon, &owned, &equipped);
        let hit = again.iter().find(|u| u.id == first).expect("同一条还在");
        assert!(hit.owned && hit.equipped);
    }

    #[test]
    fn weapon_without_table_falls_back_to_what_is_equipped() {
        let owned = HashSet::new();
        let equipped = HashSet::new();
        // 一个表里肯定没有的武器 id
        let list = upgrades_of("ffffffffffffffffffffffffffffffff", &owned, &equipped);
        assert!(list.is_empty());

        let ids = vec!["deadbeefdeadbeefdeadbeefdeadbeef".to_string()];
        let fallback = equipped_of(&ids);
        assert_eq!(fallback.len(), 1);
        assert!(fallback[0].owned && fallback[0].equipped);
        assert_eq!(fallback[0].name, "#deadbeef");
    }

    #[test]
    fn shipped_upgrade_table_is_parsed_and_keyed_lowercase() {
        let table = upgrade_table();
        assert!(table.len() > 500, "改装表应该有小几百条，实际 {}", table.len());
        assert!(
            table.keys().all(|k| k.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())),
            "键必须是 32 位小写十六进制"
        );
    }
}

#[cfg(test)]
mod tests_slots {
    use super::*;

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
