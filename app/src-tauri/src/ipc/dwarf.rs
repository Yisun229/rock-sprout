//! 「我的矿工」页要的两块数据：角色块与里程碑块。
//!
//! 这里只做聚合：等级靠 `app/src/data/levels.json` 的阈值表算，名字靠
//! `app/src/data/milestone-names.json` 查，**代码里不写死任何数字或名字**。
//! 表读不到、曲线对不上时，等级返回 `null`，让前端退回显示原始 XP。

use std::collections::HashMap;

use serde::Deserialize;
use serde::Serialize;

use super::save::pick;
use super::ApiResult;
use crate::paths;
use crate::save::store::{self, LoadClass};

const LEVELS_JSON: &str = include_str!("../../../src/data/levels.json");
const MILESTONES_JSON: &str = include_str!("../../../src/data/milestone-names.json");
const WEAPONS_JSON: &str = include_str!("../../../src/data/weapon-names.json");

#[derive(Deserialize)]
struct WeaponFile {
    weapons: HashMap<String, WeaponRow>,
}

#[derive(Deserialize)]
pub(crate) struct WeaponRow {
    #[serde(default)]
    pub(crate) class: Option<String>,
    #[serde(default)]
    pub(crate) slot: Option<String>,
    #[serde(default)]
    pub(crate) zh: Option<String>,
    #[serde(default)]
    pub(crate) en: Option<String>,
    /// 资产所在的文件夹名（如 `FlameThrower`）——超频表的 `weapon` 用这套命名
    #[serde(default)]
    pub(crate) folder: Option<String>,
    /// 这一条是武器还是任务工具（目前只有三条 `"tool"`）。没写这个字段的条目按武器处理：
    /// **缺省值和 `"weapon"` 走同一条路**，别让它们在代码里变成两个分支。
    #[serde(default)]
    pub(crate) kind: Option<String>,
}

