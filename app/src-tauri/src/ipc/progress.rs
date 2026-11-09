//! 「本周」页的数据：赛季进度、深潜、战役。
//!
//! 三条规矩（都是 v0.2.5 M0/M1 定下来的）：
//!
//! 1. **赛季等级不显示**。等级由游戏运行时累计的 PP 决定，存档里只有 `XP` 与
//!    领取条数，资产里也没有阈值表——见 `文档/交接-2026-11-07.md` §5.3。
//!    所以这里只给 `xp` 和 `claimed / total`。
//! 2. **绝不把带 `{...}` 的模板原文交给玩家**。模板在资产里带占位符，填不上就
//!    不给句子（`text: None`），界面只显示能确定的那部分。
//! 3. **表外 GUID 不报错、不隐藏、不编名字**：给 `unknown: true` 和原始 id，
//!    由界面显示前 8 位并可复制。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::ApiResult;
use crate::save::model::{ChallengeEntry, DeepDiveItem, ProgressSave};

const SEASON_NAMES: &str = include_str!("../../../src/data/season-names.json");
const CHALLENGE_NAMES: &str = include_str!("../../../src/data/season-challenge-names.json");
const CAMPAIGN_NAMES: &str = include_str!("../../../src/data/campaign-names.json");
const SEASON_REWARDS: &str = include_str!("../../../src/data/season-rewards.json");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamePair {
    pub zh: String,
    pub en: String,
}

#[derive(Deserialize)]
struct Items<T> {
    items: Vec<T>,
}

#[derive(Debug, Clone, Deserialize)]
struct NamedRow {
    #[serde(default)]
    #[allow(dead_code)]
    id: String,
    #[serde(default)]
    #[allow(dead_code)]
    asset: String,
    en: String,
    zh: String,
}

#[derive(Deserialize)]
struct ChallengeRow {
    id: String,
    #[allow(dead_code)]
    asset: String,
    en: String,
    zh: String,
    #[serde(default)]
    stats: Vec<NamedRow>,
}

#[derive(Deserialize)]
struct CampaignRow {
    id: String,
    #[allow(dead_code)]
    asset: String,
    en: String,
    zh: String,
    kind: String,
    #[allow(dead_code)]
    source: String,
}

#[derive(Deserialize)]
struct RewardsFile {
    seasons: Vec<SeasonRewardsRow>,
}

#[derive(Deserialize)]
struct SeasonRewardsRow {
    guid: String,
    #[allow(dead_code)]
    asset: String,
    #[allow(dead_code)]
    title: String,
    #[serde(rename = "titleEn")]
    #[allow(dead_code)]
    title_en: String,
    #[serde(default)]
    levels: Vec<RewardRow>,
}

#[derive(Deserialize)]
struct RewardRow {
    #[allow(dead_code)]
    level: i32,
}

fn season_table() -> &'static HashMap<String, NamedRow> {
    static TABLE: std::sync::OnceLock<HashMap<String, NamedRow>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str::<Items<NamedRow>>(SEASON_NAMES)
            .map(|f| {
                f.items
                    .into_iter()
                    .map(|row| (row.id.to_lowercase(), row))
                    .collect()
            })
            .unwrap_or_default()
    })
}

fn challenge_table() -> &'static HashMap<String, ChallengeRow> {
    static TABLE: std::sync::OnceLock<HashMap<String, ChallengeRow>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str::<Items<ChallengeRow>>(CHALLENGE_NAMES)
            .map(|f| {
                f.items
                    .into_iter()
                    .map(|row| (row.id.to_lowercase(), row))
                    .collect()
            })
            .unwrap_or_default()
    })
}

fn campaign_table() -> &'static HashMap<String, CampaignRow> {
    static TABLE: std::sync::OnceLock<HashMap<String, CampaignRow>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str::<Items<CampaignRow>>(CAMPAIGN_NAMES)
            .map(|f| {
                f.items
                    .into_iter()
                    .map(|row| (row.id.to_lowercase(), row))
                    .collect()
            })
            .unwrap_or_default()
    })
}

fn reward_total(guid: &str) -> usize {
    static TABLE: std::sync::OnceLock<HashMap<String, usize>> = std::sync::OnceLock::new();
    TABLE
        .get_or_init(|| {
            serde_json::from_str::<RewardsFile>(SEASON_REWARDS)
                .map(|f| {
                    f.seasons
                        .into_iter()
                        .map(|row| (row.guid.to_lowercase(), row.levels.len()))
                        .collect()
                })
                .unwrap_or_default()
        })
        .get(&guid.to_lowercase())
        .copied()
        .unwrap_or_default()
}

