//! 「今天」页的酒吧块：酒单、三层门槛（执照 / 材料 / 都够）、顶部原料数。
//!
//! 三层判定不是装饰：集团出品（三款）只花信用点；精酿逐款买执照、之后每杯花材料；
//! 八款特调共用一张执照、每杯花 1–3 个大麦球。所以「能点」要同时看执照和材料，
//! 只报其中一层会让玩家拿着不够的材料去点单。
//!
//! 还两条说清楚的：
//! - 当天轮到哪一款特调，存档里没有记录（只有 `HasUnlockedSpecial` 一个布尔），
//!   所以八款并列、不指定「今天的」；
//! - 季节酒（Oktoberfest / 万圣节）只在活动期能买，存档里没有活动状态，
//!   所以只标注、不做判定。
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::{dwarf, ApiResult};
use crate::save::model::SaveData;

/// 酒单表（`app/src/data/beers.json`）。
const BEERS_JSON: &str = include_str!("../../../src/data/beers.json");

#[derive(Debug, Deserialize)]
struct BeersFile {
    ingredients: Vec<IngredientRow>,
    beers: Vec<BeerRow>,
}

#[derive(Debug, Clone, Deserialize)]
struct IngredientRow {
    id: String,
    #[serde(default)]
    en: String,
    #[serde(default)]
    zh: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct BeerRow {
    id: String,
    group: String,
    #[serde(default)]
    en: String,
    #[serde(default)]
    zh: Option<String>,
    /// 有一款（RandoWeisser）资产里没写牌价，JSON 里是 null
    #[serde(default)]
    price: Option<i32>,
    #[serde(default)]
    rank: Option<i32>,
    #[serde(default)]
    license: Vec<CostRow>,
    #[serde(default)]
    needs: Vec<CostRow>,
}

#[derive(Debug, Clone, Deserialize)]
struct CostRow {
    id: String,
    #[serde(default)]
    count: i32,
}

/// 一项花费（信用点或某一种材料）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BarCostView {
    pub id: String,
    /// 中文名；材料表里没写中文时退回英文
    pub name: String,
    pub need: i32,
    pub have: i32,
}

/// 一杯酒。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BarRowView {
    pub id: String,
    pub group: String,
    pub name: String,
    pub en: String,
    /// 牌价（信用点）；只做参考，实际判定看 `needs`
    pub price: i32,
    /// 等级门槛；`None` 表示资产里没写
    pub rank: Option<i32>,
    /// 执照这一层过了没有
    pub licensed: bool,
    /// ok / license / short
    pub state: String,
    /// 每杯实际花费
    pub needs: Vec<BarCostView>,
    /// 差的那一项（`state == "short"` 时才有意义）
    pub short: Option<BarCostView>,
    pub note: Option<String>,
}

/// 顶部一排材料。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BarIngredientView {
    pub id: String,
    pub name: String,
    pub have: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BarView {
    /// ok / not_found / unknown_version / bad_structure
    pub state: String,
    pub note: Option<String>,
    pub credits: i32,
    /// 角色等级最高的一级（`levels.json` 的 25 级曲线）；推不出来就是 0
    pub max_level: i32,
    pub ingredients: Vec<BarIngredientView>,
    pub rows: Vec<BarRowView>,
    /// 能点几款
    pub ready: usize,
    pub total: usize,
}

impl BarView {
    fn empty(state: &str, note: Option<String>) -> Self {
        BarView {
            state: state.to_owned(),
            note,
            credits: 0,
            max_level: 0,
            ingredients: Vec::new(),
            rows: Vec::new(),
            ready: 0,
            total: 0,
        }
    }
}

/// 酒单表；解析失败返回 `None`，由调用方显式报「表坏了」，不静默当成空菜单。
fn beers() -> Option<&'static BeersFile> {
    use std::sync::OnceLock;
    static TABLE: OnceLock<Option<BeersFile>> = OnceLock::new();
    TABLE.get_or_init(|| serde_json::from_str(BEERS_JSON).ok()).as_ref()
}

fn name_of(row: &BeerRow) -> String {
    match row.zh.as_deref() {
        Some(zh) if !zh.is_empty() => zh.to_owned(),
        _ => row.en.clone(),
    }
}

/// 材料名；`credits` 用「信用点」。
fn cost_name(file: &BeersFile, id: &str) -> String {
    if id == "credits" {
        return "信用点".to_owned();
    }
    file.ingredients
        .iter()
        .find(|x| x.id == id)
        .map(|x| match x.zh.as_deref() {
            Some(zh) if !zh.is_empty() => zh.to_owned(),
            _ => x.en.clone(),
        })
        .unwrap_or_else(|| id.chars().take(8).collect())
}

