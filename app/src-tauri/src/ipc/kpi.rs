//! KPI（里程碑）对照：#28。
//!
//! 存档里只有「已经领到第几档」（`Milestones.ClaimedKPIRewards[].Tier`），
//! **没有**每一条当前的累计数（比如「采矿探险 137 次」）。所以这张表能给出的是：
//! 每条 KPI 有几档、每档要多少次、每档给多少天赋点、你已经领到第几档、
//! 下一档的门槛是多少——「还差多少次」游戏里才显示，这里不编。
//!
//! 分母口径：名字表来自 pak 里全部 `MIL_*` 资产（57 条），不是某一份存档里出现过的。

use std::collections::HashMap;

use serde::Deserialize;
use serde::Serialize;

use super::ApiResult;
use crate::save::model::MilestoneEntry;

const MILESTONE_NAMES: &str = include_str!("../../../src/data/milestone-names.json");

#[derive(Deserialize)]
struct NamesFile {
    milestones: HashMap<String, KpiDef>,
}

#[derive(Deserialize)]
struct KpiDef {
    #[serde(default)]
    asset: String,
    #[serde(default)]
    en: String,
    #[serde(default)]
    zh: String,
    #[serde(default)]
    tiers: Vec<KpiTier>,
    #[serde(default)]
    character: Option<String>,
}

#[derive(Deserialize)]
struct KpiTier {
    /// 这一档要求的次数（资产里的 `Tiers[].Count`）
    count: i64,
    /// 这一档给的天赋点（资产里的 `Tiers[].PerkPoints`）
    #[serde(rename = "perkPoints")]
    perk_points: i64,
}

#[derive(Serialize)]
pub struct KpiTierView {
    pub count: i64,
    pub perk_points: i64,
}

#[derive(Serialize)]
pub struct KpiRow {
    pub id: String,
    /// 中文名（占位符换成「次数」，见 `fill_placeholders`）
    pub name: String,
    pub name_en: String,
    pub asset: String,
    pub character: Option<String>,
    pub tiers: Vec<KpiTierView>,
    /// 已领到第几档（0 = 一档都没领）
    pub claimed: i32,
    /// 这一条已经赚到的天赋点
    pub claimed_points: i64,
    /// 下一档门槛；全领完是 None
    pub next_count: Option<i64>,
}

#[derive(Serialize)]
pub struct KpisView {
    pub state: String,
    pub note: String,
    pub rows: Vec<KpiRow>,
    pub total: usize,
    pub total_tiers: usize,
    pub claimed_tiers: usize,
    pub claimed_points: i64,
    /// 全部档位加起来能给多少点（分母）
    pub possible_points: i64,
}

impl KpisView {
    pub fn with_state(state: &str, note: &str) -> Self {
        Self {
            state: state.to_string(),
            note: note.to_string(),
            rows: Vec::new(),
            total: 0,
            total_tiers: 0,
            claimed_tiers: 0,
            claimed_points: 0,
            possible_points: 0,
        }
    }
}

/// `完成 {value} 个“采矿探险”任务` → 中文界面里保留 {value} 不好看，
/// 换成「N 次」的意思：这里统一把占位符换成一个可读的「若干次」。
fn fill_placeholders(text: &str) -> String {
    text.replace("{value}", "N")
        .replace("{character}", "该职业")
}