/// 把模板里的占位符换成具体值。
///
/// 返回 `None` 的两种情况：模板要的短名缺失、或替换后还剩占位符。
/// 无论哪种，**都不会**把半句或带花括号的原文交给界面。
fn fill(template: &str, wins: i32, stats: &[NamedRow], chinese: bool) -> Option<String> {
    let mut out = template.replace("{wins}", &wins.to_string());
    for (i, stat) in stats.iter().enumerate().take(2) {
        let key = format!("{{stat{}name}}", i + 1);
        if !out.contains(&key) {
            continue;
        }
        let name = if chinese { &stat.zh } else { &stat.en };
        if name.is_empty() {
            return None;
        }
        out = out.replace(&key, name);
    }
    if out.contains('{') || out.contains('}') {
        None
    } else {
        Some(out)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChallengeView {
    pub id: String,
    pub text: Option<String>,
    pub text_en: Option<String>,
    pub wins: i32,
    pub total: i32,
    pub done: bool,
    pub xp: i32,
    pub tokens: i32,
}

fn challenge_view(entry: &ChallengeEntry) -> ChallengeView {
    let id = entry.ChallengeGuid.to_lowercase();
    let row = challenge_table().get(&id);
    // `StatIndices` 是下标，进的是这条挑战自己的 `TrackedStats` 顺序：
    // 只有前两个下标会被 `{stat1name}` / `{stat2name}` 用到。
    let picked: Vec<NamedRow> = row
        .map(|row| {
            entry
                .StatIndices
                .iter()
                .filter_map(|i| usize::try_from(*i).ok().and_then(|i| row.stats.get(i)).cloned())
                .collect()
        })
        .unwrap_or_default();
    let (text, text_en) = match row {
        Some(row) => (
            fill(&row.zh, entry.CurrentWins, &picked, true),
            fill(&row.en, entry.CurrentWins, &picked, false),
        ),
        None => (None, None),
    };
    ChallengeView {
        id,
        text,
        text_en,
        wins: entry.CurrentWins,
        total: entry.NumberOfWins,
        done: entry.NumberOfWins > 0 && entry.CurrentWins >= entry.NumberOfWins,
        xp: entry.XPCompleteGain,
        tokens: entry.TokenGain,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonView {
    pub id: String,
    pub name: NamePair,
    pub xp: i32,
    pub claimed: usize,
    pub total: usize,
    pub tokens: i32,
    pub nodes_bought: i32,
    pub all_claimed: bool,
    pub challenges: Vec<ChallengeView>,
}

fn season_view(entry: &crate::save::model::SeasonEntry) -> SeasonView {
    let id = entry.SeasonID.to_lowercase();
    let name = season_table()
        .get(&id)
        .map(|row| NamePair {
            zh: row.zh.clone(),
            en: row.en.clone(),
        })
        .unwrap_or_else(|| NamePair {
            zh: "未知赛季".to_string(),
            en: "Unknown season".to_string(),
        });
    SeasonView {
        name,
        total: reward_total(&id),
        claimed: entry.Rewards.len(),
        xp: entry.XP,
        tokens: entry.Tokens,
        nodes_bought: entry.NodesBought,
        all_claimed: entry.HasClaimedAllRewards,
        challenges: entry.ActiveChallenges.iter().map(challenge_view).collect(),
        id,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiveView {
    pub seed: i32,
    pub completed: bool,
    pub progress: i32,
    pub rewards: usize,
    pub best_time: Option<i32>,
}

fn dive_view(entry: &DeepDiveItem) -> DiveView {
    DiveView {
        seed: entry.Seed,
        completed: entry.HasEverCompleted,
        progress: entry.Progress,
        rewards: entry.Rewards,
        best_time: entry.BestTime,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeepDivesView {
    pub normal: DiveView,
    pub elite: DiveView,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CampaignView {
    pub id: String,
    pub name: Option<NamePair>,
    pub kind: String,
    pub retired: bool,
    pub unknown: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CampaignsView {
    pub completed: Vec<CampaignView>,
    pub unknown: usize,
    pub retired: usize,
}

/// 「本周」页的整块数据。只读：不写存档、不碰游戏目录。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressView {
    pub state: String,
    pub note: String,
    pub seasons: Vec<SeasonView>,
    pub deep_dives: DeepDivesView,
    pub campaigns: CampaignsView,
}

impl ProgressView {
    fn with_state(state: &str, note: &str) -> Self {
        Self {
            state: state.to_string(),
            note: note.to_string(),
            seasons: Vec::new(),
            deep_dives: DeepDivesView {
                normal: dive_view(&DeepDiveItem {
                    Seed: 0,
                    HasEverCompleted: false,
                    Progress: 0,
                    Rewards: 0,
                    BestTime: None,
                }),
                elite: dive_view(&DeepDiveItem {
                    Seed: 0,
                    HasEverCompleted: false,
                    Progress: 0,
                    Rewards: 0,
                    BestTime: None,
                }),
            },
            campaigns: CampaignsView {
                completed: Vec::new(),
                unknown: 0,
                retired: 0,
            },
        }
    }
}

fn view_of(progress: &ProgressSave) -> ProgressView {
    let completed: Vec<CampaignView> = progress
        .CampaignSave
        .CompletedCampaigns
        .iter()
        .map(|guid| {
            let id = guid.to_lowercase();
            match campaign_table().get(&id) {
                Some(row) => CampaignView {
                    id,
                    name: Some(NamePair {
                        zh: row.zh.clone(),
                        en: row.en.clone(),
                    }),
                    kind: row.kind.clone(),
                    retired: row.kind == "retired",
                    unknown: false,
                },
                None => CampaignView {
                    id,
                    name: None,
                    kind: "unknown".to_string(),
                    retired: false,
                    unknown: true,
                },
            }
        })
        .collect();

    ProgressView {
        state: "ok".to_string(),
        note: String::new(),
        seasons: progress.Seasons.iter().map(season_view).collect(),
        deep_dives: DeepDivesView {
            normal: dive_view(&progress.DeepDiveSave.NormalSave),
            elite: dive_view(&progress.DeepDiveSave.EliteSave),
        },
        campaigns: CampaignsView {
            unknown: completed.iter().filter(|c| c.unknown).count(),
            retired: completed.iter().filter(|c| c.retired).count(),
            completed,
        },
    }
}

/// 「本周」页的数据。只读：不写存档、不碰游戏目录。
#[tauri::command]
pub fn progress_view(selected: Option<String>) -> ApiResult<ProgressView> {
    let Some(file) = super::save::pick(selected.as_deref()) else {
        return ApiResult::ok(ProgressView::with_state(
            "not_found",
            "没找到存档：这台机器上还没存过档，或者装的是别的版本（目前只认 Steam 版）",
        ));
    };

    let cache_root = crate::paths::data_root().join("cache");
    match crate::save::store::load(&file.path, &cache_root) {
        Ok((doc, _report)) => ApiResult::ok(view_of(&doc.data.Progress)),
        Err(e) => ApiResult::ok(ProgressView::with_state(
            e.class.code(),
            &format!("{}：{}", e.class.human(), e.detail),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::model::{CampaignEntry, DeepDiveEntry};

    fn progress_with(seasons: Vec<crate::save::model::SeasonEntry>, campaigns: Vec<String>) -> ProgressSave {
        ProgressSave {
            Milestones: Vec::new(),
            Seasons: seasons,
            CampaignSave: CampaignEntry {
                CompletedCampaigns: campaigns,
                ActiveCampaignWeek: 0,
            },
            DeepDiveSave: DeepDiveEntry {
                NormalSave: DeepDiveItem {
                    Seed: 1,
                    HasEverCompleted: true,
                    Progress: 0,
                    Rewards: 0,
                    BestTime: None,
                },
                EliteSave: DeepDiveItem {
                    Seed: 1,
                    HasEverCompleted: false,
                    Progress: 2,
                    Rewards: 2,
                    BestTime: None,
                },
            },
            UnLockedMissions: Vec::new(),
            UnLockedPlanetZones: Vec::new(),
        }
    }

    fn season(guid: &str, xp: i32, claims: usize, challenges: Vec<ChallengeEntry>) -> crate::save::model::SeasonEntry {
        crate::save::model::SeasonEntry {
            SeasonID: guid.to_string(),
            XP: xp,
            Rewards: vec![
                crate::save::model::RewardClaim {
                    normal: true,
                    special: false,
                };
                claims
            ],
            NodesBought: 0,
            ActiveChallenges: challenges,
            Tokens: 0,
            HasClaimedAllRewards: false,
            bSeasonCompletedAnnounced: false,
        }
    }

    #[test]
    fn empty_progress_still_renders() {
        let view = view_of(&progress_with(Vec::new(), Vec::new()));
        assert_eq!(view.state, "ok");
        assert!(view.seasons.is_empty());
        assert!(view.campaigns.completed.is_empty());
        assert_eq!(view.deep_dives.elite.progress, 2);
        assert!(view.deep_dives.normal.best_time.is_none());
    }

    #[test]
    fn seasons_use_the_table_total_and_never_a_level() {
        let view = view_of(&progress_with(
            vec![season("d8810f6c76d374419ae6a18ef5b3ba26", 247075, 70, Vec::new())],
            Vec::new(),
        ));
        let s = &view.seasons[0];
        assert_eq!(s.name.zh, "生物危害");
        assert_eq!(s.total, 100);
        assert_eq!(s.claimed, 70);
        assert_eq!(s.xp, 247075);
    }

    #[test]
    fn unknown_campaign_keeps_its_id_and_gets_no_name() {
        let view = view_of(&progress_with(
            Vec::new(),
            vec!["0a90846b9fc42843b6c2903b9bb6fded".to_string()],
        ));
        let c = &view.campaigns.completed[0];
        assert!(c.unknown);
        assert!(c.name.is_none());
        assert_eq!(c.id, "0a90846b9fc42843b6c2903b9bb6fded");
        assert_eq!(view.campaigns.unknown, 1);
    }

    #[test]
    fn known_retired_campaign_is_marked() {
        let view = view_of(&progress_with(
            Vec::new(),
            vec!["1fad62fb72da544abaeaffb22393ce81".to_string()],
        ));
        let c = &view.campaigns.completed[0];
        assert!(!c.unknown);
        assert!(c.retired);
        assert_eq!(view.campaigns.retired, 1);
    }

    #[test]
    fn challenge_text_never_leaks_placeholders() {
        let row = challenge_table()
            .get("ab27a0887d2a434e91f52710f2e9837c")
            .expect("挑战表里应当有 Biomes");
        let entry = ChallengeEntry {
            ChallengeGuid: "ab27a0887d2a434e91f52710f2e9837c".to_string(),
            StatIndices: vec![1, 3],
            CurrentWins: 0,
            NumberOfWins: 5,
            XPCompleteGain: 3750,
            TokenGain: 0,
        };
        let view = challenge_view(&entry);
        let zh = view.text.expect("短名齐全时应当填得出来");
        assert_eq!(zh, "完成 0 次“蔚蓝花甸”或“霉菌沼泽”区域任务");
        assert!(!zh.contains('{'));
        assert!(!view.text_en.unwrap().contains('{'));
        assert!(!view.done);
        assert_eq!(row.stats.len(), 11);
    }

    #[test]
    fn challenge_with_zero_target_is_not_done_and_does_not_divide() {
        let entry = ChallengeEntry {
            ChallengeGuid: "ab27a0887d2a434e91f52710f2e9837c".to_string(),
            StatIndices: vec![0, 1],
            CurrentWins: 0,
            NumberOfWins: 0,
            XPCompleteGain: 0,
            TokenGain: 0,
        };
        let view = challenge_view(&entry);
        assert!(!view.done);
        assert_eq!(view.total, 0);
        assert!(!view.text.unwrap().contains('{'));
    }

    #[test]
    fn missing_stat_name_falls_back_to_no_sentence() {
        let template = "完成 {wins} 次“{stat1name}”或“{stat2name}”区域任务";
        let stats = vec![
            NamedRow {
                id: "a".to_string(),
                asset: "MS_Completed_Salt".to_string(),
                en: "Salt Pits".to_string(),
                zh: String::new(),
            },
            NamedRow {
                id: "b".to_string(),
                asset: "MS_Completed_AzureWeald".to_string(),
                en: "Azure Weald".to_string(),
                zh: "蔚蓝花甸".to_string(),
            },
        ];
        assert!(fill(template, 3, &stats, true).is_none());
        assert!(fill(template, 3, &stats, false).is_some());
    }
}
