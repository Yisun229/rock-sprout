//! Steam 好友：谁在玩深岩银河、玩了多少小时。
//!
//! 全部走 Steam Web API，只读；不向 Steam 写任何东西。
//!
//! 三个定死的口径（都在计划 §2 里写过，代码里也标着）：
//!
//! 1. **五个接口全都要 key**。`GetSchemaForGame` 也报 `Required parameter 'key' is missing`，
//!    所以没有「没 key 时降级」这回事——没 key 就是整页引导卡。
//! 2. **`GetOwnedGames` 对方不公开时回 `200 + {"response":{}}`**。空对象等于「看不见」，
//!    不是「零游戏」。把这两件事混起来，好友列表上就会出现一排「没玩过任何游戏」的人。
//! 3. **先筛可见性再查库**：`GetPlayerSummaries` 一次最多 100 个，先分批拿
//!    `communityvisibilitystate`，只对 `== 3`（公开）的人发 `GetOwnedGames`。
//!    反过来的话，好友多的人会白跑几十次请求。

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 《深岩银河》在 Steam 上的 appid。
const GAME_APPID: u32 = 548_430;

/// `GetPlayerSummaries` 一次最多 100 个。
const SUMMARY_BATCH: usize = 100;

/// 同时发几个 `GetOwnedGames`。好友上百时全靠它压住总时长。
pub const CONCURRENCY: usize = 5;

/// 缓存有效期：10 分钟内进页面不再发请求。
pub const CACHE_TTL_SECS: u64 = 600;

const TIMEOUT_SECS: u64 = 15;

/// Steam 报错时我们的分类。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FriendError {
    /// key 不被接受（401/403，以及带 key 却仍被要求 key 的 400）
    BadKey,
    /// 连不上、超时、回了个认不出的东西
    Network(String),
    /// 好友列表拿不到（多半是对方资料不公开）
    NoFriendList,
}

impl FriendError {
    pub fn message(&self) -> String {
        match self {
            FriendError::BadKey => "这把 key 不被 Steam 接受，重新申请一把再粘进来".to_owned(),
            FriendError::Network(detail) => format!("连不上 Steam（{detail}）"),
            FriendError::NoFriendList => {
                "拿不到好友列表：你的 Steam 资料可能没公开".to_owned()
            }
        }
    }
}

/// 一位好友。`owns_game` / `playtime_min` 为 `None` 表示「看不到」。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendRow {
    pub steam_id: String,
    pub name: String,
    pub avatar: String,
    /// `communityvisibilitystate`：1 私密、2 仅好友、3 公开
    pub visibility: u8,
    pub owns_game: Option<bool>,
    pub playtime_min: Option<u64>,
}

impl FriendRow {
    /// 从摘要里起一行，游戏数据先留空。
    fn from_summary(s: &Summary) -> Self {
        Self {
            steam_id: s.steam_id.clone(),
            name: s.name.clone(),
            avatar: s.avatar.clone(),
            visibility: s.visibility,
            owns_game: None,
            playtime_min: None,
        }
    }
}

/// `GetPlayerSummaries` 里我们关心的几个字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub steam_id: String,
    pub name: String,
    pub avatar: String,
    pub visibility: u8,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------- 我的 SteamID

/// 本机登录的 SteamID64。读不到返回 `None`（不报错，界面走「贴主页链接」那一档）。
pub fn my_steam_id() -> Option<String> {
    registry_active_user().or_else(loginusers_most_recent)
}

