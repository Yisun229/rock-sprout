//! Steam 商店取数：当前价、折扣、在线人数。
//!
//! 这些是**展示性数据**：拉不到不该让整页报错，所以每个函数都把人话错误当字符串
//! 返回，调用方自己决定是留空、还是显示上次缓存。两个接口都不需要 key：
//!
//! - `store.steampowered.com/api/appdetails?appids=…&cc=…&l=schinese`
//! - `api.steampowered.com/ISteamUserStats/GetNumberOfCurrentPlayers/v1/?appid=…`
//!
//! 解析和网络分开写：`parse_price` / `parse_players` 是纯函数，单测不碰网络。

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// 缓存多久之内不再发请求（秒）。10 分钟：够「刷新」按钮用，又不至于打到限流。
pub const CACHE_TTL_SECS: u64 = 600;

/// 一次价格查询的结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Price {
    /// 币种代码，如 `CNY`；没有价格信息时是空串
    pub currency: String,
    /// 原价，以「分」为单位（Steam 返回的就是分）
    pub initial: i64,
    /// 现价（分）
    #[serde(rename = "final")]
    pub final_price: i64,
    /// 折扣百分比，0–100
    pub discount_percent: i32,
    /// 这个区有没有价格。免费、未上架、该区不卖都是 `false`——这不是错误，
    /// 界面写「这个区看不到价格」即可。
    pub available: bool,
}

impl Price {
    pub fn missing() -> Self {
        Self {
            currency: String::new(),
            initial: 0,
            final_price: 0,
            discount_percent: 0,
            available: false,
        }
    }
}

fn appdetails_url(appid: u32, cc: &str) -> String {
    format!("https://store.steampowered.com/api/appdetails?appids={appid}&cc={cc}&l=schinese")
}

fn players_url(appid: u32) -> String {
    format!("https://api.steampowered.com/ISteamUserStats/GetNumberOfCurrentPlayers/v1/?appid={appid}")
}

/// 解析 appdetails 的回包。
///
/// 形状：`{"548430":{"success":true,"data":{"price_overview":{...}}}}`。
/// `success:false` 是「查不到这个 App」，属于错误；`success:true` 但没有
/// `price_overview` 是「这个区不卖/免费」，属于正常但没有价。
pub fn parse_price(json: &str, appid: u32) -> Result<Price, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("Steam 回包读不懂：{e}"))?;
    let entry = v
        .get(appid.to_string())
        .ok_or_else(|| format!("Steam 回包里没有 {appid} 这一项"))?;
    let ok = entry
        .get("success")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !ok {
        return Err(format!("Steam 说查不到 App {appid}（success=false）"));
    }
    let Some(detail) = entry.get("data") else {
        return Err(format!("Steam 没有给 App {appid} 的详情"));
    };
    let Some(overview) = detail.get("price_overview") else {
        return Ok(Price::missing());
    };
    let currency = overview
        .get("currency")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_owned();
    let initial = overview
        .get("initial")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    let final_price = overview
        .get("final")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(initial);
    let discount_percent = overview
        .get("discount_percent")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    Ok(Price {
        currency,
        initial,
        final_price,
        discount_percent: discount_percent.clamp(0, 100) as i32,
        available: true,
    })
}

/// 解析在线人数回包：`{"response":{"player_count":1234,"result":1}}`。
pub fn parse_players(json: &str) -> Result<u64, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("Steam 回包读不懂：{e}"))?;
    let resp = v
        .get("response")
        .ok_or_else(|| "Steam 回包里没有 response".to_owned())?;
    if let Some(result) = resp.get("result").and_then(serde_json::Value::as_i64) {
        if result != 1 {
            return Err("Steam 说这份在线人数取不到（result≠1）".to_owned());
        }
    }
    resp.get("player_count")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "Steam 回包里没有 player_count".to_owned())
}

