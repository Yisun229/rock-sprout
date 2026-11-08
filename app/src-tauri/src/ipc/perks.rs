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
use super::ApiResult;
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

// ---------------------------------------------------------------------------
// 天赋页要的整块数据：三个数 + 已买清单 + 全部 24 条 + 每角色当前生效的那套
// ---------------------------------------------------------------------------

/// 全零 GUID：存档里没用上的角色槽会用这个值占位。
const ZERO_GUID: &str = "00000000000000000000000000000000";

const PERK_NAMES_JSON: &str = include_str!("../../../src/data/perk-names.json");

#[derive(Deserialize)]
struct NamesFile {
    perks: Vec<NameRow>,
}

#[derive(Deserialize)]
struct NameRow {
    guid: String,
    en: String,
    #[serde(default)]
    zh: Option<String>,
}

/// 名字表，键统一小写十六进制。
fn perk_names() -> &'static HashMap<String, NameRow> {
    static TABLE: std::sync::OnceLock<HashMap<String, NameRow>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str::<NamesFile>(PERK_NAMES_JSON)
            .map(|f| {
                f.perks
                    .into_iter()
                    .map(|row| (row.guid.to_lowercase(), row))
                    .collect()
            })
            .unwrap_or_default()
    })
}

fn name_of(guid: &str) -> PerkName {
    match perk_names().get(guid) {
        Some(row) => PerkName {
            en: row.en.clone(),
            zh: row.zh.clone(),
        },
        // 表里没有也不编：英文名退回 GUID，界面自己能看出这是没查到的
        None => PerkName {
            en: guid.to_string(),
            zh: None,
        },
    }
}