/// 注册表里的 `ActiveUser` 是 SteamID64 的低 32 位。
fn registry_active_user() -> Option<String> {
    #[cfg(windows)]
    {
        use crate::webview2::win::{HKEY_CURRENT_USER, read_dword};
        let n = read_dword(
            HKEY_CURRENT_USER,
            "Software\\Valve\\Steam\\ActiveProcess",
            "ActiveUser",
        )?;
        if n == 0 {
            return None;
        }
        return Some(id_from_account(n));
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// 账号 ID（低 32 位）拼成 SteamID64。
pub fn id_from_account(account: u32) -> String {
    format!("{}", 76_561_197_960_265_728u64 + u64::from(account))
}

/// `loginusers.vdf` 里 `MostRecent 1` 那一段的键名。
fn loginusers_most_recent() -> Option<String> {
    for root in steam_dirs() {
        if let Ok(text) = std::fs::read_to_string(root.join("config\\loginusers.vdf")) {
            if let Some(id) = parse_loginusers(&text) {
                return Some(id);
            }
        }
    }
    None
}

/// Steam 装在哪：先问注册表，再看几个常见位置。
fn steam_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut push = |p: PathBuf| {
        if p.is_dir() && !dirs.iter().any(|d| d == &p) {
            dirs.push(p);
        }
    };

    #[cfg(windows)]
    {
        use crate::webview2::win::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, read_string};
        for sub in ["Software\\Valve\\Steam", "Software\\WOW6432Node\\Valve\\Steam"] {
            if let Some(p) = read_string(HKEY_CURRENT_USER, sub, "SteamPath") {
                push(PathBuf::from(p));
            }
            if let Some(p) = read_string(HKEY_LOCAL_MACHINE, sub, "InstallPath") {
                push(PathBuf::from(p));
            }
        }
    }

    for p in [
        "C:\\Program Files (x86)\\Steam",
        "C:\\Program Files\\Steam",
        "D:\\Steam",
        "D:\\steam",
        "E:\\Steam",
        "E:\\steam",
    ] {
        push(PathBuf::from(p));
    }
    dirs
}

/// 从 `loginusers.vdf` 正文里找 `MostRecent` 为 1 的那个账号。
///
/// VDF 的结构是 `"users" { "7656…" { "AccountName" "x" "MostRecent" "1" } }`，
/// 只为这一处引一个 VDF 解析库不划算，按引号切词 + 大括号深度扫一遍就够了。
pub fn parse_loginusers(text: &str) -> Option<String> {
    let mut depth = 0usize;
    let mut current: Option<(String, usize)> = None;
    let mut expecting_value_at: Option<usize> = None;

    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if let Some((_, at)) = current.clone() {
                    if depth <= at {
                        current = None;
                    }
                }
            }
            '"' => {
                let mut token = String::new();
                while let Some(&n) = chars.peek() {
                    chars.next();
                    if n == '"' {
                        break;
                    }
                    if n == '\\' {
                        if let Some(&esc) = chars.peek() {
                            chars.next();
                            token.push(esc);
                            continue;
                        }
                    }
                    token.push(n);
                }
                if let Some(at) = expecting_value_at.take() {
                    if token.trim() == "1" {
                        if let Some((id, id_at)) = current.clone() {
                            if id_at + 1 == at {
                                return Some(id);
                            }
                        }
                    }
                    continue;
                }
                if token == "MostRecent" {
                    expecting_value_at = Some(depth);
                    continue;
                }
                if token.len() == 17 && token.chars().all(|c| c.is_ascii_digit()) {
                    current = Some((token, depth));
                }
            }
            _ => {}
        }
    }
    None
}

// ---------------------------------------------------------------- 解析（纯函数，好测）