fn credits_of(data: &SaveData) -> i32 {
    data.Credits
}

fn holdings(data: &SaveData) -> HashMap<String, i32> {
    data.Resources
        .OwnedResources
        .iter()
        .map(|(guid, count)| (guid.clone(), *count as i32))
        .collect()
}

fn unlocked(data: &SaveData) -> HashSet<String> {
    data.Drinks.UnlockedDrinks.iter().cloned().collect()
}

fn max_level(data: &SaveData) -> i32 {
    let thresholds = dwarf::levels();
    data.CharacterSaves
        .iter()
        .filter_map(|c| dwarf::level_of(i64::from(c.XP), &thresholds))
        .max()
        .unwrap_or(0) as i32
}

/// 逐杯判定：只吃「有多少钱、有多少料、买过哪些执照」三样，方便单测。
fn rows_of(
    credits: i32,
    have: &HashMap<String, i32>,
    owned: &HashSet<String>,
    special_ok: bool,
) -> Vec<BarRowView> {
    let Some(file) = beers() else {
        return Vec::new();
    };
    let mut rows: Vec<BarRowView> = file
        .beers
        .iter()
        .map(|row| {
            let licensed = match row.group.as_str() {
                // 集团出品：不卖执照，直接花钱
                "group" => true,
                // 特调：八款共用一张
                "special" => special_ok,
                // 季节酒：资产里有执照清单的才要买（Oktoberfest 那款没有）
                "seasonal" => row.license.is_empty() || owned.contains(&row.id),
                // 精酿：逐款买
                _ => owned.contains(&row.id),
            };

            let needs: Vec<BarCostView> = row
                .needs
                .iter()
                .map(|c| BarCostView {
                    id: c.id.clone(),
                    name: cost_name(file, &c.id),
                    need: c.count,
                    have: if c.id == "credits" {
                        credits
                    } else {
                        have.get(&c.id).copied().unwrap_or(0)
                    },
                })
                .collect();

            let short = needs.iter().find(|n| n.have < n.need).cloned();
            let state = if !licensed {
                "license"
            } else if short.is_some() {
                "short"
            } else {
                "ok"
            };

            let note = match row.group.as_str() {
                "special" => Some("八款共用一张执照；当天轮到哪一款，存档里没有记录。".to_owned()),
                "seasonal" => Some("活动期才能买；存档里没有活动状态。".to_owned()),
                _ => None,
            };

            BarRowView {
                id: row.id.clone(),
                group: row.group.clone(),
                name: name_of(row),
                en: row.en.clone(),
                price: row.price.unwrap_or(0),
                rank: row.rank,
                licensed,
                state: state.to_owned(),
                needs,
                short,
                note,
            }
        })
        .collect();

    // 组内排序（计划 §M3 第 3 条）：能点的在前，差得少的其次，要先买执照的最后。
    // 组本身也一起排，前端按固定顺序渲染组，这里只管同组内的先后。
    fn group_rank(g: &str) -> u8 {
        match g {
            "group" => 0,
            "special" => 1,
            "crafted" => 2,
            _ => 3,
        }
    }
    rows.sort_by_key(|r| {
        let state = match r.state.as_str() {
            "ok" => (0u8, 0i32),
            "short" => (1, r.short.as_ref().map(|c| c.need - c.have).unwrap_or(i32::MAX)),
            _ => (2, 0),
        };
        (group_rank(&r.group), state.0, state.1)
    });
    rows
}

fn view_of(data: &SaveData) -> BarView {
    let Some(file) = beers() else {
        return BarView::empty(
            "bad_structure",
            Some("酒单表读不出来：app/src/data/beers.json 可能坏了".to_owned()),
        );
    };
    let credits = credits_of(data);
    let have = holdings(data);
    let rows = rows_of(credits, &have, &unlocked(data), data.Drinks.HasUnlockedSpecial);

    let ingredients = file
        .ingredients
        .iter()
        .map(|i| BarIngredientView {
            id: i.id.clone(),
            name: cost_name(file, &i.id),
            have: have.get(&i.id).copied().unwrap_or(0),
        })
        .collect();

    let ready = rows.iter().filter(|r| r.state == "ok").count();
    let total = rows.len();

    BarView {
        state: "ok".to_owned(),
        note: None,
        credits,
        max_level: max_level(data),
        ingredients,
        rows,
        ready,
        total,
    }
}