fn ranks_view(row: Option<&PerkCostRow>) -> Vec<RankView> {
    row.map(|r| {
        r.ranks
            .iter()
            .map(|rank| RankView {
                tier: rank.tier,
                cost: rank.cost,
            })
            .collect()
    })
    .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PerkName {
    pub en: String,
    pub zh: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct RankView {
    /// 这一级在进阶阶梯上的档位（资产里是 4、5、6… 不是 1、2、3）
    pub tier: i64,
    pub cost: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerkItemView {
    pub guid: String,
    pub name: PerkName,
    pub ranks: Vec<RankView>,
    /// 买了但一级都没点也是 0；`None` 才是「没买」
    pub owned_rank: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnedPerkView {
    pub guid: String,
    pub rank: i64,
    pub name: PerkName,
    pub ranks: Vec<RankView>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerkLoadoutView {
    pub character_id: String,
    pub perk_guids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PerksView {
    pub state: String,
    pub note: String,
    pub points: PointsCheck,
    /// 已买的（含 rank）
    pub owned: Vec<OwnedPerkView>,
    /// 全部 24 条；`ownedRank` 为 `null` 表示没买
    pub all: Vec<PerkItemView>,
    /// 每个角色「当前生效」的那套（`RandomEquippedPerkLoadout`）
    pub loadouts: Vec<PerkLoadoutView>,
}

impl PerksView {
    fn with_state(state: &str, note: &str) -> Self {
        Self {
            state: state.to_string(),
            note: note.to_string(),
            points: PointsCheck {
                earned: None,
                spent: None,
                left: 0,
                verified: false,
            },
            owned: Vec::new(),
            all: Vec::new(),
            loadouts: Vec::new(),
        }
    }
}

/// 已买的按 rank 降序排在前面；没买的按「解锁档位 → 总花费 → 英文名」升序。
///
/// 界面顺序就是这里定的，前端不再自己排一遍——两个地方各排一次，迟早会不一致。
fn order_of(item: &PerkItemView) -> (u8, i64, i64, String) {
    match item.owned_rank {
        Some(rank) => (0, -rank, 0, item.name.en.clone()),
        None => {
            let first = item.ranks.first().map(|r| r.tier).unwrap_or(i64::MAX);
            let total: i64 = item.ranks.iter().map(|r| r.cost).sum();
            (1, first, total, item.name.en.clone())
        }
    }
}

/// 纯函数：给存档里的几块数据，算出天赋页要的整块视图（单测直接喂构造数据）。
pub(crate) fn view_of(
    perk_points: i32,
    claimed: &[MilestoneEntry],
    owned: &[PerkEntry],
    loadout: &crate::save::model::PerkLoadout,
) -> PerksView {
    let owned_rank: HashMap<String, i64> = owned
        .iter()
        .map(|entry| (entry.PerkID.to_lowercase(), i64::from(entry.currentRank)))
        .collect();

    let mut all: Vec<PerkItemView> = perk_costs()
        .values()
        .map(|row| {
            let guid = row.guid.to_lowercase();
            PerkItemView {
                name: name_of(&guid),
                ranks: ranks_view(Some(row)),
                owned_rank: owned_rank.get(&guid).copied(),
                guid,
            }
        })
        .collect();
    all.sort_by_key(order_of);

    let mut owned_views: Vec<OwnedPerkView> = owned
        .iter()
        .map(|entry| {
            let guid = entry.PerkID.to_lowercase();
            OwnedPerkView {
                name: name_of(&guid),
                ranks: ranks_view(perk_costs().get(&guid)),
                rank: i64::from(entry.currentRank),
                guid,
            }
        })
        .collect();
    owned_views.sort_by(|a, b| {
        b.rank
            .cmp(&a.rank)
            .then_with(|| a.name.en.cmp(&b.name.en))
    });

    let loadouts: Vec<PerkLoadoutView> = loadout
        .CharacterPerks
        .iter()
        .filter(|c| !c.characterID.is_empty() && c.characterID != ZERO_GUID)
        .map(|c| PerkLoadoutView {
            character_id: c.characterID.to_lowercase(),
            perk_guids: c.PerkIDs.iter().map(|g| g.to_lowercase()).collect(),
        })
        .collect();

    PerksView {
        state: "ok".to_string(),
        note: String::new(),
        points: points_check(perk_points, claimed, owned),
        owned: owned_views,
        all,
        loadouts,
    }
}

/// 天赋页的整块数据。只读：不写存档、不碰缓存以外的任何东西。
#[tauri::command]
pub fn get_perks(selected: Option<String>) -> ApiResult<PerksView> {
    let Some(file) = super::save::pick(selected.as_deref()) else {
        return ApiResult::ok(PerksView::with_state(
            "not_found",
            "没找到存档：这台机器上还没存过档，或者装的是别的版本（目前只认 Steam 版）",
        ));
    };

    let cache_root = crate::paths::data_root().join("cache");
    match crate::save::store::load(&file.path, &cache_root) {
        Ok((doc, _report)) => ApiResult::ok(view_of(
            doc.data.PerkPoints,
            &doc.data.Progress.Milestones,
            &doc.data.Perks.OwnedPerks,
            &doc.data.Perks.RandomEquippedPerkLoadout,
        )),
        Err(e) => ApiResult::ok(PerksView::with_state(
            e.class.code(),
            &format!("{}：{}", e.class.human(), e.detail),
        )),
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

    // -----------------------------------------------------------------
    // 整块视图：顺序、未买、角色配装、校验失败时清单照常
    // -----------------------------------------------------------------

    #[test]
    fn all_rows_are_twenty_four_and_every_one_has_a_name() {
        let view = view_of(0, &[], &[], &Default::default());
        assert_eq!(view.state, "ok");
        assert_eq!(view.all.len(), 24, "资产里一共 24 条天赋");
        assert!(view.owned.is_empty());
        for item in &view.all {
            assert!(!item.name.en.is_empty(), "{} 没有英文名", item.guid);
            assert_ne!(item.name.en, item.guid, "{} 的名字退回了 GUID", item.guid);
            assert!(item.name.zh.is_some(), "{} 没有中文名", item.guid);
            assert!(item.ranks.len() <= 4, "{} 超过四级", item.guid);
        }
        // 没买过时，全表按「解锁档位 → 总花费 → 英文名」排，第一条是档位最低的那条
        let first_tiers: Vec<i64> = view
            .all
            .iter()
            .map(|i| i.ranks.first().map(|r| r.tier).unwrap_or(i64::MAX))
            .collect();
        let mut sorted = first_tiers.clone();
        sorted.sort_unstable();
        assert_eq!(first_tiers, sorted, "未买的没有按解锁档位升序");
    }

    #[test]
    fn bought_ones_go_first_by_rank_and_the_rest_keep_their_order() {
        let view = view_of(0, &[], &[owned(VAMPIRE, 2)], &Default::default());
        assert_eq!(view.owned.len(), 1);
        assert_eq!(view.owned[0].guid, VAMPIRE);
        assert_eq!(view.owned[0].rank, 2);
        assert_eq!(view.all[0].guid, VAMPIRE, "买过的应该排第一");
        assert_eq!(view.all[0].owned_rank, Some(2));
        assert!(view.all[1..].iter().all(|i| i.owned_rank.is_none()));
        // 已买的两条之间按 rank 降序
        let view = view_of(0, &[], &[owned(VAMPIRE, 1), owned(ACTIVE_SLOT, 0)], &Default::default());
        let order: Vec<i64> = view.owned.iter().map(|o| o.rank).collect();
        assert_eq!(order, vec![1, 0]);
    }

    #[test]
    fn loadouts_keep_real_characters_and_drop_the_zero_slot() {
        let loadout = crate::save::model::PerkLoadout {
            CharacterPerks: vec![
                crate::save::model::CharacterPerks {
                    characterID: ZERO_GUID.to_string(),
                    PerkIDs: vec![VAMPIRE.to_string()],
                },
                crate::save::model::CharacterPerks {
                    characterID: "D6B3F2A24C0B4C758F0FDD5F4C1F2A11".to_string(),
                    PerkIDs: vec!["F747E1D81BBA744980E385E8610D11A6".to_string()],
                },
            ],
        };
        let view = view_of(0, &[], &[], &loadout);
        assert_eq!(view.loadouts.len(), 1, "全零槽要丢掉");
        assert_eq!(view.loadouts[0].character_id, "d6b3f2a24c0b4c758f0fdd5f4c1f2a11");
        assert_eq!(view.loadouts[0].perk_guids, vec![VAMPIRE.to_string()]);
    }

    #[test]
    fn failed_check_still_returns_the_full_list() {
        // 20 - 17 = 3，存档写 35：两个数都不给，但清单一条不少
        let view = view_of(35, &[claimed(SCOUT_LEVELUP, 4)], &[owned(VAMPIRE, 4)], &Default::default());
        assert!(!view.points.verified);
        assert_eq!(view.points.earned, None);
        assert_eq!(view.points.spent, None);
        assert_eq!(view.points.left, 35);
        assert_eq!(view.all.len(), 24);
        assert_eq!(view.owned.len(), 1);
    }
}