pub(crate) fn weapon_table() -> &'static HashMap<String, WeaponRow> {
    static TABLE: std::sync::OnceLock<HashMap<String, WeaponRow>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str::<WeaponFile>(WEAPONS_JSON)
            .map(|f| {
                f.weapons
                    .into_iter()
                    .map(|(k, v)| (k.to_lowercase(), v))
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// 给「装备」页用：按 GUID 查中文名/英文名/职业/槽位/资产文件夹名。查不到返回 `None`。
pub(crate) fn lookup_weapon(
    guid: &str,
) -> Option<(Option<String>, Option<String>, String, String, Option<String>)> {
    let table = weapon_table();
    let row = table.get(&guid.to_lowercase())?;
    Some((
        row.class.clone(),
        row.slot.clone(),
        row.zh.clone().unwrap_or_default(),
        row.en.clone().unwrap_or_default(),
        row.folder.clone(),
    ))
}

/// 这条 GUID 是不是任务工具（管线枪、凿岩器、共振扫描器）。
///
/// 这三件东西会以「武器」的身份出现在存档的配装格子里，但它们不是能拿去下矿的装备；
/// 只有 `kind` 明确写了 `"tool"` 才算，没写和写了别的都当武器。
pub(crate) fn is_tool(guid: &str) -> bool {
    weapon_table()
        .get(&guid.to_lowercase())
        .is_some_and(|row| {
            row.kind
                .as_deref()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("tool"))
        })
}

/// 判据只看主武器与副武器的 GUID（不投票、不取多数）：
/// 命中的每条都给出职业，全部一致才算认出；出现两种职业算「认不出」；
/// 查不到、或查到的是不带职业的条目（比如无人机的 GUID 落在主武器格里），
/// 一律当「没装」，既不参与判定，也不算「认不出」。
pub fn class_from_guids(
    guids: &[String],
    lookup: &HashMap<String, WeaponRow>,
) -> (Option<String>, Option<String>) {
    let mut hit: Vec<(String, String)> = Vec::new(); // (职业, 来源说明)
    for guid in guids {
        let Some(row) = lookup.get(&guid.to_lowercase()) else {
            continue;
        };
        let Some(class) = row.class.as_deref().filter(|c| !c.is_empty()) else {
            continue;
        };
        let what = match row.slot.as_deref() {
            Some("primary") => "主武器",
            Some("secondary") => "副武器",
            _ => "武器",
        };
        hit.push((class.to_string(), what.to_string()));
    }

    let mut kinds: Vec<&str> = hit.iter().map(|(c, _)| c.as_str()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    match kinds.len() {
        0 => (None, None),
        1 => {
            let mut seen: Vec<String> = Vec::new();
            for (c, what) in &hit {
                let one = format!("{what}={c}");
                if !seen.contains(&one) {
                    seen.push(one);
                }
            }
            (Some(kinds[0].to_string()), Some(seen.join("，")))
        }
        _ => (
            None,
            Some(format!(
                "主副武器指向不同的职业（{}），按规矩不算认出",
                kinds.join(" / ")
            )),
        ),
    }
}

#[derive(Deserialize)]
struct LevelTable {
    thresholds: Vec<i64>,
}

#[derive(Deserialize)]
struct MilestoneFile {
    milestones: HashMap<String, MilestoneRow>,
}

#[derive(Deserialize)]
struct MilestoneRow {
    #[serde(default)]
    en: String,
    #[serde(default)]
    character: Option<String>,
    #[serde(default)]
    tiers: Vec<MilestoneTierRow>,
}

#[derive(Deserialize)]
struct MilestoneTierRow {
    #[serde(rename = "count")]
    count: i64,
    #[serde(rename = "perkPoints")]
    perk_points: i64,
}

/// 等级换算：满足 `xp >= thresholds[i]` 的**最大** `i`，再加一。
/// 索引从 0 起（`thresholds[0]` 是第 1 级的起点），正好等于阈值算这一级。
/// 比最低阈值还低、或者表是空的，返回 `None`（前端据此退回 XP）。
pub fn level_of(xp: i64, thresholds: &[i64]) -> Option<i64> {
    if thresholds.is_empty() {
        return None;
    }
    let (mut lo, mut hi) = (0usize, thresholds.len());
    while lo < hi {
        let mid = (lo + hi) / 2;
        if thresholds[mid] <= xp {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    if lo == 0 {
        return None;
    }
    Some(lo as i64)
}

fn levels() -> Vec<i64> {
    serde_json::from_str::<LevelTable>(LEVELS_JSON)
        .map(|t| t.thresholds)
        .unwrap_or_default()
}

fn milestone_table() -> HashMap<String, MilestoneRow> {
    serde_json::from_str::<MilestoneFile>(MILESTONES_JSON)
        .map(|f| f.milestones)
        .unwrap_or_default()
}

#[derive(Serialize)]
pub struct CharacterRow {
    /// 从 1 开始的槽位号，和游戏里从上往下数一致
    pub slot: i64,
    pub xp: i64,
    /// 曲线缺失或 XP 低于最低阈值时为 `null`
    pub level: Option<i64>,
    pub times_retired: i64,
    pub retired_levels: i64,
    /// XP 为 0 就当还没进度（存档里没有「这个槽建没建过人」的可靠标记）
    pub has_progress: bool,
    /// 靠主副武器反推出来的职业；认不出就是 `null`
    pub class: Option<String>,
    /// 凭什么这么判（写给人看的一行），认不出时写原因
    pub basis: Option<String>,
}

#[derive(Serialize)]
pub struct MilestoneRowView {
    pub id: String,
    /// 查不到名字就是 `null`，前端显示 `#id 前 8 位`
    pub name: Option<String>,
    /// 把 `{value}` / `{character}` 填好之后的显示名（名字查不到时为空串）
    pub display: String,
    pub tier: i64,
    pub max_tier: i64,
    /// 下一级给多少点；已满级或查不到就是 `null`
    pub next_perk_points: Option<i64>,
}

#[derive(Serialize)]
pub struct DwarfView {
    pub state: String,
    pub note: String,
    pub file_name: Option<String>,
    pub from_cache: bool,
    /// 曲线表能不能用（前端据此决定显示等级还是 XP）
    pub curve_ok: bool,
    pub characters: Vec<CharacterRow>,
    pub milestones: Vec<MilestoneRowView>,
    pub milestones_claimed: i64,
    pub milestones_total: i64,
}

impl DwarfView {
    fn with_state(state: &str, note: &str) -> Self {
        Self {
            state: state.to_string(),
            note: note.to_string(),
            file_name: None,
            from_cache: false,
            curve_ok: false,
            characters: Vec::new(),
            milestones: Vec::new(),
            milestones_claimed: 0,
            milestones_total: 0,
        }
    }

    fn broken(class: LoadClass, detail: &str) -> Self {
        let mut view = Self::with_state(class.code(), class.human());
        if !detail.is_empty() {
            view.note = format!("{}：{}", class.human(), detail);
        }
        view
    }
}

/// 把资产里的标题模板填成能直接显示的一句：
/// `{value}` 换成「这一级要多少次」——已领的用已领到那级的门槛，没领的用第一级；
/// `{character}` 换成资产上记的职业名。
fn fill_title(row: &MilestoneRow, tier: i64) -> String {
    let mut text = row.en.clone();
    if text.contains("{value}") {
        let idx = if tier > 0 {
            ((tier - 1) as usize).min(row.tiers.len().saturating_sub(1))
        } else {
            0
        };
        let count = row.tiers.get(idx).map(|t| t.count).unwrap_or(0);
        text = text.replace("{value}", &count.to_string());
    }
    if let Some(who) = &row.character {
        text = text.replace("{character}", who);
    }
    text
}

#[tauri::command]
pub fn my_dwarf_view(selected: Option<String>) -> ApiResult<DwarfView> {
    let Some(file) = pick(selected.as_deref()) else {
        return ApiResult::ok(DwarfView::with_state(
            "not_found",
            "没找到存档：这台机器上还没存过档，或者装的是别的版本（目前只认 Steam 版）",
        ));
    };

    let cache_root = paths::data_root().join("cache");
    match store::load(&file.path, &cache_root) {
        Ok((doc, report)) => {
            let data = &doc.data;
            let thresholds = levels();
            let curve_ok = !thresholds.is_empty();
            let names = milestone_table();
            let weapons = weapon_table();

            let characters: Vec<CharacterRow> = data
                .CharacterSaves
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let xp = i64::from(c.XP);
                    // 只看前三套配装的主武器与副武器；其余槽位不参与判定
                    let mut guids: Vec<String> = Vec::new();
                    for loadout in c.Loadouts.iter().take(3) {
                        if !loadout.PrimaryWeapon.is_empty() {
                            guids.push(loadout.PrimaryWeapon.clone());
                        }
                        if !loadout.SecondaryWeapon.is_empty() {
                            guids.push(loadout.SecondaryWeapon.clone());
                        }
                    }
                    let (class, basis) = class_from_guids(&guids, weapons);
                    CharacterRow {
                        slot: i as i64 + 1,
                        xp,
                        level: level_of(xp, &thresholds),
                        times_retired: i64::from(c.TimesRetired),
                        retired_levels: i64::from(c.RetiredCharacterLevels),
                        has_progress: c.XP > 0 || c.TimesRetired > 0,
                        class,
                        basis,
                    }
                })
                .collect();

            let mut milestones: Vec<MilestoneRowView> = data
                .Progress
                .Milestones
                .iter()
                .map(|m| {
                    let key = m.KPIGuid.to_lowercase();
                    let row = names.get(&key);
                    let tier = i64::from(m.Tier);
                    let max_tier = row.map(|r| r.tiers.len() as i64).unwrap_or(0);
                    let next_perk_points = row.and_then(|r| {
                        let idx = (tier.max(0)) as usize; // tier 是「已领到第几级」，下一级是 tiers[tier]
                        r.tiers.get(idx).map(|t| t.perk_points)
                    });
                    MilestoneRowView {
                        id: key,
                        name: row.map(|r| r.en.clone()).filter(|s| !s.is_empty()),
                        display: row.map(|r| fill_title(r, tier)).unwrap_or_default(),
                        tier,
                        max_tier,
                        next_perk_points,
                    }
                })
                .collect();
            // 已领的排前面，同一档里按名字排（没名字的排后面）
            milestones.sort_by(|a, b| {
                b.tier
                    .cmp(&a.tier)
                    .then_with(|| a.name.cmp(&b.name))
                    .then_with(|| a.id.cmp(&b.id))
            });

            let claimed = milestones.iter().filter(|m| m.tier > 0).count() as i64;
            let total = milestones.len() as i64;

            ApiResult::ok(DwarfView {
                state: "ok".to_string(),
                note: String::new(),
                file_name: file
                    .path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string()),
                from_cache: report.from_cache,
                curve_ok,
                characters,
                milestones,
                milestones_claimed: claimed,
                milestones_total: total,
            })
        }
        Err(e) => ApiResult::ok(DwarfView::broken(e.class, &e.detail)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CURVE: [i64; 25] = [
        0, 3000, 7000, 12000, 18000, 25000, 33000, 42000, 52000, 63000, 75000, 88000, 102000,
        117000, 132500, 148500, 165000, 182000, 199500, 217500, 236000, 255000, 274500, 294500,
        315000,
    ];

    #[test]
    fn level_boundaries_are_left_closed() {
        assert_eq!(level_of(0, &CURVE), Some(1));
        assert_eq!(level_of(2999, &CURVE), Some(1));
        assert_eq!(level_of(3000, &CURVE), Some(2));
        assert_eq!(level_of(6999, &CURVE), Some(2));
        assert_eq!(level_of(7000, &CURVE), Some(3));
        assert_eq!(level_of(7047, &CURVE), Some(3));
        assert_eq!(level_of(8121, &CURVE), Some(3));
        assert_eq!(level_of(11999, &CURVE), Some(3));
        assert_eq!(level_of(12000, &CURVE), Some(4));
    }

    #[test]
    fn level_caps_at_last_threshold() {
        assert_eq!(level_of(314999, &CURVE), Some(24));
        assert_eq!(level_of(315000, &CURVE), Some(25));
        assert_eq!(level_of(999999, &CURVE), Some(25));
    }

    #[test]
    fn level_is_none_without_curve_or_below_floor() {
        assert_eq!(level_of(5000, &[]), None);
        assert_eq!(level_of(-1, &CURVE), None);
    }

    #[test]
    fn shipped_curve_matches_the_documented_one() {
        assert_eq!(levels(), CURVE.to_vec());
    }

    #[test]
    fn milestone_table_is_loaded_and_keyed_by_lowercase_guid() {
        let table = milestone_table();
        assert_eq!(table.len(), 56, "里程碑表应有 56 条");
        assert!(table.keys().all(|k| k.len() == 32 && k.chars().all(|c| c.is_ascii_hexdigit())));
    }

    fn fake_table(rows: &[(&str, Option<&str>, Option<&str>)]) -> HashMap<String, WeaponRow> {
        rows.iter()
            .map(|(guid, class, slot)| {
                (
                    guid.to_lowercase(),
                    WeaponRow {
                        class: class.map(|s| s.to_string()),
                        slot: slot.map(|s| s.to_string()),
                        zh: None,
                        en: None,
                        folder: None,
                        kind: None,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn one_weapon_is_enough_to_pin_the_class() {
        let table = fake_table(&[("aa", Some("driller"), Some("primary"))]);
        let (class, basis) = class_from_guids(&["aa".to_string()], &table);
        assert_eq!(class.as_deref(), Some("driller"));
        assert!(basis.unwrap().contains("主武器"));
    }

    #[test]
    fn two_weapons_of_the_same_class_agree() {
        let table = fake_table(&[
            ("aa", Some("scout"), Some("primary")),
            ("bb", Some("scout"), Some("secondary")),
        ]);
        let (class, _) = class_from_guids(&["aa".to_string(), "bb".to_string()], &table);
        assert_eq!(class.as_deref(), Some("scout"));
    }

    #[test]
    fn conflicting_classes_are_not_a_guess() {
        let table = fake_table(&[
            ("aa", Some("gunner"), Some("primary")),
            ("bb", Some("engineer"), Some("secondary")),
        ]);
        let (class, basis) = class_from_guids(&["aa".to_string(), "bb".to_string()], &table);
        assert!(class.is_none(), "冲突时不许认成其中任何一个");
        assert!(basis.unwrap().contains("不算认出"));
    }

    #[test]
    fn unknown_and_classless_entries_count_as_nothing() {
        // 查不到的 GUID、以及表里没有职业的条目（比如无人机的 GUID 落在主武器格里），
        // 都按「没装」处理：既不认职业，也不算「认不出」。
        let table = fake_table(&[("bosco", None, None)]);
        let (class, basis) = class_from_guids(&["zzz".to_string(), "bosco".to_string()], &table);
        assert!(class.is_none());
        assert!(basis.is_none(), "没装不该写成判据理由");
    }

    #[test]
    fn shipped_weapon_table_has_three_primaries_and_secondaries_per_class() {
        let table = weapon_table();
        let mut counts: HashMap<(String, String), usize> = HashMap::new();
        for row in table.values() {
            let (Some(class), Some(slot)) = (row.class.as_deref(), row.slot.as_deref()) else {
                continue;
            };
            if slot == "primary" || slot == "secondary" {
                *counts
                    .entry((class.to_string(), slot.to_string()))
                    .or_default() += 1;
            }
        }
        for class in ["driller", "engineer", "gunner", "scout"] {
            for slot in ["primary", "secondary"] {
                let n = counts
                    .get(&(class.to_string(), slot.to_string()))
                    .copied()
                    .unwrap_or(0);
                assert_eq!(n, 3, "{class} 的 {slot} 应有 3 把，实际 {n}");
            }
        }
    }
}
