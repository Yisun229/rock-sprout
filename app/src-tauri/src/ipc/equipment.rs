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

/// 超频表：编号 → 名字 → 属于哪把武器 → 稀有度。来自游戏 pak，见 `_tools/_v023build.mjs`。
const OVERCLOCKS_JSON: &str = include_str!("../../../src/data/overclocks.json");

#[derive(serde::Deserialize)]
struct OverclockFile {
    overclocks: Vec<OverclockRow>,
}

#[derive(serde::Deserialize, Clone)]
struct OverclockRow {
    id: String,
    en: Option<String>,
    zh: Option<String>,
    /// 武器文件夹名（如 `FlameThrower`），与 weapon-names 表里的 `en` 对得上
    weapon: Option<String>,
    /// 无瑕 / 均衡 / 不稳定
    rarity: Option<String>,
    /// `overclock` 是玩家面的超频；`modifier` 是超频内部的加成/惩罚子项，不渲染
    kind: String,
}

fn overclock_table() -> &'static Vec<OverclockRow> {
    static TABLE: OnceLock<Vec<OverclockRow>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let parsed: OverclockFile = serde_json::from_str(OVERCLOCKS_JSON).unwrap_or(OverclockFile {
            overclocks: Vec::new(),
        });
        parsed
            .overclocks
            .into_iter()
            .filter(|row| row.kind == "overclock")
            .collect()
    })
}

/// 这把武器一共有几条超频（只给数字，不列名单）。
fn overclock_total(weapon_folder: Option<&str>) -> usize {
    let Some(folder) = weapon_folder else {
        return 0;
    };
    overclock_table()
        .iter()
        .filter(|row| row.weapon.as_deref() == Some(folder))
        .count()
}

/// 武器资产所在的文件夹名——超频表 `weapon` 字段用的就是这套命名（不是显示名）。
fn weapon_folder(guid: &str) -> Option<String> {
    lookup_weapon(guid).and_then(|(_, _, _, _, folder)| folder)
}

fn overclock_by_id(id: &str) -> Option<&'static OverclockRow> {
    overclock_table()
        .iter()
        .find(|row| row.id.eq_ignore_ascii_case(id))
}

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
    /// 正装着的那条超频（没装就是 `null`）
    pub overclock: Option<OverclockItem>,
    /// 这只武器在存档里开没开超频（`OverclockingUnlocked`）
    pub oc_open: bool,
    /// 这把武器一共有几条超频；只给数字，不列名单
    pub oc_total: usize,
}

/// 一条超频：名字 + 稀有度。稀有度只显示文字，不加颜色。
#[derive(Serialize)]
pub struct OverclockItem {
    pub id: String,
    pub name: String,
    pub name_en: Option<String>,
    pub rarity: Option<String>,
}

/// 一件改装：名字、买没买、有没有正装着。
#[derive(Serialize)]
pub struct UpgradeItem {
    pub id: String,
    pub name: String,
    pub name_en: Option<String>,
    pub owned: bool,
    pub equipped: bool,
    /// 这条在改装表里有没有。`false` 表示「正装着，但我们没从资产里抽到它」——
    /// 界面上要明写一句，不能让它悄悄消失。
    pub in_table: bool,
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
    /// 皮肤：654 条，owned 表示这份存档里有
    pub skins: Vec<OwnedNameRow>,
    /// 十字镐部件：160 条
    pub pickaxe: Vec<OwnedNameRow>,
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
            skins: Vec::new(),
            pickaxe: Vec::new(),
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
            overclock: None,
            oc_open: false,
            oc_total: 0,
        };
    }
    let short = id.get(..8).unwrap_or(id);
    match lookup_weapon(id) {
        Some((class, slot_kind, zh, en, _folder)) => EquipmentItem {
            key: key.to_string(),
            id: id.to_string(),
            name: if zh.is_empty() { en.clone() } else { zh },
            name_en: Some(en),
            known: true,
            class,
            slot_kind: slot_kind.or_else(|| Some(kind.to_string())),
            upgrades: Vec::new(),
            overclock: None,
            oc_open: false,
            oc_total: 0,
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
            overclock: None,
            oc_open: false,
            oc_total: 0,
        },
    }
}

