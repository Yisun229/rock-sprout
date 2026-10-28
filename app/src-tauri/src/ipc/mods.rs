//! mod 相关的命令。
//!
//! 行为：
//!
//! - 搜索 / 详情 / 文件列表 / 下载都走真的 mod.io 接口（见 [`crate::modio`]）。
//! - 令牌从 `<数据目录>\modio-token.txt` 读，没有就返回 `no_token`，前端据此引导去设置页。
//! - 「安装」= 下载到本地 + 记账 + **立刻按记账重建游戏里的合并包**（见 [`crate::game`]）；
//!   「卸载」= 删掉下载的文件和记账 + 重建合并包，一个不剩时把包和加载器一起清掉。
//!
//! 记账方式很朴素：每个已下载的 mod 在 `<数据目录>\mods\installed\<id>.json` 存一份。
//! 合并包每次都按「记账里现有的全部 mod」整包重建——这样装的顺序、卸载、覆盖都不会打架。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::ApiResult;
use crate::cache::Cache;
use crate::game;
use crate::modio::{ModDetail, Modio, ModioError, SearchPage};
use crate::paths;

pub use crate::modio::ModSummary;

/// 已装列表里的一项。
#[derive(Debug, Serialize)]
pub struct InstalledMod {
    pub id: u64,
    pub name: String,
    pub version: String,
    /// `modio` = 从 mod.io 下的；`local` = 自己导入的本地文件
    pub source: String,
    /// 记账指向的文件已经不在了（原文件被删或挪走），界面上要标灰
    pub missing: bool,
}

/// 安装 / 卸载的结果。
#[derive(Debug, Serialize)]
pub struct InstallOutcome {
    pub id: u64,
    pub name: String,
    pub installed: bool,
    /// 合并后写进游戏的那个 pak（一个都不装、已经清干净时为 `None`）
    pub pak: Option<String>,
}

/// 磁盘上的记账文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct InstalledRecord {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) file_id: u64,
    pub(crate) filename: String,
    pub(crate) size: u64,
    pub(crate) path: String,
    /// 来源。老记账文件里没有这一项，一律当 `modio`（本地导入是后加的功能）。
    #[serde(default = "default_source")]
    pub(crate) source: String,
    /// 导入时用户给的那个路径，只用来显示，不参与 id（文件挪走后还能看出原来在哪）。
    #[serde(default)]
    pub(crate) original_path: Option<String>,
    /// 内容指纹（sha256 十六进制）。本地导入的 id 就是按它算的；老记账没有这一项，
    /// 启动时的迁移会补上。
    #[serde(default)]
    pub(crate) content_hash: Option<String>,
    /// 记账指向的文件已经不在了（迁移时找不到原文件就标上）。
    #[serde(default)]
    pub(crate) missing: bool,
    /// 第一次装上的时刻（毫秒时间戳）。合并两条重复记录时留更早的那个。
    #[serde(default)]
    pub(crate) installed_at: Option<u64>,
}

fn default_source() -> String {
    "modio".to_owned()
}

impl InstalledRecord {
    fn to_installed(&self) -> InstalledMod {
        InstalledMod {
            id: self.id,
            name: self.name.clone(),
            version: self.filename.clone(),
            source: self.source.clone(),
            missing: self.missing,
        }
    }

    /// 交给合并器时用的「来源」：mod.io 的页面地址，本地文件则标成本地来源。
    ///
    /// 这个串会写进游戏 pak 的元数据里，用来在游戏内的 mod 列表上区分谁是谁。
    fn source_url(&self) -> String {
        if self.source == "local" {
            format!("local://{}", self.filename)
        } else {
            mod_page_url(self.id)
        }
    }
}

pub(crate) fn records_dir() -> PathBuf {
    paths::mods_cache_dir().join("installed")
}

/// 现在的毫秒时间戳。记账里用它记「第一次装上的时刻」，合并重复记录时留更早的那个。
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(crate) fn records_dir_at(data_root: &Path) -> PathBuf {
    data_root.join("mods").join("installed")
}

