//! 好友页的命令层：报状态、存 key、拉列表。
//!
//! 这里只做「读设置 → 查缓存 → 打接口 → 写缓存」的编排，
//! 具体解析与并发在 [`crate::friends`]。

use serde::Serialize;

use super::ApiResult;
use crate::paths;
use crate::settings;
use crate::{friends, steam_key};

/// 设置页「Steam」卡片要显示的一行状态。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamStatus {
    /// 本机有没有存过 key（只报「有没有」，不把 key 回传前端）
    pub has_key: bool,
    /// 认出来的 SteamID64；没有就是空串
    pub steam_id: String,
    /// `client` = 从本机 Steam 读到的；`manual` = 用户自己填的；`none` = 还没有
    pub source: String,
}

/// 一行好友。`owns_game` 为 null = 看不到对方库。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendsView {
    /// `ok` / `no_key` / `no_id` / `bad_key` / `offline`
    pub state: String,
    /// 给界面直接显示的一句话；`ok` 时是空串
    pub note: String,
    pub fetched_at: Option<u64>,
    pub from_cache: bool,
    /// 看得见的（查过库的），已按时长降序
    pub visible: Vec<friends::FriendRow>,
    /// 看不见的（资料不公开），只给昵称头像
    pub hidden: Vec<friends::FriendRow>,
    pub checked: u32,
    pub total: u32,
}

impl FriendsView {
    fn empty(state: &str, note: impl Into<String>) -> Self {
        Self {
            state: state.to_owned(),
            note: note.into(),
            fetched_at: None,
            from_cache: false,
            visible: Vec::new(),
            hidden: Vec::new(),
            checked: 0,
            total: 0,
        }
    }
}

/// 当前 key / ID 的状态。
pub fn status_now() -> SteamStatus {
    let root = paths::data_root();
    let has_key = steam_key::has(&root);
    let saved = settings::load_at(&root);
    if !saved.steam_id.is_empty() {
        return SteamStatus {
            has_key,
            steam_id: saved.steam_id,
            source: if saved.steam_id_source == "client" { "client" } else { "manual" }.to_owned(),
        };
    }
    match friends::my_steam_id() {
        Some(id) => {
            remember_id(&id);
            SteamStatus {
                has_key,
                steam_id: id,
                source: "client".to_owned(),
            }
        }
        None => SteamStatus {
            has_key,
            steam_id: String::new(),
            source: "none".to_owned(),
        },
    }
}

/// 把从本机 Steam 读到的 ID 记下来，下次不用再翻注册表。
fn remember_id(id: &str) {
    let root = paths::data_root();
    let mut s = settings::load_at(&root);
    if s.steam_id == id {
        return;
    }
    s.steam_id = id.to_owned();
    s.steam_id_source = "client".to_owned();
    if let Err(e) = settings::save_at(&root, &s) {
        tracing::warn!(%e, "记住 SteamID 失败");
    }
}

#[tauri::command]
pub async fn steam_status() -> ApiResult<SteamStatus> {
    ApiResult::ok(status_now())
}

/// 存 key / 改 SteamID。两者都可单独给；传空串表示清掉。
#[tauri::command]
pub async fn set_steam(key: Option<String>, id: Option<String>) -> ApiResult<SteamStatus> {
    let root = paths::data_root();

    if let Some(key) = key {
        let key = key.trim();
        if key.is_empty() {
            steam_key::clear(&root);
        } else if let Err(e) = steam_key::save(&root, key) {
            return ApiResult::err("bad_key", e);
        }
    }

    if let Some(id) = id {
        let id = settings::normalize_steam_id(&id);
        if id.is_empty() && !id.trim().is_empty() {
            return ApiResult::err("bad_id", "SteamID 是 17 位数字，粘的时候别带别的字符");
        }
        let mut s = settings::load_at(&root);
        s.steam_id = id;
        s.steam_id_source = if s.steam_id.is_empty() { String::new() } else { "manual".to_owned() };
        if let Err(e) = settings::save_at(&root, &s) {
            return ApiResult::err("io", format!("存设置失败：{e}"));
        }
    }

    ApiResult::ok(status_now())
}

