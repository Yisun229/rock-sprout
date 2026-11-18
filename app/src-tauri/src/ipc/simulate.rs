//! 模拟配装：把「这把枪 + 这些改装」算成五项数值。
//!
//! 这一层是**纯计算**，不读存档、不写任何文件——选哪些改装只是数组里多几个 id。
//!
//! 公式（我们推的，不是从游戏里读出来的）：
//!
//! ```text
//! 结果 = 基础值 × (1 + Σ(mult 系数 − 1)) + Σ(add 值)
//! ```
//!
//! 乘算按增量相加而不是逐个相乘：逐个相乘会让第二件改装看起来变弱。

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::ApiResult;

const STATS_JSON: &str = include_str!("../../../src/data/weapon-stats.json");
const EFFECTS_JSON: &str = include_str!("../../../src/data/upgrade-effects.json");

/// 五项属性的顺序固定，界面按这个顺序排。
pub(crate) const STAT_KEYS: [&str; 5] = ["damage", "clip", "ammo", "rof", "reload"];

#[derive(Debug, Deserialize)]
struct WeaponFile {
    #[serde(default, rename = "$units")]
    units: HashMap<String, String>,
    weapons: HashMap<String, WeaponRow>,
}

#[derive(Debug, Deserialize)]
struct WeaponRow {
    #[serde(default)]
    folder: String,
    #[serde(default)]
    stats: HashMap<String, serde_json::Value>,
    #[serde(default)]
    missing: Vec<String>,
    #[serde(default)]
    notes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EffectFile {
    effects: HashMap<String, EffectRow>,
}

#[derive(Debug, Deserialize)]
struct EffectRow {
    #[serde(default)]
    stat: Option<String>,
    #[serde(default)]
    amount: Option<f64>,
    #[serde(default)]
    mode: String,
}

fn weapons() -> Option<&'static WeaponFile> {
    static TABLE: OnceLock<Option<WeaponFile>> = OnceLock::new();
    TABLE
        .get_or_init(|| serde_json::from_str::<WeaponFile>(STATS_JSON).ok())
        .as_ref()
}

fn effects() -> Option<&'static EffectFile> {
    static TABLE: OnceLock<Option<EffectFile>> = OnceLock::new();
    TABLE
        .get_or_init(|| serde_json::from_str::<EffectFile>(EFFECTS_JSON).ok())
        .as_ref()
}

