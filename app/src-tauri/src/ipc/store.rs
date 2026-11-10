//! 「今天」页的商店块：当前价、折扣、同时在线人数。
//!
//! 只读、只展示：这些数字不参与任何判定，拉不到就退回 10 分钟内的缓存，缓存也没有
//! 就留空——不编数字，也不因为商店挂了让整页报错。

use serde::Serialize;

use super::ApiResult;
use crate::paths;
use crate::settings;
use crate::steam;
use crate::store::{self, Price};

/// 商店地区设置。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreSettings {
    /// 两个字母的国家/地区代码
    pub country: String,
}

fn store_settings() -> StoreSettings {
    StoreSettings {
        country: settings::load_at(&paths::data_root()).store_country,
    }
}

/// 读商店地区。
#[tauri::command]
pub fn get_store_settings() -> ApiResult<StoreSettings> {
    ApiResult::ok(store_settings())
}

/// 改商店地区。只收两个字母，其余回 `CN`（见 `settings::normalize_country`）。
#[tauri::command]
pub fn set_store_country(country: String) -> ApiResult<StoreSettings> {
    let root = paths::data_root();
    let mut current = settings::load_at(&root);
    current.store_country = settings::normalize_country(&country);
    match settings::save_at(&root, &current) {
        Ok(_) => ApiResult::ok(StoreSettings {
            country: current.store_country,
        }),
        Err(e) => ApiResult::err("io", format!("设置存不下来：{e}")),
    }
}

/// 一次商店快照。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreSnapshot {
    /// 游戏在 Steam 上的 appid（目前固定 548430，留给以后换游戏）
    pub appid: u32,
    /// 查询时用的地区
    pub country: String,
    /// 有没有价格信息（免费 / 该区不卖时为 false）
    pub available: bool,
    pub price: Option<Price>,
    pub players: Option<u64>,
    /// 这批数据是什么时候拿到的（unix 秒）；0 表示从没拿到过
    pub fetched_at: u64,
    /// 这一次是直接读缓存、没有重发请求
    pub from_cache: bool,
}

/// 取商店块的数据。`refresh` 为真时跳过 10 分钟有效期直接重查。
#[tauri::command]
pub async fn store_snapshot(refresh: bool) -> ApiResult<StoreSnapshot> {
    let root = paths::data_root();
    let country = settings::load_at(&root).store_country;
    let (cache, from_cache) = store::load_store(&root, steam::GAME_APPID, &country, refresh).await;
    ApiResult::ok(StoreSnapshot {
        appid: cache.appid,
        country: cache.cc,
        available: cache.price.as_ref().is_some_and(|p| p.available),
        price: cache.price,
        players: cache.players,
        fetched_at: cache.fetched_at,
        from_cache,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 快照字段在_json_里是驼峰() {
        let json = serde_json::to_value(StoreSnapshot {
            appid: 548_430,
            country: "CN".to_owned(),
            available: true,
            price: Some(Price::missing()),
            players: Some(1234),
            fetched_at: 99,
            from_cache: false,
        })
        .unwrap();
        assert_eq!(json["fetchedAt"], 99);
        assert_eq!(json["fromCache"], false);
        assert_eq!(json["price"]["discountPercent"], 0);
    }
}