/// `GetFriendList` 的回包 → steamid 列表。
///
/// `friendslist` 不在 = 拿不到（资料没公开）；`friendslist` 是空对象 = 真的没好友。
pub fn parse_friend_ids(v: &Value) -> Result<Vec<String>, FriendError> {
    let list = v.get("friendslist").ok_or(FriendError::NoFriendList)?;
    let friends = list.get("friends").and_then(|f| f.as_array());
    Ok(friends
        .map(|arr| {
            arr.iter()
                .filter_map(|f| f.get("steamid").and_then(|s| s.as_str()))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default())
}

/// `GetPlayerSummaries` 的回包 → 摘要列表。
pub fn parse_summaries(v: &Value) -> Vec<Summary> {
    let Some(players) = v
        .get("response")
        .and_then(|r| r.get("players"))
        .and_then(|p| p.as_array())
    else {
        return Vec::new();
    };
    players
        .iter()
        .filter_map(|p| {
            Some(Summary {
                steam_id: p.get("steamid")?.as_str()?.to_owned(),
                name: p
                    .get("personaname")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_owned(),
                avatar: p
                    .get("avatarfull")
                    .or_else(|| p.get("avatarmedium"))
                    .and_then(|a| a.as_str())
                    .unwrap_or("")
                    .to_owned(),
                visibility: p
                    .get("communityvisibilitystate")
                    .and_then(|c| c.as_u64())
                    .unwrap_or(0) as u8,
            })
        })
        .collect()
}

/// `GetOwnedGames` 的回包 → `None` 表示看不见，`Some((是否拥有, 分钟数))` 表示看得见。
///
/// 关键：**空对象 `{"response":{}}` 是「不公开」，不是「零游戏」**。
pub fn parse_owned(v: &Value) -> Option<(bool, u64)> {
    let response = v.get("response")?;
    let obj = response.as_object()?;
    if obj.is_empty() {
        return None;
    }
    let games = response.get("games").and_then(|g| g.as_array());
    if let Some(games) = games {
        let hit = games.iter().find(|g| {
            g.get("appid").and_then(|a| a.as_u64()) == Some(u64::from(GAME_APPID))
        });
        return Some(match hit {
            Some(game) => (
                true,
                game.get("playtime_forever")
                    .and_then(|p| p.as_u64())
                    .unwrap_or(0),
            ),
            None => (false, 0),
        });
    }
    // 有 game_count 但没有 games：看得见，只是库里没有这个 app
    if response.get("game_count").is_some() {
        return Some((false, 0));
    }
    None
}

/// 按时长降序；同一时长按名字排，避免每次刷新顺序都在变。
pub fn sort_rows(rows: &mut [FriendRow]) {
    rows.sort_by(|a, b| {
        b.playtime_min
            .unwrap_or(0)
            .cmp(&a.playtime_min.unwrap_or(0))
            .then_with(|| a.name.cmp(&b.name))
    });
}

// ---------------------------------------------------------------- 缓存

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendsCache {
    /// 这份列表是谁的。换 key、换账号时必须对不上——对不上就当没有缓存。
    pub steam_id: String,
    pub fetched_at: u64,
    pub friends: Vec<FriendRow>,
}

pub fn cache_path(data_root: &Path) -> PathBuf {
    data_root.join("cache").join("steam-friends.json")
}

/// 读缓存；文件坏了、缺字段，一律当没有。
pub fn read_cache(data_root: &Path) -> Option<FriendsCache> {
    let text = std::fs::read_to_string(cache_path(data_root)).ok()?;
    serde_json::from_str(&text).ok()
}

/// 读缓存，但只有属于这个 SteamID 的才算数。
pub fn read_cache_for(data_root: &Path, steam_id: &str) -> Option<FriendsCache> {
    read_cache(data_root).filter(|c| c.steam_id == steam_id)
}

pub fn write_cache(data_root: &Path, cache: &FriendsCache) -> Result<(), String> {
    let path = cache_path(data_root);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("建缓存目录失败：{e}"))?;
    }
    let body = serde_json::to_string(cache).map_err(|e| format!("缓存写不出来：{e}"))?;
    std::fs::write(&path, body).map_err(|e| format!("写缓存失败：{e}"))
}

pub fn is_fresh(cache: &FriendsCache, now: u64) -> bool {
    now.saturating_sub(cache.fetched_at) < CACHE_TTL_SECS
}

// ---------------------------------------------------------------- HTTP

/// 网络错误只说「哪一类」，绝不把 reqwest 的原文拼进去——
/// 它的 `Display` 会带上整条 URL，而 URL 里有 key。
fn short_net_error(e: reqwest::Error) -> FriendError {
    let why = if e.is_timeout() {
        "请求超时"
    } else if e.is_connect() {
        "连不上"
    } else if e.is_decode() {
        "回包读不懂"
    } else if e.is_request() {
        "请求没发出去"
    } else {
        "网络出错"
    };
    FriendError::Network(why.to_owned())
}

/// 只用来打这几个只读接口的客户端。
pub struct Steam {
    client: reqwest::Client,
}