/// 职业的存档英文名 → 中文名；表里认不出的原样返回（不猜）。
fn class_name(raw: &str) -> String {
    match raw {
        "Gunner" => "枪手".to_string(),
        "Engineer" => "工程师".to_string(),
        "Driller" => "钻机手".to_string(),
        "Scout" => "侦察兵".to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn view_of(claimed: &[MilestoneEntry]) -> KpisView {
    let names: NamesFile = serde_json::from_str(MILESTONE_NAMES).expect("milestone-names.json 读不了");
    let by_id: HashMap<String, i32> = claimed
        .iter()
        .map(|one| (one.KPIGuid.to_ascii_lowercase(), one.Tier))
        .collect();

    let mut rows: Vec<KpiRow> = names
        .milestones
        .iter()
        .map(|(id, def)| {
            let tiers: Vec<KpiTierView> = def
                .tiers
                .iter()
                .map(|tier| KpiTierView {
                    count: tier.count,
                    perk_points: tier.perk_points,
                })
                .collect();
            let claimed_tier = *by_id.get(&id.to_ascii_lowercase()).unwrap_or(&0);
            let claimed_tier = claimed_tier.clamp(0, tiers.len() as i32);
            let claimed_points: i64 = tiers
                .iter()
                .take(claimed_tier as usize)
                .map(|tier| tier.perk_points)
                .sum();
            let next_count = tiers.get(claimed_tier as usize).map(|tier| tier.count);
            KpiRow {
                id: id.clone(),
                name: fill_placeholders(if def.zh.is_empty() { &def.en } else { &def.zh }),
                name_en: def.en.clone(),
                asset: def.asset.clone(),
                character: def.character.as_deref().map(class_name),
                tiers,
                claimed: claimed_tier,
                claimed_points,
                next_count,
            }
        })
        .collect();

    // 已经领过的排前面，同档数按中文名排，最后按 id 保证稳定
    rows.sort_by(|a, b| {
        b.claimed
            .cmp(&a.claimed)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    });

    let total_tiers: usize = rows.iter().map(|row| row.tiers.len()).sum();
    let claimed_tiers: usize = rows.iter().map(|row| row.claimed.max(0) as usize).sum();
    let claimed_points: i64 = rows.iter().map(|row| row.claimed_points).sum();
    let possible_points: i64 = rows
        .iter()
        .flat_map(|row| row.tiers.iter())
        .map(|tier| tier.perk_points)
        .sum();

    KpisView {
        state: "ok".to_string(),
        note: "存档里只有「领到第几档」，没有当前的累计次数——「还差多少次」游戏里才显示，这张表不编。"
            .to_string(),
        total: rows.len(),
        total_tiers,
        claimed_tiers,
        claimed_points,
        possible_points,
        rows,
    }
}

#[tauri::command]
pub fn kpis(selected: Option<String>) -> ApiResult<KpisView> {
    let Some(file) = super::save::pick(selected.as_deref()) else {
        return ApiResult::ok(KpisView::with_state(
            "not_found",
            "没找到存档：这台机器上还没存过档，或者装的是别的版本（目前只认 Steam 版）",
        ));
    };

    let cache_root = crate::paths::data_root().join("cache");
    match crate::save::store::load(&file.path, &cache_root) {
        Ok((doc, _report)) => {
            let mut view = view_of(&doc.data.Progress.Milestones);
            view.note = format!(
                "{}；这张表的分母是游戏里全部 57 条 KPI，不是这份存档里出现过的。",
                view.note
            );
            ApiResult::ok(view)
        }
        Err(e) => ApiResult::ok(KpisView::with_state(
            e.class.code(),
            &format!("{}：{}", e.class.human(), e.detail),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, tier: i32) -> MilestoneEntry {
        MilestoneEntry {
            KPIGuid: id.to_string(),
            Tier: tier,
        }
    }

    #[test]
    fn all_57_kpis_are_listed_even_without_any_claim() {
        let view = view_of(&[]);
        assert_eq!(view.total, 57);
        assert!(view.total_tiers > 57);
        assert_eq!(view.claimed_tiers, 0);
        assert!(view.rows.iter().all(|row| row.claimed == 0));
        assert!(view.rows.iter().all(|row| row.next_count.is_some()));
    }

    #[test]
    fn claimed_tier_counts_points_only_up_to_that_tier() {
        let names: NamesFile =
            serde_json::from_str(MILESTONE_NAMES).expect("milestone-names.json 读不了");
        let (id, def) = names
            .milestones
            .iter()
            .find(|(_, def)| def.tiers.len() >= 3)
            .expect("至少要有一条三档以上的 KPI");
        let view = view_of(&[entry(id, 2)]);
        let row = view
            .rows
            .iter()
            .find(|row| &row.id == id)
            .expect("这条 KPI 应该在表里");
        assert_eq!(row.claimed, 2);
        let expect: i64 = def.tiers.iter().take(2).map(|tier| tier.perk_points).sum();
        assert_eq!(row.claimed_points, expect);
        assert_eq!(row.next_count, Some(def.tiers[2].count));
        assert_eq!(view.claimed_tiers, 2);
    }

    #[test]
    fn claim_past_the_last_tier_is_clamped_not_panicking() {
        let names: NamesFile =
            serde_json::from_str(MILESTONE_NAMES).expect("milestone-names.json 读不了");
        let (id, def) = names.milestones.iter().next().expect("表里至少有一条");
        let view = view_of(&[entry(id, 99)]);
        let row = view.rows.iter().find(|row| &row.id == id).expect("应该在表里");
        assert_eq!(row.claimed as usize, def.tiers.len());
        assert_eq!(row.next_count, None);
    }

    #[test]
    fn placeholders_are_not_left_in_the_table() {
        let view = view_of(&[]);
        for row in &view.rows {
            assert!(!row.name.contains("{value}"), "{} 还留着占位符", row.name);
            assert!(!row.name.contains("{character}"), "{} 还留着占位符", row.name);
        }
    }
}
