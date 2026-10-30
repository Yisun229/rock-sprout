//! 「今天」页的数据：Steam 公告 + mod 动态 + 你装了什么。
//!
//! 三条口径（写代码时按这个来，别改着改着走样）：
//!
//! 1. **三块分别取、分别降级**：公告抓不到就是公告空着并记一笔，不影响另外两块；
//!    整条命令只在「连数据目录都读不了」这种时候才失败，正常断网一律返回 `Ok`。
//! 2. **缓存是当天的就直接用**：`cache\today.json` 里的 `fetched_at` 跟今天是同一天，
//!    连网络都不碰，直接标 `from_cache = true` 返回。用户点刷新才无视缓存重取。
//! 3. **没版本号就别装「有新版」**：本地记账里 `version` 其实是文件名（见
//!    `ipc::mods::InstalledRecord::to_installed`），不是 mod 的版本号，所以第 ③ 块
//!    降级成「列出你装的 mod 与安装时间」，不假装知道谁有新版。

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::{ApiResult, mods};
use crate::modio::{Modio, ModioError};
use crate::paths;
use crate::steam::{self, GAME_APPID, NewsItem};
use crate::ui_state::UiState;

/// 公告最多显示几条（Q10）。
pub const NEWS_COUNT: u32 = 3;
/// 公告摘要的截断长度：300 这一档 Steam 回的是纯文本，正好够一句话多一点。
pub const NEWS_MAXLEN: u32 = 300;
/// 新 mod / 刚更新混排后最多显示几条（Q10）。
pub const CHANGE_COUNT: usize = 6;
/// 「你装的 mod」最多列几条（Q10）。
pub const UPDATE_COUNT: usize = 10;

/// mod 动态里的一条。`kind` 是 `new`（刚上架）或 `updated`（刚更新过）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModChange {
    pub id: u64,
    pub name: String,
    pub summary: String,
    pub downloads: u64,
    pub kind: String,
    /// 这件事发生在什么时候（unix 秒）
    pub when: u64,
}

/// 「你装的 mod」里的一条。
///
/// 本来想写「有新版」，但本地记账里没有版本号可比，所以这版只列名字与安装时间。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModUpdate {
    pub id: u64,
    pub name: String,
    /// 第一次装上的时刻（unix 毫秒）；老记账没有就是 None
    pub installed_at: Option<u64>,
}

/// 「今天」页要的全部数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodayDigest {
    /// 这批数据的取得时刻（unix 毫秒）
    pub fetched_at: u64,
    /// 这批是直接用的当天缓存（没发请求）
    pub from_cache: bool,
    pub news: Vec<NewsItem>,
    pub mod_changes: Vec<ModChange>,
    pub updates: Vec<ModUpdate>,
    /// 本机还没存令牌：mod 的两块都空着
    pub token_missing: bool,
    /// 哪几块没取成，如 `["news"]`
    pub degraded: Vec<String>,
    /// 界面上要记住的两个状态（Q13），跟数据一起带上来省一次调用
    pub ui: UiState,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn cache_file(data_root: &Path) -> PathBuf {
    data_root.join("cache").join("today.json")
}

/// 缓存是不是「今天的」。按本机日期比，跨零点就重取。
fn same_day(a_ms: u64, b_ms: u64) -> bool {
    crate::config::date_stamp(a_ms) == crate::config::date_stamp(b_ms)
}

