//! 天赋点数：已赚、已花、还剩，以及三者是否闭合。
//!
//! 三式（`文档/v0.2.4-实施计划.md` §2.2）：
//!
//! ```text
//! 已赚 = Σ 已领里程碑  milestones[guid].tiers[0 .. Tier-1].perkPoints
//! 已花 = Σ 已买天赋    perk_costs[guid].ranks[0 .. currentRank-1].cost
//! 还剩 = 存档 PerkPoints（直接读，不参与加总）
//! 校验 = 已赚 - 已花 == 还剩
//! ```
//!
//! **这张表是唯一能证明 `Ranks` 读法的东西**：资产的字节对齐没有上游文档，只有
//! 「里程碑赚到多少」和「天赋花掉多少」这两个互相独立的来源算出来的差，正好等于
//! 存档里剩下的点数，才算读对了。所以任何一格算不出来（表里没有、级数超出）时
//! 一律 `verified: false`，绝不把缺的那部分当 0 去硬凑。

use std::collections::HashMap;

use serde::Deserialize;

use super::dwarf::{milestone_table, MilestoneRow};
use crate::save::model::{MilestoneEntry, PerkEntry};

const PERK_COSTS_JSON: &str = include_str!("../../../src/data/perk-costs.json");

#[derive(Deserialize)]
struct CostsFile {
    perks: Vec<PerkCostRow>,
}

#[derive(Deserialize)]
pub(crate) struct PerkCostRow {
    pub(crate) guid: String,
    /// 资产名（`PERK_Vampire`），出问题时报错用得上
    pub(crate) asset: String,
    pub(crate) ranks: Vec<RankRow>,
}

#[derive(Deserialize)]
pub(crate) struct RankRow {
    pub(crate) tier: i64,
    pub(crate) cost: i64,
}