async fn get_text(url: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("rock-sprout/0.2 (+https://github.com/Yisun229/rock-sprout)")
        .build()
        .map_err(|e| format!("建 HTTP 客户端失败：{e}"))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("连不上 Steam：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Steam 返回 {}", resp.status().as_u16()));
    }
    resp.text()
        .await
        .map_err(|e| format!("读 Steam 回包失败：{e}"))
}

/// 查当前价。`cc` 是国家/地区代码（两个字母，大写），如 `CN`、`US`。
pub async fn fetch_price(appid: u32, cc: &str) -> Result<Price, String> {
    let text = get_text(&appdetails_url(appid, cc)).await?;
    parse_price(&text, appid)
}

/// 查同时在线人数。
pub async fn fetch_players(appid: u32) -> Result<u64, String> {
    let text = get_text(&players_url(appid)).await?;
    parse_players(&text)
}

/// 写入磁盘的缓存：一次「刷新」拿到的东西，带拿到的时刻。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreCache {
    /// 拿到这些数据的时刻（unix 秒）
    pub fetched_at: u64,
    /// 查询时用的国家/地区代码
    pub cc: String,
    pub appid: u32,
    pub price: Option<Price>,
    pub players: Option<u64>,
}

/// 缓存路径：`<数据目录>/cache/steam.json`
pub fn cache_path(root: &Path) -> PathBuf {
    root.join("cache").join("steam.json")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 读缓存。文件不在、读不动、内容不合法都返回 None（缓存坏了就当没有）。
pub fn read_cache(root: &Path) -> Option<StoreCache> {
    let raw = std::fs::read_to_string(cache_path(root)).ok()?;
    serde_json::from_str::<StoreCache>(&raw).ok()
}

/// 写缓存。写不进去只是少了缓存，不该让取数失败，所以错误交给调用方决定记不记。
pub fn write_cache(root: &Path, cache: &StoreCache) -> std::io::Result<PathBuf> {
    let path = cache_path(root);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(cache)?)?;
    Ok(path)
}

/// 这份缓存是不是还在有效期内。
pub fn is_fresh(cache: &StoreCache, now: u64) -> bool {
    now.saturating_sub(cache.fetched_at) < CACHE_TTL_SECS
}

/// 这份旧缓存能不能直接拿来用：同 appid、同地区、没过期、不是强制刷新。
///
/// 地区必须一致——切了地区还用旧价格，界面上就会出现「US」标签配人民币的数字。
fn chose_cache(
    old: Option<&StoreCache>,
    appid: u32,
    cc: &str,
    force: bool,
    now: u64,
) -> Option<StoreCache> {
    if force {
        return None;
    }
    let c = old?;
    if c.appid != appid || !c.cc.eq_ignore_ascii_case(cc) || !is_fresh(c, now) {
        return None;
    }
    Some(c.clone())
}

/// 新一次查询的起点。**价格一律清掉**：换了地区还留着旧价格就是错的数据；
/// 在线人数与地区无关，可以接着用。
fn seed_cache(old: Option<StoreCache>, appid: u32, cc: &str) -> StoreCache {
    StoreCache {
        fetched_at: 0,
        cc: cc.to_owned(),
        appid,
        price: None,
        players: old.filter(|c| c.appid == appid).and_then(|c| c.players),
    }
}

