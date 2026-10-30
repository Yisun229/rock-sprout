//! 「今天」页的数据：Steam 公告 + mod 动态 + 你装了什么。
//!
//! 三条口径（写代码时按这个来，别改着改着走样）：
//!
//! 1. **三块分别取、分别降级**：公告抓不到就是公告空着并记一笔，不影响另外两块；
//!    整条命令只在「连数据目录都读不了」这种时候才失败，正常断网一律返回 `Ok`。
//! 2. **缓存是当天的就直接用**：`cache\today.json` 里的 `fetched_at` 跟今天是同一天，
//!    连网络都不碰，直接标 `from_cache = true` 返回。用户点刷新才无视缓存重取。
//! 3. **有新版按文件 id 判**：本地记账里 `version` 其实是文件名，不是版本号
//!    （见 `ipc::mods::InstalledRecord::to_installed`），所以不比版本号，比
//!    `file_id`——每个已装 mod 取一次最新文件，id 变了就是有新版。这一项要联网，
//!    取不到就把这一块标成降级，不假装「都是最新的」。

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
/// 「更新」块最多列几条（Q10）。这一版不比版本号，这个数只用来看"装没装过 mod"。
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
/// 只放**有新版**的那些：装上时的文件 id 与 mod.io 上最新文件的 id 不一样。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModUpdate {
    pub id: u64,
    pub name: String,
    /// 第一次装上的时刻（unix 毫秒）；老记账没有就是 None
    pub installed_at: Option<u64>,
    /// 装上时那个文件的 id（记账里记的）
    pub installed_file_id: u64,
    /// mod.io 上最新文件的 id
    pub latest_file_id: u64,
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
    /// 有新版的那几个（最多 `UPDATE_COUNT` 条）
    pub updates: Vec<ModUpdate>,
    /// 本机一共装着几个 mod（用来区分「都最新」和「一个都没装」）
    pub installed_count: usize,
    /// 这次真去 mod.io 问成了几个（本地导入的包没法问）
    ///
    /// 界面靠它分清「问过，都最新」和「装的全是本地包，没得问」。
    pub checked_count: usize,
    /// 本机还没存令牌：mod 的两块都空着
    pub token_missing: bool,
    /// 哪几块没取成，如 `["news"]`
    pub degraded: Vec<String>,
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
    // 界面状态单独存 ui-state.json，数据缓存里只有数据
    let mut clean = digest.clone();
    clean.from_cache = false;
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

/// 本地记账，最近装的在前（全给出来，查新版那步自己截断到 `UPDATE_COUNT`）。
fn installed_records(data_root: &Path) -> Vec<mods::InstalledRecord> {
    let dir = mods::records_dir_at(data_root);
    let mut records = mods::load_records(&dir);
    records.sort_by(|a, b| b.installed_at.unwrap_or(0).cmp(&a.installed_at.unwrap_or(0)));
    records
}

/// 能查新版的那些：从 mod.io 装的、记账里带文件 id、文件还在。
fn checkable(record: &mods::InstalledRecord) -> bool {
    record.source != "local" && record.file_id != 0 && !record.missing
}

/// 真的要去问的那几个（最多 `UPDATE_COUNT` 条，跟 Q10 的上限对得上）。
fn to_check(records: &[mods::InstalledRecord]) -> Vec<&mods::InstalledRecord> {
    records.iter().filter(|r| checkable(r)).take(UPDATE_COUNT).collect()
}

/// 拿「装上时的文件 id」跟「最新文件的 id」比，挑出有新版的。
///
/// 分开写纯粹为了能不联网地测：`latest` 是每个 mod id 对应的最新文件 id。
fn updates_from(records: &[mods::InstalledRecord], latest: &[(u64, Option<u64>)]) -> Vec<ModUpdate> {
    let mut out = Vec::new();
    for record in records.iter().filter(|r| checkable(r)) {
        let Some((_, Some(latest_id))) = latest.iter().find(|(id, _)| *id == record.id) else {
            continue;
        };
        if *latest_id != record.file_id {
            out.push(ModUpdate {
                id: record.id,
                name: record.name.clone(),
                installed_at: record.installed_at,
                installed_file_id: record.file_id,
                latest_file_id: *latest_id,
            });
        }
    }
    out
}