/// 某把武器的改装清单：表里属于它的那些，**并上**它现在装着的、
/// 但表里没有的那些（标 `in_table: false`）。两段合起来，缺口是看得见的标注，不是悄悄少几条。
fn upgrades_of(
    weapon: &str,
    equipped_here: &[String],
    owned: &HashSet<String>,
    equipped: &HashSet<String>,
) -> Vec<UpgradeItem> {
    let key = weapon.to_ascii_lowercase();
    let table = upgrade_table();
    let mut out: Vec<UpgradeItem> = table
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
            in_table: true,
        })
        .collect();

    // 表里漏掉的：正装着、但上面没列出来的
    let listed: HashSet<String> = out.iter().map(|u| u.id.clone()).collect();
    for id in equipped_here {
        if is_blank(id) {
            continue;
        }
        let lower = id.to_ascii_lowercase();
        if listed.contains(&lower) {
            continue;
        }
        let row = table.get(&lower);
        out.push(UpgradeItem {
            id: lower.clone(),
            name: row
                .and_then(|r| r.zh.clone().filter(|s| !s.is_empty()).or_else(|| r.en.clone()))
                .unwrap_or_else(|| format!("#{}", lower.get(..8).unwrap_or(&lower))),
            name_en: row.and_then(|r| r.en.clone()),
            owned: true,
            equipped: true,
            in_table: false,
        });
    }

    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 皮肤 / 十字镐：存档里有 GUID 清单，所以能标「有没有」
#[derive(Debug, Clone, serde::Serialize)]
pub struct OwnedNameRow {
    pub id: String,
    pub name: String,
    /// 名字来源：table（表里一对）/ asset（资产英文）/ derived（去前缀兜底）
    pub source: String,
    /// 这一条是什么：十字镐是部件类型（镐刃/镐柄/镐尾），皮肤是属于哪把武器
    pub hint: String,
    pub owned: bool,
}

const SKIN_NAMES_JSON: &str = include_str!("../../../src/data/skin-names.json");
const PICKAXE_NAMES_JSON: &str = include_str!("../../../src/data/pickaxe-parts.json");