/// 取一份商店数据：10 分钟内、同一地区用过缓存，过期就重查；查失败的部分保留旧值。
///
/// 返回 `(数据, 是不是直接读的缓存)`。第二项就是界面上「X 分钟前拿到的」要不要出现。
pub async fn load_store(
    root: &Path,
    appid: u32,
    cc: &str,
    force: bool,
) -> (StoreCache, bool) {
    let cc_upper = cc.to_ascii_uppercase();
    let old = read_cache(root);
    if let Some(c) = chose_cache(old.as_ref(), appid, &cc_upper, force, now_secs()) {
        return (c, true);
    }
    // 只有同一个 App、同一个地区，才允许把上次的价格和人数当兜底；
    // 换地区不沿用旧价格（币种会串），这是 M1 定下的规矩。
    let reuse = old
        .as_ref()
        .filter(|c| c.appid == appid && c.cc.eq_ignore_ascii_case(&cc_upper));
    let old_at = reuse.map(|c| c.fetched_at).unwrap_or(0);
    let old_price = reuse.and_then(|c| c.price.clone());
    let old_players = reuse.and_then(|c| c.players);
    let cache = seed_cache(old, appid, &cc_upper);
    let (cache, got_any) = merge_fetched(
        cache,
        old_at,
        old_price,
        old_players,
        fetch_price(appid, &cc_upper).await,
        fetch_players(appid).await,
    );
    if got_any {
        if let Err(e) = write_cache(root, &cache) {
            tracing::warn!(%e, "商店缓存写不进去，这一次照常显示");
        }
    }
    (cache, false)
}