/// 天赋价格表，键统一小写十六进制。
pub(crate) fn perk_costs() -> &'static HashMap<String, PerkCostRow> {
    static TABLE: std::sync::OnceLock<HashMap<String, PerkCostRow>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str::<CostsFile>(PERK_COSTS_JSON)
            .map(|f| {
                f.perks
                    .into_iter()
                    .map(|row| (row.guid.to_lowercase(), row))
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// 三个数与校验结果。`verified` 为假时 `earned` / `spent` 都是 `None`——
/// 界面拿不到这两个数，也就没法把两个数摆在一起「看着差不多」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct PointsCheck {
    pub earned: Option<i64>,
    pub spent: Option<i64>,
    /// 存档里的 `PerkPoints`，永远给真值，不参与校验
    pub left: i64,
    pub verified: bool,
}

/// 已赚：已领里程碑按 `Tier` 累加它前面各级的点数。
///
/// - `Tier = 0`：没领到任何一级，贡献 0（回归样本里真有这种元素，别当脏数据丢掉）
/// - 表里没有这条里程碑，或者 `Tier` 超过了资产的级数：返回 `None`，因为算不全
fn earned_of(claimed: &[MilestoneEntry]) -> Option<i64> {
    let table = milestone_table();
    let mut sum: i64 = 0;
    for entry in claimed {
        let row: &MilestoneRow = table.get(&entry.KPIGuid.to_lowercase())?;
        let tier = entry.Tier.max(0) as usize;
        if tier == 0 {
            continue;
        }
        let tiers = row.tiers.get(..tier)?;
        sum += tiers.iter().map(|t| t.perk_points).sum::<i64>();
    }
    Some(sum)
}

/// 已花：已买天赋按 `currentRank` 累加它前面各级的花费。
///
/// - `currentRank = 0`：买了但一级都没点，贡献 0
/// - 表里没有这条天赋，或者 `currentRank` 超过了资产的级数：返回 `None`
fn spent_of(owned: &[PerkEntry]) -> Option<i64> {
    let table = perk_costs();
    let mut sum: i64 = 0;
    for entry in owned {
        let row = table.get(&entry.PerkID.to_lowercase())?;
        let rank = entry.currentRank.max(0) as usize;
        if rank == 0 {
            continue;
        }
        let ranks = row.ranks.get(..rank)?;
        sum += ranks.iter().map(|r| r.cost).sum::<i64>();
    }
    Some(sum)
}

/// 把三式计算出来并与存档对齐。
pub fn points_check(
    perk_points: i32,
    claimed: &[MilestoneEntry],
    owned: &[PerkEntry],
) -> PointsCheck {
    let earned = earned_of(claimed);
    let spent = spent_of(owned);
    let left = i64::from(perk_points);
    let verified = match (earned, spent) {
        (Some(earned), Some(spent)) => earned - spent == left,
        _ => false,
    };
    PointsCheck {
        earned: if verified { earned } else { None },
        spent: if verified { spent } else { None },
        left,
        verified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claimed(guid: &str, tier: i32) -> MilestoneEntry {
        MilestoneEntry {
            KPIGuid: guid.to_string(),
            Tier: tier,
        }
    }

    fn owned(guid: &str, rank: i32) -> PerkEntry {
        PerkEntry {
            PerkID: guid.to_string(),
            currentRank: rank,
        }
    }

    /// 真实存在的一对：Scout 的升级里程碑（四级，每级 5 点）
    const SCOUT_LEVELUP: &str = "e06479f5dff94e498258a84b4048436a";
    /// Vampire 的逐级花费是 2 / 3 / 5 / 7
    const VAMPIRE: &str = "f747e1d81bba744980e385e8610d11a6";
    /// 没有 Ranks 的那一条（靠晋升解锁，ranks 为空数组）
    const ACTIVE_SLOT: &str = "b50bec28941fd047b8be00f1e3a96464";

    #[test]
    fn zero_earned_and_zero_spent_only_match_when_nothing_is_left() {
        let ok = points_check(0, &[], &[]);
        assert_eq!(
            ok,
            PointsCheck {
                earned: Some(0),
                spent: Some(0),
                left: 0,
                verified: true,
            }
        );
        // 没赚没花却还剩 5 点：三个数都对，但加不起来 → 不许报 verified
        let bad = points_check(5, &[], &[]);
        assert!(!bad.verified);
        assert_eq!((bad.earned, bad.spent, bad.left), (None, None, 5));
    }

    #[test]
    fn tier_zero_contributes_nothing() {
        // 回归样本里存在 Tier=0 的已领里程碑：是数据，不是脏数据，贡献 0
        let check = points_check(0, &[claimed(SCOUT_LEVELUP, 0)], &[]);
        assert!(check.verified);
        assert_eq!(check.earned, Some(0));
    }

    #[test]
    fn four_tiers_of_the_same_milestone_pay_twenty() {
        let check = points_check(20, &[claimed(SCOUT_LEVELUP, 4)], &[]);
        assert!(check.verified);
        assert_eq!(check.earned, Some(20));
    }

    #[test]
    fn perk_costs_follow_the_asset_table() {
        // Vampire 四级：2 + 3 + 5 + 7 = 17
        let check = points_check(-17, &[], &[owned(VAMPIRE, 4)]);
        assert!(check.verified);
        assert_eq!(check.spent, Some(17));
        // 只点了一级：2
        let check = points_check(-2, &[], &[owned(VAMPIRE, 1)]);
        assert_eq!(check.spent, Some(2));
    }

    #[test]
    fn mismatch_is_not_verified_and_hides_both_numbers() {
        let check = points_check(35, &[claimed(SCOUT_LEVELUP, 4)], &[owned(VAMPIRE, 4)]);
        // 20 - 17 = 3，存档写 35：对不上
        assert!(!check.verified);
        assert_eq!(check.earned, None);
        assert_eq!(check.spent, None);
        assert_eq!(check.left, 35);
    }

    #[test]
    fn tier_beyond_the_asset_is_unknown_not_zero() {
        assert!(!points_check(0, &[claimed(SCOUT_LEVELUP, 5)], &[]).verified);
        assert!(!points_check(0, &[], &[owned(VAMPIRE, 5)]).verified);
    }

    #[test]
    fn unknown_guids_mean_we_cannot_verify() {
        let unknown = "00000000000000000000000000000000";
        assert!(!points_check(0, &[claimed(unknown, 1)], &[]).verified);
        assert!(!points_check(0, &[], &[owned(unknown, 1)]).verified);
    }

    #[test]
    fn perk_without_ranks_is_free_until_it_gets_a_level() {
        // PERK_PerkSlots_Active 没有 Ranks：没点时贡献 0，一旦有 rank 就算不出来
        assert!(points_check(0, &[], &[owned(ACTIVE_SLOT, 0)]).verified);
        assert!(!points_check(0, &[], &[owned(ACTIVE_SLOT, 1)]).verified);
    }
}
