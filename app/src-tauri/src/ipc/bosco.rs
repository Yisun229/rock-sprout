//! 我的机器人（博斯科）：13 条改装的中英名与三态、涂装 45 条的解锁口径。
//!
//! 两个口径是查存档查出来的，别再想当然：
//!
//! - 涂装总数是 **45**（`OwningCharacter == BoscoID` 的皮肤资产），不是 `Drone/Skins` 目录下的 39 条：
//!   另有 6 条在 `_GlobalSkins` 里，同样属于博斯科。
//! - 「解锁」只认 `Vanity.UnlockedItemSkins[Bosco]`；`WeaponEntry.EquippedSkins` 是**装着**的记录，
//!   不是已解锁集合（13 件装备里 6 件的这两个集合对不上，拿它当解锁集算会偏）。

use std::collections::HashSet;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::ApiResult;

/// 博斯科在 `ItemUpgradeLoadouts` 里的键（`WeaponID`）
const BOSCO_ID: &str = "d3af4b9e940fef41a7db46914463f084";
const UPGRADES_JSON: &str = include_str!("../../../src/data/bosco-upgrades.json");
const SKINS_JSON: &str = include_str!("../../../src/data/skin-names.json");

#[derive(Debug, Deserialize)]
struct UpgradeFile {
    upgrades: Vec<UpgradeRow>,
}

#[derive(Debug, Deserialize)]
struct UpgradeRow {
    id: String,
    #[allow(dead_code)]
    asset: String,
    en: String,
    #[serde(default)]
    zh: Option<String>,
    #[serde(default)]
    zh_suspect: bool,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SkinFile {
    skins: Vec<SkinRow>,
}

#[derive(Debug, Deserialize)]
struct SkinRow {
    id: String,
    #[serde(default)]
    en: String,
    #[serde(default)]
    zh: String,
    owner: String,
}

fn upgrades() -> Option<&'static UpgradeFile> {
    static TABLE: OnceLock<Option<UpgradeFile>> = OnceLock::new();
    TABLE
        .get_or_init(|| serde_json::from_str::<UpgradeFile>(UPGRADES_JSON).ok())
        .as_ref()
}

fn skins() -> Option<&'static SkinFile> {
    static TABLE: OnceLock<Option<SkinFile>> = OnceLock::new();
    TABLE
        .get_or_init(|| serde_json::from_str::<SkinFile>(SKINS_JSON).ok())
        .as_ref()
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NamePair {
    pub zh: String,
    pub en: String,
}