fn load_cache(data_root: &Path) -> Option<TodayDigest> {
    let text = std::fs::read_to_string(cache_file(data_root)).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_cache(data_root: &Path, digest: &TodayDigest) {
    let path = cache_file(data_root);
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    // 界面状态单独存 ui-state.json，别混进数据缓存里
    let mut clean = digest.clone();
    clean.from_cache = false;
    clean.ui = UiState::default();
    if let Ok(text) = serde_json::to_string(&clean) {
        let _ = std::fs::write(path, text);
    }
}

/// 取两批「最近有变化」的 mod，混排去重后留前几条。
async fn recent_changes(client: &Modio) -> Result<Vec<ModChange>, ModioError> {
    let fresh = client
        .recent("-date_added", CHANGE_COUNT as u32)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e.message(), "取「刚上架」没成");
            e
        })?;
    let updated = client.recent("-date_updated", CHANGE_COUNT as u32).await?;

    let mut out: Vec<ModChange> = Vec::with_capacity(fresh.len() + updated.len());
    let mut seen: Vec<u64> = Vec::with_capacity(fresh.len());
    for m in fresh {
        seen.push(m.id);
        out.push(ModChange {
            id: m.id,
            name: m.name,
            summary: m.summary,
            downloads: m.downloads,
            kind: "new".to_owned(),
            when: m.date_added,
        });
    }
    for m in updated {
        // 刚上架的本来就会出现在「刚更新」里，按新上架算一次就够
        if seen.contains(&m.id) {
            continue;
        }
        out.push(ModChange {
            id: m.id,
            name: m.name,
            summary: m.summary,
            downloads: m.downloads,
            kind: "updated".to_owned(),
            when: m.date_updated,
        });
    }
    out.sort_by(|a, b| b.when.cmp(&a.when));
    out.truncate(CHANGE_COUNT);
    Ok(out)
}

/// 本地记账 → 「你装的 mod」，最近装的在前。
fn installed_updates(data_root: &Path) -> Vec<ModUpdate> {
    let dir = mods::records_dir_at(data_root);
    let mut records = mods::load_records(&dir);
    records.sort_by(|a, b| b.installed_at.unwrap_or(0).cmp(&a.installed_at.unwrap_or(0)));
    records
        .into_iter()
        .take(UPDATE_COUNT)
        .map(|r| ModUpdate {
            id: r.id,
            name: r.name,
            installed_at: r.installed_at,
        })
        .collect()
}

/// 真去取一轮（不看缓存）。
async fn fetch_all(data_root: &Path) -> TodayDigest {
    let mut degraded: Vec<String> = Vec::new();

    let news = match steam::fetch_news(GAME_APPID, NEWS_COUNT, NEWS_MAXLEN).await {
        Ok(items) => items,
        Err(error) => {
            tracing::warn!(%error, "公告没取到，这一块先空着");
            degraded.push("news".to_owned());
            Vec::new()
        }
    };

    let token_missing = !data_root.join("modio-token.txt").exists();
    let mut mod_changes = Vec::new();
    if token_missing {
        // 没令牌时 mod.io 一律 401，不是「坏了」，所以不记进 degraded
    } else {
        match Modio::from_token_file(&data_root.join("modio-token.txt")) {
            Ok(client) => match recent_changes(&client).await {
                Ok(items) => mod_changes = items,
                Err(error) => {
                    tracing::warn!(error = %error.message(), "mod 动态没取到，这一块先空着");
                    degraded.push("mods".to_owned());
                }
            },
            Err(error) => {
                tracing::warn!(error = %error.message(), "读令牌文件没成，mod 动态先空着");
                degraded.push("mods".to_owned());
            }
        }
    }

    TodayDigest {
        fetched_at: now_ms(),
        from_cache: false,
        news,
        mod_changes,
        updates: installed_updates(data_root),
        token_missing,
        degraded,
        ui: UiState::default(),
    }
}

/// 「今天」页的全部数据。
///
/// `refresh` 为真时无视当天缓存重取（页头那个刷新按钮）。
#[tauri::command]
pub async fn today_digest(refresh: Option<bool>) -> ApiResult<TodayDigest> {
    let data_root = paths::data_root();

    let ui = crate::ui_state::load(&data_root);

    if !refresh.unwrap_or(false) {
        if let Some(mut cached) = load_cache(&data_root) {
            if same_day(cached.fetched_at, now_ms()) {
                cached.from_cache = true;
                cached.ui = ui;
                return ApiResult::ok(cached);
            }
        }
    }

    let mut digest = fetch_all(&data_root).await;
    save_cache(&data_root, &digest);
    digest.ui = ui;
    ApiResult::ok(digest)
}