pub(crate) fn record_path(dir: &Path, id: u64) -> PathBuf {
    dir.join(format!("{id}.json"))
}

pub(crate) fn save_record(dir: &Path, rec: &InstalledRecord) -> Result<(), ModioError> {
    std::fs::create_dir_all(dir).map_err(|e| ModioError::Io(e.to_string()))?;
    let text = serde_json::to_string_pretty(rec).map_err(|e| ModioError::Io(e.to_string()))?;
    std::fs::write(record_path(dir, rec.id), text).map_err(|e| ModioError::Io(e.to_string()))
}

pub(crate) fn load_records(dir: &Path) -> Vec<InstalledRecord> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(rec) = serde_json::from_str::<InstalledRecord>(&text) {
                out.push(rec);
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 把底层错误翻成前端认识的格式。断网、没登录、被限速各有各的码。
fn to_api<T>(result: Result<T, ModioError>) -> ApiResult<T> {
    match result {
        Ok(v) => ApiResult::ok(v),
        Err(e) => ApiResult::err(e.code(), e.message()),
    }
}

/// 按令牌文件建客户端；没有令牌就返回「没登录」。
fn client() -> Result<Modio, ModioError> {
    Modio::from_token_file(&paths::token_file())
}

// ---- 命令本体（Tauri 只做一层薄包装，逻辑都在这些 async 函数里）----

/// 请求成功后顺手存一份缓存；存失败只记日志，不影响这次结果。
fn remember_page(cache: &Cache, query: &str, page: u32, data: &SearchPage) {
    if let Err(e) = cache.save_search(query, page, data) {
        tracing::warn!(%e, "写搜索缓存失败，这次结果照常返回");
    }
}

/// 把「网络结果」和「缓存」揉成最终返回值。
///
/// 只有 `Network` 这种「网不通」的情况才回退缓存——令牌失效、被限速这些必须如实报错，
/// 否则用户会以为「已经登录了、只是网慢」。
fn search_with_cache(
    cache: &Cache,
    result: Result<SearchPage, ModioError>,
    query: &str,
    page: u32,
) -> Result<SearchPage, ModioError> {
    match result {
        Ok(data) => {
            remember_page(cache, query, page, &data);
            Ok(data)
        }
        Err(ModioError::Network(msg)) => match cache.load_search(query, page) {
            Some(fresh) => {
                tracing::info!(query, page, "连不上 mod.io，用上次存下的搜索结果");
                let mut data = fresh.value;
                data.stale = true;
                data.cached_at = Some(fresh.cached_at);
                Ok(data)
            }
            None => Err(ModioError::Network(msg)),
        },
        Err(e) => Err(e),
    }
}

/// 详情版本的回退，规则同上。
fn detail_with_cache(
    cache: &Cache,
    result: Result<ModDetail, ModioError>,
    id: u64,
) -> Result<ModDetail, ModioError> {
    match result {
        Ok(data) => {
            if let Err(e) = cache.save_detail(&data) {
                tracing::warn!(%e, "写详情缓存失败，这次结果照常返回");
            }
            Ok(data)
        }
        Err(ModioError::Network(msg)) => match cache.load_detail(id) {
            Some(fresh) => {
                tracing::info!(id, "连不上 mod.io，用上次存下的 mod 详情");
                let mut data = fresh.value;
                data.stale = true;
                data.cached_at = Some(fresh.cached_at);
                Ok(data)
            }
            None => Err(ModioError::Network(msg)),
        },
        Err(e) => Err(e),
    }
}

#[tauri::command]
pub async fn search_mods(query: String, page: u32) -> ApiResult<SearchPage> {
    let cache = Cache::at_data_dir();
    let result = async {
        let c = client()?;
        c.search(&query, page).await
    }
    .await;
    to_api(search_with_cache(&cache, result, &query, page))
}

#[tauri::command]
pub async fn get_mod_detail(id: u64) -> ApiResult<ModDetail> {
    let cache = Cache::at_data_dir();
    let result = async {
        let c = client()?;
        c.detail(id).await
    }
    .await;
    to_api(detail_with_cache(&cache, result, id))
}

/// mod.io 上这个 mod 的页面地址。pak 里记的「来源」就是它，不用另存字段。
fn mod_page_url(id: u64) -> String {
    format!("https://mod.io/g/drg/m/{id}")
}

/// 把「已下载并记账」的那批 mod 翻成合并器认识的输入。
fn pending_mods() -> Vec<game::ModToInstall> {
    mods_to_install(load_records(&records_dir()))
}

/// 记账 → 合并器输入（单独拆出来是为了能单测，不用碰真实数据目录）。
fn mods_to_install(records: Vec<InstalledRecord>) -> Vec<game::ModToInstall> {
    records
        .into_iter()
        .map(|r| game::ModToInstall {
            name: r.name.clone(),
            url: r.source_url(),
            path: PathBuf::from(&r.path),
        })
        .filter(|m| m.path.exists())
        .collect()
}

/// 按当前记账重建游戏里的合并包。没有 mod 剩下时就把包和加载器一起清掉。
///
/// 合并是同步的重活（要读 2.5 GB 主包），所以丢到阻塞线程池里跑，别堵住界面。
async fn rebuild_pak() -> Result<Option<String>, String> {
    let list = pending_mods();
    if list.is_empty() {
        tokio::task::spawn_blocking(game::clear)
            .await
            .map_err(|e| format!("后台清理任务出错：{e}"))??;
        return Ok(None);
    }

    let pak = tokio::task::spawn_blocking(move || game::apply(&list))
        .await
        .map_err(|e| format!("后台合并任务出错：{e}"))??;
    Ok(Some(pak.display().to_string()))
}

/// 只下载 + 记账，不动游戏目录。
///
/// 批量装的时候先并发下载（2–3 路），全部下完再一次性合并，
/// 这样「下载」能重叠、「合并」不会互相踩。
#[tauri::command]
pub async fn download_mod(id: u64) -> ApiResult<InstallOutcome> {
    match prepare(id).await {
        Ok(rec) => ApiResult::ok(InstallOutcome {
            id: rec.id,
            name: rec.name,
            installed: false,
            pak: None,
        }),
        Err((code, message)) => ApiResult::err(code, message),
    }
}

/// 按现在的记账重建一次合并包（不含下载）。
#[tauri::command]
pub async fn apply_all() -> ApiResult<Option<String>> {
    match rebuild_pak().await {
        Ok(pak) => ApiResult::ok(pak),
        Err(message) => ApiResult::err("integrate", message),
    }
}

/// 下载 → 记账 → 合并进游戏。
#[tauri::command]
pub async fn install_mod(id: u64) -> ApiResult<InstallOutcome> {
    let rec = match prepare(id).await {
        Ok(rec) => rec,
        Err((code, message)) => return ApiResult::err(code, message),
    };

    match rebuild_pak().await {
        Ok(pak) => ApiResult::ok(InstallOutcome {
            id: rec.id,
            name: rec.name,
            installed: true,
            pak,
        }),
        Err(message) => ApiResult::err("integrate", message),
    }
}

/// 下载一个 mod 的文件并写记账；出错时返回 (错误码, 中文说明)。
async fn prepare(id: u64) -> Result<InstalledRecord, (&'static str, String)> {
    let c = match client() {
        Ok(c) => c,
        Err(e) => return Err((e.code(), e.message())),
    };

    let detail = match c.detail(id).await {
        Ok(d) => d,
        Err(e) => return Err((e.code(), e.message())),
    };
    let latest = match c.files(id).await {
        Ok(files) => match files.into_iter().next() {
            Some(f) => f,
            None => return Err(("no_file", "这个 mod 没有可下载的文件".to_owned())),
        },
        Err(e) => return Err((e.code(), e.message())),
    };

    let outcome = match c.download(&latest, &paths::mods_cache_dir()).await {
        Ok(o) => o,
        Err(e) => return Err((e.code(), e.message())),
    };

    let rec = InstalledRecord {
        id: detail.id,
        name: detail.name,
        file_id: latest.id,
        filename: outcome.filename,
        size: outcome.size,
        path: outcome.path,
        source: "modio".to_owned(),
        original_path: None,
        content_hash: None,
        missing: false,
        installed_at: Some(now_ms()),
    };

    if let Err(e) = save_record(&records_dir(), &rec) {
        return Err((e.code(), e.message()));
    }
    Ok(rec)
}

/// 导入本地文件：.pak、装着 .pak 的 .zip，或者解包后的文件夹（文件夹会被打成 pak）。
///
/// 认下来之后和从 mod.io 装的走同一条路：复制到自己目录 → 记账 → 重建合并包。
/// 所以「断网也能装东西」就是这么来的——这一步完全不碰网络。
#[tauri::command]
pub async fn import_local(path: String) -> ApiResult<InstallOutcome> {
    let dest_dir = paths::mods_cache_dir().join("local");
    let resolved = match crate::local::resolve(Path::new(&path), &dest_dir) {
        Ok(v) => v,
        Err(e) => return ApiResult::err("import", e),
    };

    let filename = resolved
        .path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| resolved.name.clone());
    let size = std::fs::metadata(&resolved.path).map(|m| m.len()).unwrap_or(0);
    let id = resolved.id;

    // 同一个内容再导一次时，把原来的「什么时候装的」留着，别覆盖成现在
    let dir = records_dir();
    let installed_at = load_records(&dir)
        .into_iter()
        .find(|r| r.id == id)
        .and_then(|r| r.installed_at)
        .or_else(|| Some(now_ms()));

    if let Err(e) = save_record(
        &dir,
        &InstalledRecord {
            id,
            name: resolved.name.clone(),
            file_id: 0,
            filename,
            size,
            path: resolved.path.display().to_string(),
            source: "local".to_owned(),
            original_path: Some(path.clone()),
            content_hash: Some(resolved.hash.clone()),
            missing: false,
            installed_at,
        },
    ) {
        return ApiResult::err(e.code(), e.message());
    }

    match rebuild_pak().await {
        Ok(pak) => ApiResult::ok(InstallOutcome {
            id,
            name: resolved.name,
            installed: true,
            pak,
        }),
        Err(message) => ApiResult::err("integrate", message),
    }
}