/// 一条改装。`unused` 的两条不进 `upgrades`，分母因此是 11。
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeView {
    pub id: String,
    pub name: String,
    pub en: String,
    /// 买过（`PurchasedItemUpgrades`）
    pub owned: bool,
    /// 当前装着（`EquippedUpgrades`）
    pub equipped: bool,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkinView {
    pub id: String,
    pub name: NamePair,
    pub owned: bool,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CurrentSkin {
    pub id: String,
    pub name: Option<NamePair>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoscoView {
    pub state: String,
    pub note: Option<String>,
    pub upgrades: Vec<UpgradeView>,
    pub upgrades_owned: u32,
    /// 分母：13 条里去掉 `status = "unused"` 的 2 条 = 11
    pub upgrades_total: u32,
    pub skins: Vec<SkinView>,
    pub skins_owned: u32,
    pub skins_total: u32,
    /// 存档里 `EquippedSkins` 记的那几条（名字照抄，怎么组合成外观未证实）
    pub current_skin: Vec<CurrentSkin>,
}

impl BoscoView {
    fn empty(state: &str, note: Option<String>) -> Self {
        Self {
            state: state.to_string(),
            note,
            upgrades: Vec::new(),
            upgrades_owned: 0,
            upgrades_total: 0,
            skins: Vec::new(),
            skins_owned: 0,
            skins_total: 0,
            current_skin: Vec::new(),
        }
    }
}

fn name_of(row: &UpgradeRow) -> String {
    let zh = row.zh.as_deref().unwrap_or("").trim();
    if zh.is_empty() {
        row.en.clone()
    } else {
        zh.to_string()
    }
}

/// 13 条里真正会出现的（`unused` 的两条只留在表里，界面不显示）。
pub(crate) fn rows_of(
    purchased: &HashSet<String>,
    equipped: &HashSet<String>,
) -> Vec<UpgradeView> {
    let Some(table) = upgrades() else {
        return Vec::new();
    };
    table
        .upgrades
        .iter()
        .filter(|r| r.status.as_deref() != Some("unused"))
        .map(|r| UpgradeView {
            id: r.id.clone(),
            name: name_of(r),
            en: r.en.clone(),
            owned: purchased.contains(&r.id),
            equipped: equipped.contains(&r.id),
        })
        .collect()
}

/// 博斯科皮肤的 45 条与其中已解锁的几条。
pub(crate) fn skins_of(owned: &HashSet<String>) -> (Vec<SkinView>, u32, u32) {
    let Some(table) = skins() else {
        return (Vec::new(), 0, 0);
    };
    let rows: Vec<SkinView> = table
        .skins
        .iter()
        .filter(|s| s.owner == "Bosco")
        .map(|s| SkinView {
            id: s.id.clone(),
            name: NamePair {
                zh: if s.zh.trim().is_empty() {
                    s.en.clone()
                } else {
                    s.zh.clone()
                },
                en: s.en.clone(),
            },
            owned: owned.contains(&s.id),
        })
        .collect();
    let have = rows.iter().filter(|r| r.owned).count() as u32;
    let total = rows.len() as u32;
    (rows, have, total)
}

fn current_of(equipped_skins: &[String]) -> Vec<CurrentSkin> {
    let Some(table) = skins() else {
        return Vec::new();
    };
    equipped_skins
        .iter()
        .map(|id| {
            let row = table.skins.iter().find(|s| &s.id == id);
            CurrentSkin {
                id: id.clone(),
                name: row.map(|s| NamePair {
                    zh: if s.zh.trim().is_empty() {
                        s.en.clone()
                    } else {
                        s.zh.clone()
                    },
                    en: s.en.clone(),
                }),
            }
        })
        .collect()
}

fn view_of(data: &crate::save::model::SaveData) -> BoscoView {
    if upgrades().is_none() || skins().is_none() {
        return BoscoView::empty(
            "bad_structure",
            Some("博斯科的表读不出来：app/src/data/bosco-upgrades.json 或 skin-names.json 可能坏了".into()),
        );
    }

    let entry = data
        .Weapons
        .ItemUpgradeLoadouts
        .iter()
        .flat_map(|l| l.Loadout.iter())
        .find(|e| e.WeaponID == BOSCO_ID);

    let purchased: HashSet<String> = data.Weapons.PurchasedItemUpgrades.iter().cloned().collect();
    let equipped: HashSet<String> = entry
        .map(|e| e.EquippedUpgrades.iter().cloned().collect())
        .unwrap_or_default();
    let upgrades_rows = rows_of(&purchased, &equipped);
    let owned_upgrades = upgrades_rows.iter().filter(|r| r.owned).count() as u32;
    let total_upgrades = upgrades_rows.len() as u32;

    let unlocked: HashSet<String> = data
        .Vanity
        .UnlockedItemSkins
        .iter()
        .find(|(k, _)| k == BOSCO_ID)
        .map(|(_, v)| v.iter().cloned().collect())
        .unwrap_or_default();
    let (skin_rows, skins_owned, skins_total) = skins_of(&unlocked);

    let equipped_skins: Vec<String> = entry
        .map(|e| e.EquippedSkins.iter().cloned().collect())
        .unwrap_or_default();

    BoscoView {
        state: "ok".into(),
        note: entry.is_none().then(|| "这份存档里没有博斯科的改装记录".to_string()),
        upgrades: upgrades_rows,
        upgrades_owned: owned_upgrades,
        upgrades_total: total_upgrades,
        skins: skin_rows,
        skins_owned,
        skins_total,
        current_skin: current_of(&equipped_skins),
    }
}

/// 读博斯科这一页需要的全部数据。只读，不碰存档与游戏目录。
#[tauri::command]
pub fn bosco_view(selected: Option<String>) -> ApiResult<BoscoView> {
    let Some(file) = super::save::pick(selected.as_deref()) else {
        return ApiResult::ok(BoscoView::empty(
            "not_found",
            Some("没找到存档：这台机器上还没存过档，或者装的是别的版本（目前只认 Steam 版）".into()),
        ));
    };
    let cache_root = crate::paths::data_root().join("cache");
    match crate::save::store::load(&file.path, &cache_root) {
        Ok((doc, _)) => ApiResult::ok(view_of(&doc.data)),
        Err(e) => ApiResult::ok(BoscoView::empty(
            e.class.code(),
            Some(format!("{}：{}", e.class.human(), e.detail)),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    fn row_for<'a>(rows: &'a [UpgradeView], asset_id: &str) -> &'a UpgradeView {
        rows.iter().find(|r| r.id == asset_id).expect("表里应该有这条")
    }

    #[test]
    fn upgrade_denominator_is_eleven_not_thirteen() {
        let rows = rows_of(&HashSet::new(), &HashSet::new());
        assert_eq!(rows.len(), 11, "两条 unused 不能进列表，否则分母变成 13");
        assert!(
            !rows.iter().any(|r| r.id == "f0f719d1e9608046a4462e8f144b655e"),
            "UPG_Bosco_ExtraAbillityCharge 应被过滤"
        );
        assert!(
            !rows.iter().any(|r| r.id == "8295a9472b9215469acde56a024f25f8"),
            "UPG_Bosco_SAAOE 应被过滤"
        );
    }

    #[test]
    fn purchased_but_not_equipped_is_a_different_state() {
        // 快速装填：买过、没装
        let rows = rows_of(&set(&["1dd857328533fd4194dd687f552aa0f9"]), &HashSet::new());
        let r = row_for(&rows, "1dd857328533fd4194dd687f552aa0f9");
        assert!(r.owned);
        assert!(!r.equipped);

        // 火箭弹：买过、装着
        let rows = rows_of(
            &set(&["c828ad05f868754ebc4fa53df3063725"]),
            &set(&["c828ad05f868754ebc4fa53df3063725"]),
        );
        let r = row_for(&rows, "c828ad05f868754ebc4fa53df3063725");
        assert!(r.owned);
        assert!(r.equipped);
    }

    #[test]
    fn light_radius_keeps_the_locres_name_and_is_flagged() {
        let rows = rows_of(&HashSet::new(), &HashSet::new());
        let r = row_for(&rows, "5c14ac9e04fa9041aabb75f443cb62d0");
        assert_eq!(r.name, "A1 高闪来一个");
        assert_eq!(r.en, "Let There Be Light");
    }

    #[test]
    fn skins_total_is_forty_five_including_global_folder_ones() {
        let (rows, have, total) = skins_of(&HashSet::new());
        assert_eq!(total, 45);
        assert_eq!(have, 0);
        assert_eq!(rows.len(), 45);
        // 曾经被记成「全局」的 6 条要算在博斯科里
        assert!(rows.iter().any(|r| r.id == "4a35d244688f7a45808671eaf3f3f170"));
    }

    #[test]
    fn unlocked_skins_count_only_what_the_save_says() {
        let (_, have, total) = skins_of(&set(&[
            "1c947eadefaeec48a97f69e93dc1b9c9",
            "fee18c6e8cdec440babf0731a4660ef3",
        ]));
        assert_eq!(have, 2);
        assert_eq!(total, 45);
    }

    #[test]
    fn current_skin_names_come_from_the_save() {
        let cur = current_of(&["1c947eadefaeec48a97f69e93dc1b9c9".to_string()]);
        assert_eq!(cur.len(), 1);
        assert!(cur[0].name.is_some());
    }
}