/// 把这一轮拿到的结果合进缓存。
///
/// 两个都失败时**不动** `fetched_at`：它必须还是「上次真正拿到数据」的那一刻，
/// 界面才知道要写「这是上次拿到的」。把 0 留在那儿会让界面以为「从没拿到过」，
/// 顺便把旧价格藏成「这个区看不到价格」——那是假话。
fn merge_fetched(
    mut cache: StoreCache,
    old_at: u64,
    old_price: Option<Price>,
    old_players: Option<u64>,
    price: Result<Price, String>,
    players: Result<u64, String>,
) -> (StoreCache, bool) {
    let mut got_any = false;
    match price {
        Ok(p) => {
            cache.price = Some(p);
            got_any = true;
        }
        Err(e) => {
            cache.price = old_price;
            tracing::warn!(%e, "商店价格没拿到，保留上一次的值");
        }
    }
    match players {
        Ok(p) => {
            cache.players = Some(p);
            got_any = true;
        }
        Err(e) => {
            cache.players = old_players;
            tracing::warn!(%e, "在线人数没拿到，保留上一次的值");
        }
    }
    if got_any {
        cache.fetched_at = now_secs();
    } else {
        cache.fetched_at = old_at;
    }
    (cache, got_any)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 有折扣的回包能读成价格() {
        let json = r#"{"548430":{"success":true,"data":{
            "name":"Deep Rock Galactic",
            "price_overview":{"currency":"CNY","initial":9000,"final":4500,"discount_percent":50}
        }}}"#;
        let p = parse_price(json, 548_430).unwrap();
        assert_eq!(p.currency, "CNY");
        assert_eq!(p.initial, 9000);
        assert_eq!(p.final_price, 4500);
        assert_eq!(p.discount_percent, 50);
        assert!(p.available);
    }

    #[test]
    fn success_false_是错误不是没价() {
        let json = r#"{"548430":{"success":false}}"#;
        let e = parse_price(json, 548_430).unwrap_err();
        assert!(e.contains("success=false"), "{e}");
    }

    #[test]
    fn 这个区不卖时没有价格但不算错误() {
        let json = r#"{"548430":{"success":true,"data":{"name":"Deep Rock Galactic"}}}"#;
        let p = parse_price(json, 548_430).unwrap();
        assert!(!p.available);
        assert_eq!(p.currency, "");
        assert_eq!(p.discount_percent, 0);
    }

    #[test]
    fn 回包不是_json_时给一句人话() {
        let e = parse_price("<html>", 1).unwrap_err();
        assert!(e.contains("读不懂"), "{e}");
    }

    #[test]
    fn 在线人数正常回包() {
        let json = r#"{"response":{"player_count":12345,"result":1}}"#;
        assert_eq!(parse_players(json).unwrap(), 12_345);
    }

    #[test]
    fn 在线人数取不到时是错误() {
        let e = parse_players(r#"{"response":{"result":42}}"#).unwrap_err();
        assert!(e.contains("result≠1"), "{e}");
    }

    #[test]
    fn 缓存写进去再读出来一样_并且十分钟内算新() {
        let dir = std::env::temp_dir().join(format!("rs-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cache = StoreCache {
            fetched_at: 1_000,
            cc: "CN".to_owned(),
            appid: 548_430,
            price: Some(Price {
                currency: "CNY".to_owned(),
                initial: 9000,
                final_price: 4500,
                discount_percent: 50,
                available: true,
            }),
            players: Some(1234),
        };
        write_cache(&dir, &cache).unwrap();
        assert_eq!(read_cache(&dir).unwrap(), cache);
        assert!(is_fresh(&cache, 1_599));
        assert!(!is_fresh(&cache, 1_600));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 改地区后第一次不会命中缓存() {
        let cached = StoreCache {
            fetched_at: 1_000,
            cc: "CN".to_owned(),
            appid: 548_430,
            price: Some(Price::missing()),
            players: Some(1234),
        };
        // 同地区、没过期：命中（界面上 fromCache=true）
        assert!(chose_cache(Some(&cached), 548_430, "CN", false, 1_100).is_some());
        // 切到 US：第一次必然重查（fromCache=false）
        assert!(chose_cache(Some(&cached), 548_430, "US", false, 1_100).is_none());
        // 强制刷新也不命中
        assert!(chose_cache(Some(&cached), 548_430, "CN", true, 1_100).is_none());
    }

    #[test]
    fn 两个都没拿到时保留上次的时间戳和数字() {
        let old = StoreCache {
            fetched_at: 1_700_000_000,
            cc: "CN".to_owned(),
            appid: 548_430,
            price: Some(Price {
                currency: "CNY".to_owned(),
                initial: 10_800,
                final_price: 10_800,
                discount_percent: 0,
                available: true,
            }),
            players: Some(1234),
        };
        let (merged, got_any) = merge_fetched(
            old.clone(),
            1_700_000_000,
            old.price.clone(),
            old.players,
            Err("断网".to_owned()),
            Err("断网".to_owned()),
        );
        assert!(!got_any);
        assert_eq!(merged.fetched_at, 1_700_000_000);
        assert_eq!(merged.players, Some(1234));
        assert_eq!(merged.price.as_ref().map(|p| p.final_price), Some(10_800));
    }

    #[test]
    fn 只拿到人数也算拿到了_但价格还是上次的() {
        let old = StoreCache {
            fetched_at: 1_700_000_000,
            cc: "CN".to_owned(),
            appid: 548_430,
            price: Some(Price {
                currency: "CNY".to_owned(),
                initial: 10_800,
                final_price: 10_800,
                discount_percent: 0,
                available: true,
            }),
            players: None,
        };
        let (merged, got_any) = merge_fetched(
            old.clone(),
            1_700_000_000,
            old.price.clone(),
            old.players,
            Err("断网".to_owned()),
            Ok(4321),
        );
        assert!(got_any);
        assert!(merged.fetched_at > 1_700_000_000);
        assert_eq!(merged.players, Some(4321));
        assert_eq!(merged.price.as_ref().map(|p| p.final_price), Some(10_800));
    }

    #[test]
    fn 切地区时旧价格不沿用_在线人数可以接着用() {
        let cached = StoreCache {
            fetched_at: 1_000,
            cc: "CN".to_owned(),
            appid: 548_430,
            price: Some(Price {
                currency: "CNY".to_owned(),
                initial: 10800,
                final_price: 10800,
                discount_percent: 0,
                available: true,
            }),
            players: Some(4392),
        };
        let seeded = seed_cache(Some(cached), 548_430, "US");
        assert_eq!(seeded.cc, "US");
        assert!(seeded.price.is_none(), "换地区还留着人民币价格会显示成美元");
        assert_eq!(seeded.players, Some(4392));
        assert_eq!(seeded.fetched_at, 0);
    }

    #[test]
    fn 缓存坏了当作没有() {
        let dir = std::env::temp_dir().join(format!("rs-store-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("cache")).unwrap();
        std::fs::write(cache_path(&dir), "not json").unwrap();
        assert!(read_cache(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