/// 卸载：删下载的文件和记账，然后重建合并包（没剩下就清干净）。
#[tauri::command]
pub async fn uninstall_mod(id: u64) -> ApiResult<InstallOutcome> {
    let dir = records_dir();
    let path = record_path(&dir, id);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return ApiResult::err("not_installed", "这个 mod 不在已装列表里"),
    };
    let rec: InstalledRecord = match serde_json::from_str(&text) {
        Ok(r) => r,
        Err(e) => return ApiResult::err("io", format!("记账文件读不出来：{e}")),
    };

    let _ = std::fs::remove_file(&rec.path);
    if let Err(e) = std::fs::remove_file(&path) {
        return ApiResult::err("io", format!("删记账文件失败：{e}"));
    }

    match rebuild_pak().await {
        Ok(pak) => ApiResult::ok(InstallOutcome {
            id: rec.id,
            name: rec.name,
            installed: false,
            pak,
        }),
        Err(message) => ApiResult::err("integrate", message),
    }
}

/// 预检：装这些东西会不会盖掉已经装的。
///
/// 每一项可以是本地文件 / 文件夹路径，也可以是已下载过的 mod.io 数字 id。
/// 只回答「会撞吗、撞谁、撞几处」，不动游戏目录、不装任何东西。
#[tauri::command]
pub fn precheck_install(mod_ids: Vec<String>) -> ApiResult<crate::manifest::PrecheckReport> {
    let records = load_records(&records_dir());
    let installed = records
        .iter()
        .map(|r| crate::manifest::Incoming {
            name: r.name.clone(),
            path: PathBuf::from(&r.path),
        })
        .collect::<Vec<_>>();

    let mut incoming = Vec::new();
    for item in &mod_ids {
        if let Ok(id) = item.parse::<u64>()
            && let Some(record) = records.iter().find(|r| r.id == id)
        {
            incoming.push(crate::manifest::Incoming {
                name: record.name.clone(),
                path: PathBuf::from(&record.path),
            });
            continue;
        }
        let path = PathBuf::from(item);
        if !path.exists() {
            return ApiResult::err("not_found", format!("找不到这个文件或文件夹：{item}"));
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| item.clone());
        incoming.push(crate::manifest::Incoming { name, path });
    }

    match crate::manifest::precheck(&incoming, &installed, &Cache::at_data_dir().root()) {
        Ok(report) => ApiResult::ok(report),
        Err(message) => ApiResult::err("precheck", message),
    }
}