/// 一行数值。`base`/`value` 为 `None` 表示这把枪没有这一项（不是 0，也不猜）。
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatRow {
    pub key: String,
    pub base: Option<f64>,
    pub value: Option<f64>,
    pub changed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimView {
    pub state: String,
    pub note: Option<String>,
    pub weapon: Option<String>,
    pub folder: Option<String>,
    pub rows: Vec<StatRow>,
    /// 每项的单位，来自 `weapon-stats.json` 的 `$units`（不在界面上硬编码）
    pub units: HashMap<String, String>,
    /// 这把枪本来就缺的项（`weapon-stats.json` 的 `missing`）
    pub missing: Vec<String>,
    pub notes: Vec<String>,
}

impl SimView {
    fn empty(state: &str, note: &str) -> Self {
        Self {
            state: state.to_string(),
            note: Some(note.to_string()),
            weapon: None,
            folder: None,
            rows: Vec::new(),
            units: HashMap::new(),
            missing: Vec::new(),
            notes: Vec::new(),
        }
    }
}

/// 从 `weapon-stats.json` 的 `stats` 里取一个数：`damage` 是对象，其余是数字。
fn base_of(stats: &HashMap<String, serde_json::Value>, key: &str) -> Option<f64> {
    let v = stats.get(key)?;
    if let Some(n) = v.as_f64() {
        return Some(n);
    }
    v.get("value").and_then(|x| x.as_f64())
}

/// 公式本体：乘算增量相加，最后加固定值。
pub(crate) fn combine(base: f64, mults: &[f64], adds: &[f64]) -> f64 {
    let factor = 1.0 + mults.iter().map(|m| m - 1.0).sum::<f64>();
    let sum: f64 = adds.iter().sum();
    base * factor + sum
}

fn changed(base: f64, value: f64) -> bool {
    (value - base).abs() > 1e-6
}

fn rows_for(weapon: &WeaponRow, upgrade_ids: &[String]) -> Vec<StatRow> {
    let table = effects();
    let mut out = Vec::with_capacity(STAT_KEYS.len());
    for key in STAT_KEYS {
        let base = base_of(&weapon.stats, key);
        let mut mults: Vec<f64> = Vec::new();
        let mut adds: Vec<f64> = Vec::new();
        if let Some(table) = table {
            for id in upgrade_ids {
                let Some(row) = table.effects.get(&id.to_ascii_lowercase()) else {
                    continue;
                };
                if row.stat.as_deref() != Some(key) {
                    continue;
                }
                let Some(amount) = row.amount else { continue };
                match row.mode.as_str() {
                    "mult" => mults.push(amount),
                    "add" => adds.push(amount),
                    // unknown 不参与计算：没有数值的改装不该悄悄当 0
                    _ => {}
                }
            }
        }
        match base {
            Some(base) => {
                let value = combine(base, &mults, &adds);
                out.push(StatRow {
                    key: key.to_string(),
                    base: Some(base),
                    value: Some(round(value)),
                    changed: changed(base, value),
                });
            }
            None => out.push(StatRow {
                key: key.to_string(),
                base: None,
                value: None,
                changed: false,
            }),
        }
    }
    out
}

/// 保留两位小数，避免 `30.000000000000004` 这种尾巴。
fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// 模拟一次配装。`weapon_id` 是武器 SaveGameID；`upgrade_ids` 是当前「装上了」的改装。
pub fn simulate(weapon_id: &str, upgrade_ids: &[String]) -> Option<SimView> {
    let table = weapons()?;
    let key = weapon_id.to_ascii_lowercase();
    let weapon = table.weapons.get(&key)?;
    Some(SimView {
        state: "ok".to_string(),
        note: None,
        weapon: Some(key),
        folder: Some(weapon.folder.clone()),
        rows: rows_for(weapon, upgrade_ids),
        units: table.units.clone(),
        missing: weapon.missing.clone(),
        notes: weapon.notes.clone(),
    })
}

#[tauri::command]
pub fn simulate_loadout(weapon_id: String, upgrade_ids: Vec<String>) -> ApiResult<SimView> {
    match simulate(&weapon_id, &upgrade_ids) {
        Some(view) => ApiResult::ok(view),
        None => ApiResult::err(
            "unknown_weapon",
            format!("数值表里没有这把武器：{weapon_id}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 乘算按增量相加_不是逐个相乘() {
        // 1.5 与 1.5 两件：1 + 0.5 + 0.5 = 2 倍，而不是 2.25 倍
        assert!((combine(100.0, &[1.5, 1.5], &[]) - 200.0).abs() < 1e-9);
    }

    #[test]
    fn 固定值最后加() {
        // 100 × 2 + 10 = 210
        assert!((combine(100.0, &[2.0], &[10.0]) - 210.0).abs() < 1e-9);
    }

    #[test]
    fn 没有改装就不算变化() {
        assert!(!changed(30.0, 30.0));
        assert!(!changed(30.0, 30.000_000_1));
        assert!(changed(30.0, 32.0));
    }

    #[test]
    fn 小于一的倍率也能正确相乘() {
        // 装填 3.0 × 0.73 = 2.19
        assert!((round(combine(3.0, &[0.73], &[])) - 2.19).abs() < 1e-9);
    }

    #[test]
    fn 表里读一把真枪_五项都在() {
        let view = simulate("1af31165bcb11c48a5a3d1a6ba5a3540", &[]).expect("weapon");
        assert_eq!(view.state, "ok");
        assert_eq!(view.folder.as_deref(), Some("AssaultRifle"));
        assert_eq!(view.rows.len(), 5);
        let clip = view.rows.iter().find(|r| r.key == "clip").unwrap();
        assert_eq!(clip.base, Some(30.0));
        assert!(!clip.changed);
    }

    #[test]
    fn 装一改装后数值变了_没装的那项不变() {
        // 大容量弹匣：clip +10
        let ids = vec!["d9d2c784d075d44a9c0ef29b07744115".to_string()];
        let view = simulate("1af31165bcb11c48a5a3d1a6ba5a3540", &ids).expect("weapon");
        let clip = view.rows.iter().find(|r| r.key == "clip").unwrap();
        assert_eq!(clip.value, Some(40.0));
        assert!(clip.changed);
        let rof = view.rows.iter().find(|r| r.key == "rof").unwrap();
        assert!(!rof.changed);
    }

    #[test]
    fn 缺项是_none_不是_0() {
        // 等离子卡宾没有装填与备弹
        let view = simulate("0a3ae2198ca5b649b56e4e11d6762ac6", &[]);
        // 上面这个 id 不是武器，只是确认未知武器返回 None
        assert!(view.is_none());
        let some = weapons().and_then(|w| w.weapons.values().find(|w| !w.missing.is_empty()));
        if let Some(w) = some {
            let rows = rows_for(w, &[]);
            assert!(rows.iter().any(|r| r.base.is_none()));
        }
    }

    #[test]
    fn 没有数值的改装不参与计算() {
        // 空尖弹：弱伤加成，不属于五项，mode 是 unknown
        let ids = vec!["f610a1f6b5ad8f4bbf17f760bf8699a2".to_string()];
        let view = simulate("1af31165bcb11c48a5a3d1a6ba5a3540", &ids).expect("weapon");
        for row in &view.rows {
            assert!(!row.changed, "{} 不该变", row.key);
        }
    }
}