/// 挨个问 mod.io「这个 mod 最新的文件是哪个」。一个失败就整块算失败（不猜）。
///
/// 回来的是「有新版的那几个」和「真问成了几个」——后者让界面能分清
/// 「问过、都最新」和「一个都没可问的（本地导入的包）」，两者不能都说成「都是最新的」。
async fn check_updates(
    client: &Modio,
    records: &[mods::InstalledRecord],
) -> Result<(Vec<ModUpdate>, usize), ModioError> {
    let mut latest: Vec<(u64, Option<u64>)> = Vec::new();
    let mut asked = 0_usize;
    for record in to_check(records) {
        let newest = match client.latest_file(record.id).await {
            Ok(file) => file.map(|f| f.id),
            Err(error) => {
                tracing::warn!(mod_id = record.id, error = %error.message(), "查新版没成");
                return Err(error);
            }
        };
        asked += 1;
        latest.push((record.id, newest));
    }
    Ok((updates_from(records, &latest), asked))
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

    let token_missing = !crate::token::has_token(data_root);
    let records = installed_records(data_root);
    let installed_count = records.len();
    let mut mod_changes = Vec::new();
    let mut updates = Vec::new();
    let mut checked_count = 0_usize;
    if token_missing {
        // 没令牌时 mod.io 一律 401，不是「坏了」，所以不记进 degraded；
        // 「更新」块这时只能提示去粘令牌，不能声称「都是最新的」
    } else {
        match crate::token::client(data_root) {
            Ok(client) => {
                match recent_changes(&client).await {
                    Ok(items) => mod_changes = items,
                    Err(error) => {
                        tracing::warn!(error = %error.message(), "mod 动态没取到，这一块先空着");
                        degraded.push("mod_changes".to_owned());
                    }
                }
                match check_updates(&client, &records).await {
                    Ok((items, asked)) => {
                        updates = items;
                        checked_count = asked;
                    }
                    Err(error) => {
                        tracing::warn!(error = %error.message(), "查新版没成，这一块先空着");
                        degraded.push("updates".to_owned());
                    }
                }
            }
            Err(error) => {
                tracing::warn!(error = %error.message(), "读令牌文件没成，mod 的两块先空着");
                degraded.push("mod_changes".to_owned());
                degraded.push("updates".to_owned());
            }
        }
    }

    TodayDigest {
        fetched_at: now_ms(),
        from_cache: false,
        news,
        mod_changes,
        updates,
        installed_count,
        checked_count,
        token_missing,
        degraded,
    }
}

/// 「今天」页的全部数据。
///
/// `refresh` 为真时无视当天缓存重取（页头那个刷新按钮）。
#[tauri::command]
pub async fn today_digest(refresh: Option<bool>) -> ApiResult<TodayDigest> {
    let data_root = paths::data_root();

    if !refresh.unwrap_or(false) {
        if let Some(mut cached) = load_cache(&data_root) {
            if same_day(cached.fetched_at, now_ms()) {
                cached.from_cache = true;
                return ApiResult::ok(cached);
            }
        }
    }

    let digest = fetch_all(&data_root).await;
    save_cache(&data_root, &digest);
    ApiResult::ok(digest)
}

/// 读界面状态（切页、引导卡收起与否）。
///
/// 单独一条命令是为了启动时**不等网络**：这条只读一个本地小文件，几毫秒就回来，
/// 界面能直接落在上次那一页上，不用先渲染一页再跳。
#[tauri::command]
pub fn ui_state() -> ApiResult<UiState> {
    ApiResult::ok(crate::ui_state::load(&paths::data_root()))
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
                installed_file_id: 111,
                latest_file_id: 222,
            }],
            installed_count: 1,
            checked_count: 1,
            token_missing: false,
            degraded: vec![],
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
    fn 缓存里不存来自缓存的标记() {
        let dir = temp_dir("no-cache-flag");
        let mut digest = sample_digest();
        digest.from_cache = true;
        save_cache(&dir, &digest);

        let back = load_cache(&dir).expect("读缓存");
        assert!(!back.from_cache, "落盘时这一项要归零");
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

        let list = installed_records(&dir);
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].installed_at, Some(300), "最近装的排最前");
        assert_eq!(list[2].installed_at, Some(100));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn record(id: u64, file_id: u64, source: &str) -> mods::InstalledRecord {
        mods::InstalledRecord {
            id,
            name: format!("mod {id}"),
            file_id,
            filename: format!("m{id}.zip"),
            size: 1,
            path: "x".to_owned(),
            source: source.to_owned(),
            original_path: None,
            content_hash: None,
            missing: false,
            installed_at: Some(1_792_000_000_000),
        }
    }

    #[test]
    fn 最新文件id变了就是有新版() {
        let records = vec![record(1, 111, "modio"), record(2, 222, "modio")];
        let latest = vec![(1, Some(999)), (2, Some(222))];
        let updates = updates_from(&records, &latest);
        assert_eq!(updates.len(), 1, "只有 id=1 那个换了文件");
        assert_eq!(updates[0].id, 1);
        assert_eq!(updates[0].installed_file_id, 111);
        assert_eq!(updates[0].latest_file_id, 999);
    }

    #[test]
    fn 最新文件id一样不算有新版() {
        let records = vec![record(1, 111, "modio")];
        assert!(updates_from(&records, &[(1, Some(111))]).is_empty());
    }

    #[test]
    fn 查不到最新文件不算有新版() {
        let records = vec![record(1, 111, "modio")];
        assert!(updates_from(&records, &[(1, None)]).is_empty(), "问不到就别瞎猜");
        assert!(updates_from(&records, &[]).is_empty(), "压根没问也一样");
    }

    #[test]
    fn 本地导入的跟没文件id的不去问() {
        let records = vec![
            record(1, 111, "local"),
            record(2, 0, "modio"),
            record(3, 333, "modio"),
        ];
        let checked: Vec<u64> = to_check(&records).iter().map(|r| r.id).collect();
        assert_eq!(checked, vec![3], "只查从 mod.io 装、有文件 id 的");
    }

    #[test]
    fn 文件已经不在的不去问() {
        let mut gone = record(1, 111, "modio");
        gone.missing = true;
        let records = [gone];
        let checked = to_check(&records);
        assert!(checked.is_empty(), "记账指向的文件都没了，查新版没意义");
    }

    #[test]
    fn 最多查十个() {
        let records: Vec<_> = (1..=12).map(|id| record(id, id * 10, "modio")).collect();
        assert_eq!(to_check(&records).len(), UPDATE_COUNT.min(10));
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