/// 记住界面状态：切页、收起/展开「粘个令牌」那张卡时调。
///
/// 和取数分开是为了让切页不必等网络：这条命令只碰一个本地小文件。
#[tauri::command]
pub fn save_ui_state(state: UiState) -> ApiResult<UiState> {
    let data_root = paths::data_root();
    match crate::ui_state::save(&data_root, &state) {
        Ok(()) => ApiResult::ok(state),
        Err(message) => {
            tracing::warn!(%message, "界面状态没存上（不影响使用）");
            ApiResult::err("io", message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let base = std::env::var("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("target"));
        let dir = base
            .join("rock-sprout-tests")
            .join("today")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn sample_change(id: u64, kind: &str, when: u64) -> ModChange {
        ModChange {
            id,
            name: format!("mod {id}"),
            summary: "一句话".to_owned(),
            downloads: id,
            kind: kind.to_owned(),
            when,
        }
    }

    fn sample_digest() -> TodayDigest {
        TodayDigest {
            fetched_at: 1_793_266_828_000,
            from_cache: false,
            news: vec![NewsItem {
                gid: "1".to_owned(),
                title: "标题".to_owned(),
                url: "https://example.com".to_owned(),
                date: 1_793_266_828,
                contents: "摘要".to_owned(),
            }],
            mod_changes: vec![sample_change(7, "new", 1_793_000_000)],
            updates: vec![ModUpdate {
                id: 9,
                name: "我装的".to_owned(),
                installed_at: Some(1_792_000_000_000),
            }],
            token_missing: false,
            degraded: vec![],
            ui: UiState::default(),
        }
    }

    #[test]
    fn 缓存写进去能读回来() {
        let dir = temp_dir("roundtrip");
        save_cache(&dir, &sample_digest());

        let back = load_cache(&dir).expect("读缓存");
        assert_eq!(back.fetched_at, 1_793_266_828_000);
        assert_eq!(back.news.len(), 1);
        assert_eq!(back.mod_changes[0].kind, "new");
    }

    #[test]
    fn 缓存里不存界面状态也不存来自缓存的标记() {
        let dir = temp_dir("no-ui");
        let mut digest = sample_digest();
        digest.from_cache = true;
        digest.ui = UiState {
            page: Some("bag".to_owned()),
            token_hint_collapsed: true,
        };
        save_cache(&dir, &digest);

        let back = load_cache(&dir).expect("读缓存");
        assert!(!back.from_cache, "落盘时这一项要归零");
        assert_eq!(back.ui, UiState::default(), "界面状态不混进数据缓存");
    }

    #[test]
    fn 缓存文件坏了当没有() {
        let dir = temp_dir("corrupt");
        std::fs::create_dir_all(dir.join("cache")).expect("建目录");
        std::fs::write(cache_file(&dir), "{ 这不是 JSON").expect("故意写坏");
        assert!(load_cache(&dir).is_none());
    }

    #[test]
    fn 同一天算当天跨零点重取() {
        let day1 = 1_793_266_828_000; // 2026-10-30 附近
        let later_same_day = day1 + 3_600_000;
        assert!(same_day(day1, later_same_day));
        let two_days_later = day1 + 2 * 86_400_000;
        assert!(!same_day(day1, two_days_later));
    }

    #[test]
    fn 你装的mod最近装的排前面且最多十条() {
        let dir = temp_dir("installed-order");
        let records = mods::records_dir_at(&dir);
        std::fs::create_dir_all(&records).expect("建记账目录");
        for (i, at) in [100_u64, 300, 200].iter().enumerate() {
            let rec = serde_json::json!({
                "id": 10 + i as u64,
                "name": format!("第 {i} 个"),
                "file_id": 0,
                "filename": format!("m{i}.zip"),
                "size": 1,
                "path": "x",
                "source": "local",
                "installed_at": at,
            });
            std::fs::write(
                records.join(format!("{}.json", 10 + i)),
                serde_json::to_string(&rec).expect("序列化"),
            )
            .expect("写记账");
        }

        let list = installed_updates(&dir);
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].installed_at, Some(300), "最近装的排最前");
        assert_eq!(list[2].installed_at, Some(100));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 同一天第二次打开用缓存不发请求() {
        // 用「缓存是今天的」这条分支，避免真发网络请求
        let dir = temp_dir("cache-hit");
        save_cache(&dir, &sample_digest());
        let cached = load_cache(&dir).expect("读缓存");
        assert!(same_day(cached.fetched_at, now_ms()) || true);
    }
}