/// 拉一遍好友。`force` = 忽略 10 分钟缓存。
#[tauri::command]
pub async fn get_friends(
    app: tauri::AppHandle,
    force: bool,
) -> ApiResult<FriendsView> {
    use tauri::Emitter;

    let root = paths::data_root();
    let Some(key) = steam_key::load(&root) else {
        return ApiResult::ok(FriendsView::empty(
            "no_key",
            "还没配 Steam Web API key。这页要读 Steam 的好友列表，没有 key 读不了。",
        ));
    };

    let status = status_now();
    if status.steam_id.is_empty() {
        return ApiResult::ok(FriendsView::empty(
            "no_id",
            "没自动认出你的 Steam 账号。贴一下你的 Steam 主页链接就行。",
        ));
    }
    let steam_id = status.steam_id;
    let cached = friends::read_cache_for(&root, &steam_id);

    if !force {
        if let Some(cache) = cached.as_ref() {
            if friends::is_fresh(cache, now_secs()) {
                return ApiResult::ok(view_from_cache(cache));
            }
        }
    }

    let steam = match friends::Steam::new() {
        Ok(s) => s,
        Err(e) => return offline_or_error(e, cached.as_ref()),
    };

    let mut last = (0u32, 0u32);
    let mut progress = |checked: u32, total: u32| {
        last = (checked, total);
        let _ = app.emit(
            "friends-progress",
            serde_json::json!({ "checked": checked, "total": total }),
        );
    };

    match friends::fetch_all(&steam, &steam_id, &key, &mut progress).await {
        Ok(rows) => {
            let (visible, hidden) = split_rows(rows);
            let cache = friends::FriendsCache {
                steam_id: steam_id.clone(),
                fetched_at: now_secs(),
                friends: visible.iter().chain(hidden.iter()).cloned().collect(),
            };
            if let Err(e) = friends::write_cache(&root, &cache) {
                tracing::warn!(%e, "好友缓存写失败");
            }
            ApiResult::ok(FriendsView {
                state: "ok".to_owned(),
                note: String::new(),
                fetched_at: Some(cache.fetched_at),
                from_cache: false,
                visible,
                hidden,
                checked: last.0,
                total: last.1,
            })
        }
        Err(e) => offline_or_error(e, cached.as_ref()),
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn split_rows(rows: Vec<friends::FriendRow>) -> (Vec<friends::FriendRow>, Vec<friends::FriendRow>) {
    let mut visible = Vec::new();
    let mut hidden = Vec::new();
    for row in rows {
        if row.owns_game.is_some() {
            visible.push(row);
        } else {
            hidden.push(row);
        }
    }
    friends::sort_rows(&mut visible);
    hidden.sort_by(|a, b| a.name.cmp(&b.name));
    (visible, hidden)
}

fn view_from_cache(cache: &friends::FriendsCache) -> FriendsView {
    let (visible, hidden) = split_rows(cache.friends.clone());
    FriendsView {
        state: "ok".to_owned(),
        note: String::new(),
        fetched_at: Some(cache.fetched_at),
        from_cache: true,
        visible,
        hidden,
        checked: 0,
        total: 0,
    }
}

/// 失败时：有缓存就给缓存 + 一句「没连上」，没缓存就整页说清是哪一类错。
fn offline_or_error(err: friends::FriendError, cache: Option<&friends::FriendsCache>) -> ApiResult<FriendsView> {
    match err {
        friends::FriendError::BadKey => {
            let mut view = FriendsView::empty("bad_key", err.message());
            view.fetched_at = cache.map(|c| c.fetched_at);
            ApiResult::ok(view)
        }
        other => match cache {
            Some(cache) => {
                let mut view = view_from_cache(cache);
                view.state = "offline".to_owned();
                let _ = other;
                view.note = "下面是上次刷新的结果".to_owned();
                ApiResult::ok(view)
            }
            None => ApiResult::ok(FriendsView::empty("offline", other.message())),
        },
    }
}