#[tauri::command]
pub async fn list_installed() -> ApiResult<Vec<InstalledMod>> {
    let records = load_records(&records_dir());
    ApiResult::ok(records.iter().map(|r| r.to_installed()).collect())
}

/// 扫一遍已经装着的 mod，回答「谁盖着谁」。
///
/// 合并顺序按记账里的 `installed_at`（先装的在前），同一个文件后面的盖前面的——
/// 跟后端真正合并时的顺序一致，所以结果能当真。
#[tauri::command]
pub fn scan_conflicts() -> ApiResult<crate::manifest::ScanReport> {
    let mut records = load_records(&records_dir());
    records.sort_by_key(|r| r.installed_at.unwrap_or(0));
    let list = records
        .iter()
        .map(|r| (r.id, r.name.clone(), PathBuf::from(&r.path)))
        .collect::<Vec<_>>();
    match crate::manifest::scan(&list, &Cache::at_data_dir().root()) {
        Ok(report) => ApiResult::ok(report),
        Err(message) => ApiResult::err("scan", message),
    }
}

/// 当前已装列表 → 导出用的清单。
///
/// 只留「哪一台机器都能用得上」的信息：名字、来源、地址、文件名。
/// 本地路径不导出——换机器后那个路径根本不存在，写进去只会误导。
pub fn exported_mods() -> Vec<crate::config::ExportedMod> {
    records_to_export(load_records(&records_dir()))
}