fn owned_rows(
    json: &str,
    key: &str,
    owned: &HashSet<String>,
    hints: &HashMap<String, String>,
) -> Vec<OwnedNameRow> {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    let mut out = Vec::new();
    for row in v.get(key).and_then(|x| x.as_array()).cloned().unwrap_or_default() {
        let id = row
            .get("id")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if id.is_empty() {
            continue;
        }
        let name = row
            .get("zh")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| row.get("en").and_then(|x| x.as_str()).filter(|s| !s.is_empty()))
            .unwrap_or("")
            .to_string();
        let source = row
            .get("source")
            .and_then(|x| x.as_str())
            .unwrap_or("none")
            .to_string();
        // 十字镐：表里带 part（Blade*/Handle*/Grip*），按前缀翻成中文；皮肤：靠调用方给的「属于哪把武器」
        let hint = match row.get("part").and_then(|x| x.as_str()) {
            Some(p) if p.starts_with("Blade") => "镐刃".to_string(),
            Some(p) if p.starts_with("Handle") => "镐柄".to_string(),
            Some(p) if p.starts_with("Grip") || p.starts_with("Shaft") => "镐尾".to_string(),
            Some(other) => other.to_string(),
            None => hints.get(&id).cloned().unwrap_or_default(),
        };
        out.push(OwnedNameRow {
            owned: owned.contains(&id),
            id,
            name,
            source,
            hint,
        });
    }
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
            // 每把武器「正装着哪条超频」与「开没开超频」，都按武器 GUID 索引
            let mut oc_equipped: HashMap<String, String> = HashMap::new();
            let mut oc_open: HashMap<String, bool> = HashMap::new();
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
                        let oc = entry.EquippedOverclock.to_ascii_lowercase();
                        oc_equipped.insert(weapon.clone(), oc.clone());
                        equipped.insert(oc);
                    }
                    oc_open.insert(weapon.clone(), entry.OverclockingUnlocked);
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
                                    let here = equipped_of_weapon
                                        .get(&item.id.to_ascii_lowercase())
                                        .cloned()
                                        .unwrap_or_default();
                                    item.upgrades = upgrades_of(&item.id, &here, &owned, &equipped);
                                    let wid = item.id.to_ascii_lowercase();
                                    item.oc_total = overclock_total(weapon_folder(&item.id).as_deref());
                                    item.oc_open = *oc_open.get(&wid).unwrap_or(&false);
                                    if let Some(oc) = oc_equipped.get(&wid) {
                                        if let Some(row) = overclock_by_id(oc) {
                                            let fallback = format!("#{}", oc.get(..8).unwrap_or(oc));
                                            item.overclock = Some(OverclockItem {
                                                id: oc.clone(),
                                                name: row
                                                    .zh
                                                    .clone()
                                                    .filter(|s| !s.is_empty())
                                                    .or_else(|| row.en.clone())
                                                    .unwrap_or(fallback),
                                                name_en: row.en.clone(),
                                                rarity: row.rarity.clone(),
                                            });
                                        }
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

            // 皮肤与十字镐：存档里有 GUID 清单，能判「有没有」
            // 皮肤：`UnlockedItemSkins` 是「物品 GUID → 已解锁皮肤集合」，要并起来
            let mut owned_skins: HashSet<String> = HashSet::new();
            // 皮肤 → 属于哪把武器（键是武器 GUID，翻成名字当提示）
            let mut skin_hints: HashMap<String, String> = HashMap::new();
            for (weapon, skins) in &doc.data.Vanity.UnlockedItemSkins {
                let label = super::dwarf::lookup_weapon(weapon)
                    .map(|(_, _, zh, en, _)| {
                        if zh.is_empty() { en } else { zh }
                    })
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| format!("#{}", weapon.get(..8).unwrap_or(weapon)));
                for id in skins {
                    if !is_blank(id) {
                        let key = id.to_ascii_lowercase();
                        owned_skins.insert(key.clone());
                        skin_hints.entry(key).or_insert_with(|| label.clone());
                    }
                }
            }
            let mut owned_parts: HashSet<String> = HashSet::new();
            for id in &doc.data.Vanity.UnlockedPickaxeParts {
                if !is_blank(id) {
                    owned_parts.insert(id.to_ascii_lowercase());
                }
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
                skins: owned_rows(SKIN_NAMES_JSON, "skins", &owned_skins, &skin_hints),
                pickaxe: owned_rows(PICKAXE_NAMES_JSON, "parts", &owned_parts, &HashMap::new()),
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
        let list = upgrades_of(&weapon, &[], &owned, &equipped);
        assert!(!list.is_empty(), "这把武器应该有改装");
        assert!(list.iter().all(|u| !u.owned && !u.equipped));

        // 把第一条标成「买了、装着」
        let first = list[0].id.clone();
        owned.insert(first.clone());
        equipped.insert(first.clone());
        let again = upgrades_of(&weapon, &[], &owned, &equipped);
        let hit = again.iter().find(|u| u.id == first).expect("同一条还在");
        assert!(hit.owned && hit.equipped);
    }

    #[test]
    fn equipped_but_missing_from_table_is_appended_and_marked() {
        let owned = HashSet::new();
        let equipped = HashSet::new();
        // 一把表里没有的武器 + 它正装着的一条
        let here = vec!["deadbeefdeadbeefdeadbeefdeadbeef".to_string()];
        let list = upgrades_of("ffffffffffffffffffffffffffffffff", &here, &owned, &equipped);
        assert_eq!(list.len(), 1);
        assert!(list[0].equipped && list[0].owned);
        assert!(!list[0].in_table, "这条要标成「表里没有」");
        assert_eq!(list[0].name, "#deadbeef");
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