impl Steam {
    pub fn new() -> Result<Self, FriendError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(TIMEOUT_SECS))
            .user_agent("rock-sprout/0.2 (+https://github.com/Yisun229/rock-sprout)")
            .build()
            .map_err(|e| FriendError::Network(format!("建 HTTP 客户端失败：{e}")))?;
        Ok(Self { client })
    }

    async fn get(&self, url: &str) -> Result<Value, FriendError> {
        let resp = self.client.get(url).send().await.map_err(short_net_error)?;
        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(FriendError::BadKey);
        }
        if status.as_u16() == 400 {
            // Steam 对「没带 key / key 不合法」都回 400
            return Err(FriendError::BadKey);
        }
        if !status.is_success() {
            return Err(FriendError::Network(format!(
                "Steam 返回 {}",
                status.as_u16()
            )));
        }
        let text = resp
            .text()
            .await
            .map_err(|e| FriendError::Network(format!("读回包失败：{e}")))?;
        serde_json::from_str(&text)
            .map_err(|_| FriendError::Network("Steam 回了个认不出的东西".to_owned()))
    }

    /// `GetFriendList`：只要 steamid。
    pub async fn friend_ids(&self, steam_id: &str, key: &str) -> Result<Vec<String>, FriendError> {
        let url = format!(
            "https://api.steampowered.com/ISteamUser/GetFriendList/v1/?key={key}&steamid={steam_id}&relationship=friend"
        );
        parse_friend_ids(&self.get(&url).await?)
    }

    /// `GetPlayerSummaries`：自动按 100 个一批。
    pub async fn summaries(
        &self,
        ids: &[String],
        key: &str,
    ) -> Result<Vec<Summary>, FriendError> {
        let mut out = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(SUMMARY_BATCH) {
            let joined = chunk.join(",");
            let url = format!(
                "https://api.steampowered.com/ISteamUser/GetPlayerSummaries/v2/?key={key}&steamids={joined}"
            );
            out.extend(parse_summaries(&self.get(&url).await?));
        }
        Ok(out)
    }

    /// `GetOwnedGames`：一人一次，`None` = 看不见。
    ///
    /// `include_played_free_games=1` 是因为 DRG 试玩周末也写进库里；
    /// `appids_filter[0]` 只要这一个游戏，少传几百条无关记录。
    pub async fn owned(
        &self,
        steam_id: &str,
        key: &str,
    ) -> Result<Option<(bool, u64)>, FriendError> {
        let url = format!(
            "https://api.steampowered.com/IPlayerService/GetOwnedGames/v1/?key={key}&steamid={steam_id}&include_appinfo=0&include_played_free_games=1&appids_filter[0]={GAME_APPID}"
        );
        Ok(parse_owned(&self.get(&url).await?))
    }
}

