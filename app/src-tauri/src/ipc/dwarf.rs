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

            let characters: Vec<CharacterRow> = data
                .CharacterSaves
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let xp = i64::from(c.XP);
                    CharacterRow {
                        slot: i as i64 + 1,
                        xp,
                        level: level_of(xp, &thresholds),
                        times_retired: i64::from(c.TimesRetired),
                        retired_levels: i64::from(c.RetiredCharacterLevels),
                        has_progress: c.XP > 0 || c.TimesRetired > 0,
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
}