fn records_to_export(records: Vec<InstalledRecord>) -> Vec<crate::config::ExportedMod> {
    records
        .into_iter()
        .map(|r| crate::config::ExportedMod {
            name: r.name.clone(),
            source: r.source.clone(),
            url: if r.source == "local" {
                None
            } else {
                Some(mod_page_url(r.id))
            },
            filename: r.filename.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用的临时目录放在构建产物目录下，不写 C 盘。
    fn test_dir(name: &str) -> PathBuf {
        let base = std::env::var("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("target"));
        let dir = base.join("rock-sprout-tests").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn sample_record() -> InstalledRecord {
        InstalledRecord {
            id: 101,
            name: "更亮的矿物".to_owned(),
            file_id: 9001,
            filename: "brighter-objects-1.12.3.zip".to_owned(),
            size: 452_000,
            path: "X:/mods/brighter-objects-1.12.3.zip".to_owned(),
            source: "modio".to_owned(),
            original_path: None,
            content_hash: None,
            missing: false,
            installed_at: Some(1_700_000_000_000),
        }
    }

    #[test]
    fn 记账写进去能读回来() {
        let dir = test_dir("record-roundtrip");
        save_record(&dir, &sample_record()).expect("写记账");
        let loaded = load_records(&dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, 101);
        assert_eq!(loaded[0].to_installed().version, "brighter-objects-1.12.3.zip");
    }

    #[test]
    fn 目录不存在时已装列表是空的而不是报错() {
        let dir = test_dir("empty-dir");
        assert!(load_records(&dir).is_empty());
    }

    #[test]
    fn 错误会被翻成前端认识的码() {
        let api = to_api::<u8>(Err(ModioError::NoToken));
        assert!(!api.ok);
        assert_eq!(api.error.expect("有 error").code, "no_token");

        let api = to_api::<u8>(Err(ModioError::Unauthorized));
        assert_eq!(api.error.expect("有 error").code, "unauthorized");
    }

    #[test]
    fn 断网时有缓存就回放旧结果并标上离线() {
        let cache = Cache::new(test_dir("offline-search-hit"));
        let page = SearchPage {
            query: "钻石".to_owned(),
            page: 1,
            total: 1,
            items: Vec::new(),
            stale: false,
            cached_at: None,
        };
        cache.save_search("钻石", 1, &page).expect("先存一份缓存");

        let got = search_with_cache(
            &cache,
            Err(ModioError::Network("dns 失败".to_owned())),
            "钻石",
            1,
        )
        .expect("应该回放缓存");

        assert!(got.stale, "要标明这是离线数据");
        assert!(got.cached_at.is_some(), "要带上缓存时间");
    }

    #[test]
    fn 断网时没缓存就照旧报网络错误() {
        let cache = Cache::new(test_dir("offline-search-miss"));
        let err = search_with_cache(
            &cache,
            Err(ModioError::Network("dns 失败".to_owned())),
            "从没搜过的词",
            1,
        )
        .expect_err("没缓存就该报错");

        assert_eq!(err.code(), "network");
    }

    #[test]
    fn 令牌失效不能被缓存兜住() {
        let cache = Cache::new(test_dir("offline-search-401"));

        let err = search_with_cache(&cache, Err(ModioError::Unauthorized), "钻石", 1)
            .expect_err("401 必须如实报错");

        assert_eq!(err.code(), "unauthorized");
    }

    #[test]
    fn 详情断网时也能回放缓存() {
        let cache = Cache::new(test_dir("offline-detail-hit"));
        let detail = ModDetail {
            id: 42,
            name: "测试".to_owned(),
            summary: String::new(),
            description: String::new(),
            downloads: 1,
            version: "1.0".to_owned(),
            stale: false,
            cached_at: None,
        };
        cache.save_detail(&detail).expect("先存一份详情");

        let got = detail_with_cache(&cache, Err(ModioError::Network("断网".to_owned())), 42)
            .expect("应该回放缓存");

        assert!(got.stale);
        assert_eq!(got.name, "测试");
    }

    #[test]
    fn 老记账文件没有来源字段时当成modio() {
        let dir = test_dir("record-legacy-source");
        std::fs::create_dir_all(&dir).expect("建目录");
        std::fs::write(
            record_path(&dir, 7),
            r#"{"id":7,"name":"老记录","file_id":1,"filename":"a.zip","size":1,"path":"X:/a.zip"}"#,
        )
        .expect("写老格式的记账文件");

        let loaded = load_records(&dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].source, "modio", "老记录要默认当 mod.io 来源");
    }

    #[test]
    fn 本地导入的记录用本地来源而不是modio页面() {
        let dir = test_dir("local-source-url");
        std::fs::create_dir_all(&dir).expect("建目录");
        let file = dir.join("我的美化.pak");
        std::fs::write(&file, b"pak-bytes").expect("造个文件");

        let mut rec = sample_record();
        rec.source = "local".to_owned();
        rec.filename = "我的美化.pak".to_owned();
        rec.path = file.display().to_string();

        let got = mods_to_install(vec![rec]);
        assert_eq!(got.len(), 1, "文件在，应该留下来");
        assert_eq!(got[0].url, "local://我的美化.pak");
        assert_eq!(got[0].name, "更亮的矿物");
    }

    #[test]
    fn modio来源的记录用的是页面地址() {
        let dir = test_dir("modio-source-url");
        std::fs::create_dir_all(&dir).expect("建目录");
        let file = dir.join("brighter-objects-1.12.3.zip");
        std::fs::write(&file, b"zip-bytes").expect("造个文件");

        let mut rec = sample_record();
        rec.path = file.display().to_string();

        let got = mods_to_install(vec![rec]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].url, "https://mod.io/g/drg/m/101");
    }

    #[test]
    fn 文件已经不在的记录不会被拿去合并() {
        let dir = test_dir("missing-file-record");
        let mut rec = sample_record();
        rec.path = dir.join("早就被删了.zip").display().to_string();

        assert!(mods_to_install(vec![rec]).is_empty());
    }
}