/// 酒吧块：酒单 + 三层判定 + 顶部原料数。只读。
#[tauri::command]
pub fn bar_view(selected: Option<String>) -> ApiResult<BarView> {
    let Some(file) = super::save::pick(selected.as_deref()) else {
        return ApiResult::ok(BarView::empty(
            "not_found",
            Some("没找到存档：这台机器上还没存过档，或者装的是别的版本（目前只认 Steam 版）".to_owned()),
        ));
    };

    let cache_root = crate::paths::data_root().join("cache");
    match crate::save::store::load(&file.path, &cache_root) {
        Ok((doc, _)) => ApiResult::ok(view_of(&doc.data)),
        Err(e) => ApiResult::ok(BarView::empty(
            e.class.code(),
            Some(format!("{}：{}", e.class.human(), e.detail)),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> &'static BeersFile {
        beers().expect("beers.json 应该能解析")
    }

    fn stock(all: i32) -> HashMap<String, i32> {
        table()
            .ingredients
            .iter()
            .map(|i| (i.id.clone(), all))
            .collect()
    }

    fn none() -> HashMap<String, i32> {
        HashMap::new()
    }

    #[test]
    fn group_beers_only_need_credits() {
        let poor = rows_of(0, &none(), &HashSet::new(), false);
        let slammer = poor.iter().find(|r| r.group == "group").unwrap();
        assert_eq!(slammer.state, "short");
        assert_eq!(slammer.short.as_ref().unwrap().id, "credits");

        let rich = rows_of(10_000, &none(), &HashSet::new(), false);
        assert!(rich.iter().filter(|r| r.group == "group").all(|r| r.state == "ok"));
    }

    #[test]
    fn rows_are_ordered_ok_then_short_then_license_within_a_group() {
        let rows = rows_of(10_000, &none(), &HashSet::new(), true);
        let crafted: Vec<_> = rows.iter().filter(|r| r.group == "crafted").collect();
        assert!(!crafted.is_empty());
        let rank = |s: &str| match s {
            "ok" => 0,
            "short" => 1,
            _ => 2,
        };
        let mut last = 0;
        for row in &crafted {
            let here = rank(&row.state);
            assert!(here >= last, "组内顺序反了：{} 在 {} 之后", row.state, last);
            last = here;
        }
        assert!(
            crafted.iter().any(|r| r.state == "license"),
            "没执照的精酿应当排在最后，样本里有 6 款"
        );
    }

    #[test]
    fn crafted_needs_license_first_then_materials() {
        let crafted = table().beers.iter().find(|b| b.group == "crafted").cloned().unwrap();

        let no_license = rows_of(99_999, &none(), &HashSet::new(), false);
        let row = no_license.iter().find(|r| r.id == crafted.id).unwrap();
        assert_eq!(row.state, "license");
        assert!(!row.licensed);

        // 有执照但没材料：差的是材料，不是执照
        let owned: HashSet<String> = [crafted.id.clone()].into_iter().collect();
        let licensed_only = rows_of(99_999, &none(), &owned, false);
        let row = licensed_only.iter().find(|r| r.id == crafted.id).unwrap();
        assert!(row.licensed);
        assert_eq!(row.state, "short");
        assert_ne!(row.short.as_ref().unwrap().id, "credits");
    }

    #[test]
    fn special_beers_share_one_license() {
        let special = table().beers.iter().find(|b| b.group == "special").cloned().unwrap();

        let locked = rows_of(0, &stock(100), &HashSet::new(), false);
        let row = locked.iter().find(|r| r.id == special.id).unwrap();
        assert_eq!(row.state, "license");

        let open = rows_of(0, &stock(100), &HashSet::new(), true);
        let row = open.iter().find(|r| r.id == special.id).unwrap();
        assert_eq!(row.state, "ok");
    }

    #[test]
    fn seasonal_beer_without_license_list_is_buyable() {
        let okto = table()
            .beers
            .iter()
            .find(|b| b.group == "seasonal" && b.license.is_empty())
            .cloned()
            .unwrap();
        let rows = rows_of(10_000, &none(), &HashSet::new(), false);
        let row = rows.iter().find(|r| r.id == okto.id).unwrap();
        assert!(row.licensed);
        assert_eq!(row.state, "ok");
        assert!(row.note.is_some());
    }

    #[test]
    fn every_row_has_a_name_and_a_cost() {
        for row in rows_of(10_000, &stock(100), &HashSet::new(), true) {
            assert!(!row.name.is_empty(), "{} 没有名字", row.id);
            assert!(!row.needs.is_empty(), "{} 没有花费", row.id);
        }
    }

    #[test]
    fn the_menu_matches_the_table() {
        assert_eq!(table().beers.len(), 32);
        assert_eq!(table().ingredients.len(), 4);
    }
}