/// 取全量好友并标注「看不看得见」。
///
/// `on_progress(checked, total)` 每查完一个人叫一次，界面拿它显示「正在查 37/120」。
pub async fn fetch_all<F>(
    steam: &Steam,
    steam_id: &str,
    key: &str,
    on_progress: &mut F,
) -> Result<Vec<FriendRow>, FriendError>
where
    F: FnMut(u32, u32) + Send,
{
    let ids = steam.friend_ids(steam_id, key).await?;
    let total = ids.len() as u32;
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let summaries = steam.summaries(&ids, key).await?;
    let mut rows: Vec<FriendRow> = summaries
        .iter()
        .filter(|s| ids.iter().any(|id| id == &s.steam_id))
        .map(FriendRow::from_summary)
        .collect();

    // 只对公开的人查库；私密/仅好友的一次都不发
    let visible: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.visibility == 3)
        .map(|(i, _)| i)
        .collect();

    let mut checked = 0u32;
    for chunk in visible.chunks(CONCURRENCY) {
        let mut set = tokio::task::JoinSet::new();
        for &i in chunk {
            let id = rows[i].steam_id.clone();
            let key = key.to_owned();
            let client = steam.client.clone();
            set.spawn(async move {
                let url = format!(
                    "https://api.steampowered.com/IPlayerService/GetOwnedGames/v1/?key={key}&steamid={id}&include_appinfo=0&include_played_free_games=1&appids_filter[0]={GAME_APPID}"
                );
                let result = match client.get(&url).send().await {
                    Ok(resp) => {
                        let status = resp.status();
                        if status.as_u16() == 401 || status.as_u16() == 403 || status.as_u16() == 400 {
                            Err(FriendError::BadKey)
                        } else if !status.is_success() {
                            Err(FriendError::Network(format!("Steam 返回 {}", status.as_u16())))
                        } else {
                            match resp.text().await {
                                Ok(text) => Ok(serde_json::from_str::<Value>(&text)
                                    .ok()
                                    .and_then(|v| parse_owned(&v))),
                                Err(e) => Err(FriendError::Network(format!("读回包失败：{e}"))),
                            }
                        }
                    }
                    Err(e) => Err(short_net_error(e)),
                };
                (i, result)
            });
        }
        while let Some(joined) = set.join_next().await {
            let Ok((i, result)) = joined else {
                continue;
            };
            match result? {
                Some((owns, minutes)) => {
                    rows[i].owns_game = Some(owns);
                    rows[i].playtime_min = Some(minutes);
                }
                None => { /* 看不见：保持 None，界面把它归到「未公开」 */ }
            }
            checked += 1;
            on_progress(checked, total);
        }
    }

    // 没查库的（不公开）也算查过了，进度条才收得回去
    on_progress(total, total);
    sort_rows(&mut rows);
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn 好友列表缺失是不给_不是没好友() {
        let v = json!({});
        assert_eq!(parse_friend_ids(&v), Err(FriendError::NoFriendList));
        let empty = json!({"friendslist": {}});
        assert_eq!(parse_friend_ids(&empty), Ok(Vec::new()));
    }

    #[test]
    fn 好友列表按原样取_steamid() {
        let v = json!({"friendslist": {"friends": [
            {"steamid": "76561198000000001", "relationship": "friend"},
            {"steamid": "76561198000000002", "relationship": "friend"}
        ]}});
        assert_eq!(
            parse_friend_ids(&v),
            Ok(vec![
                "76561198000000001".to_owned(),
                "76561198000000002".to_owned()
            ])
        );
    }

    #[test]
    fn 摘要里取昵称头像与可见性() {
        let v = json!({"response": {"players": [
            {"steamid": "1", "personaname": "甲", "avatarfull": "https://a", "communityvisibilitystate": 3},
            {"steamid": "2", "personaname": "乙", "avatarmedium": "https://b", "communityvisibilitystate": 1}
        ]}});
        let s = parse_summaries(&v);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].visibility, 3);
        assert_eq!(s[1].avatar, "https://b");
    }

    #[test]
    fn 空对象是看不见_不是零游戏() {
        assert_eq!(parse_owned(&json!({"response": {}})), None);
    }

    #[test]
    fn 有_game_count_但没买这个游戏_是看得见且没买() {
        assert_eq!(
            parse_owned(&json!({"response": {"game_count": 0}})),
            Some((false, 0))
        );
    }

    #[test]
    fn 买了就带出分钟数() {
        let v = json!({"response": {"game_count": 1, "games": [
            {"appid": 548430, "playtime_forever": 12345}
        ]}});
        assert_eq!(parse_owned(&v), Some((true, 12345)));
    }

    #[test]
    fn 看不见与零游戏在缓存里也是两回事() {
        let hidden = FriendRow {
            steam_id: "1".to_owned(),
            name: "看不见".to_owned(),
            avatar: String::new(),
            visibility: 3,
            owns_game: None,
            playtime_min: None,
        };
        let none = FriendRow {
            steam_id: "2".to_owned(),
            name: "看得见没买".to_owned(),
            avatar: String::new(),
            visibility: 3,
            owns_game: Some(false),
            playtime_min: Some(0),
        };
        assert_ne!(hidden.owns_game, none.owns_game);
    }

    #[test]
    fn 缓存认人_换账号就不算数() {
        let dir = std::env::temp_dir().join("rs-friends-cache-test");
        let _ = std::fs::remove_dir_all(&dir);
        let cache = FriendsCache {
            steam_id: "76561198000000001".to_owned(),
            fetched_at: now_secs(),
            friends: Vec::new(),
        };
        write_cache(&dir, &cache).unwrap();
        assert!(read_cache_for(&dir, "76561198000000001").is_some());
        assert!(read_cache_for(&dir, "76561198000000002").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 排序按时长降序_同样时长按名字() {
        let row = |name: &str, min: Option<u64>| FriendRow {
            steam_id: name.to_owned(),
            name: name.to_owned(),
            avatar: String::new(),
            visibility: 3,
            owns_game: min.map(|_| true),
            playtime_min: min,
        };
        let mut rows = vec![row("乙", Some(10)), row("甲", Some(90)), row("丙", Some(10))];
        sort_rows(&mut rows);
        assert_eq!(rows[0].name, "甲");
        assert_eq!(rows[1].name, "丙");
        assert_eq!(rows[2].name, "乙");
    }

    #[test]
    fn 账号号拼成_steamid64() {
        assert_eq!(id_from_account(0), "76561197960265728");
        assert_eq!(id_from_account(100), "76561197960265828");
    }

    #[test]
    fn 从_loginusers_里找最近登录那个() {
        let vdf = r#"
"users"
{
	"76561199027062409"
	{
		"AccountName"		"alpha"
		"MostRecent"		"0"
	}
	"76561199034598912"
	{
		"AccountName"		"beta"
		"MostRecent"		"1"
	}
}
"#;
        assert_eq!(parse_loginusers(vdf).as_deref(), Some("76561199034598912"));
    }

    #[test]
    fn loginusers_读不出来就返回_none() {
        assert_eq!(parse_loginusers(""), None);
        assert_eq!(parse_loginusers("\"users\" { }"), None);
    }
}
