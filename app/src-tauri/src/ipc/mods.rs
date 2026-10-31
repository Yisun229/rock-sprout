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
    /// `pak` = 数据包（合进 `mods_P.pak`）；`script` = 脚本类（落在 `ue4ss\Mods\<名字>\`）
    pub kind: String,
    /// 记账指向的文件已经不在了（原文件被删或挪走），界面上要标灰
    pub missing: bool,
    /// 脚本类专用：`ok` / `failed` / `unknown`（数据包一律空串）
    pub script_state: String,
    /// 脚本类专用：没生效时的一句话原因（界面直接显示）
    pub script_detail: String,
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
    /// `pak` / `script`。老记账没有这一项，一律当数据包。
    #[serde(default = "default_kind")]
    pub(crate) kind: String,
}

fn default_source() -> String {
    "modio".to_owned()
}

fn default_kind() -> String {
    "pak".to_owned()
}

impl InstalledRecord {
    fn to_installed(&self) -> InstalledMod {
        InstalledMod {
            id: self.id,
            name: self.name.clone(),
            version: self.filename.clone(),
            source: self.source.clone(),
            kind: self.kind.clone(),
            missing: self.missing,
            script_state: String::new(),
            script_detail: String::new(),
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

/// 按本机令牌建客户端；没有令牌就返回「没登录」。
fn client() -> Result<Modio, ModioError> {
    crate::token::client(&paths::data_root())
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

/// 带筛选的列表：关键字 + 类型标签 + 游戏版本 + 排序。
///
/// 跟 `search_mods` 共用同一条请求构造（`modio::list`），区别只在这条多了筛选项、
/// 并且**能翻页**——筛完了用户还得能看第二页。缓存只在「跟老的纯搜索完全同一条请求」
/// 时才写，免得筛选结果把纯搜索的缓存顶掉。
#[tauri::command]
pub async fn list_mods(
    keyword: Option<String>,
    tags: Vec<String>,
    version: Option<String>,
    sort: String,
    page: u32,
) -> ApiResult<SearchPage> {
    let has_version = version
        .as_deref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let plain_search = tags.is_empty() && !has_version && sort == "downloads";

    let result = async {
        let c = client()?;
        c.list(keyword.as_deref(), &tags, version.as_deref(), &sort, page)
            .await
    }
    .await;

    if !plain_search {
        return to_api(result);
    }
    let cache = Cache::at_data_dir();
    let query = keyword.unwrap_or_default();
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

/// 记账 → 合并器输入（单独拆出来是为了能单测，不用碰真实数据目录）。
///
/// 只送 `kind == "pak"` 的：脚本类 mod 走 UE4SS 那条路，塞进合并包既没用又会让人以为装上了。
fn mods_to_install(records: Vec<InstalledRecord>) -> Vec<game::ModToInstall> {
    records
        .into_iter()
        .filter(|r| r.kind == "pak")
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
    rebuild_from(load_records(&records_dir())).await
}

/// 按指定的这批记录重建合并包。
///
/// 升级时用得到：手里拿的是「除了换掉的那条以外都是旧记录」，合并成功之前一个字节都不落账。
async fn rebuild_from(records: Vec<InstalledRecord>) -> Result<Option<String>, String> {
    let list = mods_to_install(records);
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

/// 动手改游戏目录之前先留一份现场（失败了只记一笔，不挡这次操作）。
///
/// 用户最怕的是「装完游戏起不来了，也退不回去」，
/// 所以凡是会改合并包或者记账的入口，都先拍一份自动快照。
fn snapshot_before(reason: &str) {
    match crate::snapshot::create(reason, false) {
        Ok(snap) => tracing::info!(id = %snap.id, mods = snap.meta.mods, reason, "动之前拍了快照"),
        Err(e) => tracing::warn!(%e, reason, "快照没拍成，这次照常往下走"),
    }
}

/// 游戏开着的时候文件被它占着，改不了——先问一句再动手。
fn blocked_by_running_game() -> Option<(String, String)> {
    match crate::proc::guard() {
        Ok(()) => None,
        Err(message) => Some(("game_running".to_owned(), message)),
    }
}

/// 只下载 + 记账，不动游戏目录。
///
/// 批量装的时候先并发下载（2–3 路），全部下完再一次性合并，
/// 这样「下载」能重叠、「合并」不会互相踩。
#[tauri::command]
pub async fn download_mod(id: u64) -> ApiResult<InstallOutcome> {
    if let Some((code, message)) = blocked_by_running_game() {
        return ApiResult::err(code, message);
    }
    snapshot_before("装之前");
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
    if let Some((code, message)) = blocked_by_running_game() {
        return ApiResult::err(code, message);
    }
    snapshot_before("装之前");
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
        kind: "pak".to_owned(),
    };

    if let Err(e) = save_record(&records_dir(), &rec) {
        return Err((e.code(), e.message()));
    }
    Ok(rec)
}

/// 升级时下载下来的包先放这里，合并成功了才搬进正式缓存目录。
fn upgrade_tmp_dir() -> PathBuf {
    paths::mods_cache_dir().join("upgrade")
}

/// 下载 mod.io 上最新的那个文件，但**先不落账、也不动旧文件**。
///
/// 返回的是「万一后面失败要清掉的那个临时路径」和新记录。落盘顺序按计划 §0 第 24 条：
/// 下载 → 校验大小 → 留在临时目录 → 重建合并包 → 成功了才搬文件、改记账、删旧文件。
async fn prepare_upgrade(
    c: &Modio,
    rec: &InstalledRecord,
) -> Result<(InstalledRecord, PathBuf), (&'static str, String)> {
    let latest = match c.latest_file(rec.id).await {
        Ok(Some(f)) => f,
        Ok(None) => {
            return Err(("no_file", "mod.io 上这个 mod 没有可下载的文件".to_owned()));
        }
        Err(e) => return Err((e.code(), e.message())),
    };
    if latest.id == rec.file_id {
        return Err(("already_latest", "已经是最新的了".to_owned()));
    }

    let tmp = upgrade_tmp_dir().join(rec.id.to_string());
    let outcome = match c.download(&latest, &tmp).await {
        Ok(o) => o,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err((e.code(), e.message()));
        }
    };
    let path = PathBuf::from(&outcome.path);
    if outcome.size != latest.size {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err((
            "size_mismatch",
            format!(
                "下载下来的文件大小不对（应该是 {} 字节，实际 {} 字节）",
                latest.size, outcome.size
            ),
        ));
    }

    let mut next = rec.clone();
    next.file_id = latest.id;
    next.filename = outcome.filename;
    next.size = outcome.size;
    next.path = outcome.path.clone();
    next.missing = false;
    Ok((next, path))
}

/// 合并已经换好了，才把文件搬进正式目录、改记账、删掉旧文件。
///
/// 顺序是「先搬再删」：搬完记账就指向新文件了，旧文件这时候删掉才安全。
fn commit_upgrade(rec: &InstalledRecord, next: &InstalledRecord, tmp: &Path) -> Result<(), (&'static str, String)> {
    let final_path = paths::mods_cache_dir().join(&next.filename);
    if let Some(parent) = final_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::rename(tmp, &final_path) {
        return Err(("io", format!("把新版搬进缓存目录时出错：{e}")));
    }

    let mut done = next.clone();
    done.path = final_path.display().to_string();
    if let Err(e) = save_record(&records_dir(), &done) {
        return Err((e.code(), e.message()));
    }

    let old = PathBuf::from(&rec.path);
    if old != final_path && old.exists() {
        if let Err(e) = std::fs::remove_file(&old) {
            tracing::warn!(%e, path = %old.display(), "旧文件没删掉，之后会显示成孤儿");
        }
    }
    let _ = std::fs::remove_dir_all(upgrade_tmp_dir());
    Ok(())
}

/// 把一条已装的 mod 换成 mod.io 上的最新版。
///
/// 失败时游戏里还是旧版（旧包、旧记账一个字节都没动），错误里明确写「旧版还能用」。
#[tauri::command]
pub async fn upgrade_mod(id: u64) -> ApiResult<InstallOutcome> {
    if let Some((code, message)) = blocked_by_running_game() {
        return ApiResult::err(code, message);
    }
    let records = load_records(&records_dir());
    let Some(old) = records.iter().find(|r| r.id == id).cloned() else {
        return ApiResult::err("not_installed", "这个 mod 不在已装列表里");
    };
    if old.source != "modio" {
        return ApiResult::err("not_modio", "本地导入的包没有在线版本可以换");
    }
    snapshot_before("升级之前");

    let c = match client() {
        Ok(c) => c,
        Err(e) => return ApiResult::err(e.code(), e.message()),
    };
    let (next, tmp) = match prepare_upgrade(&c, &old).await {
        Ok(v) => v,
        Err((code, message)) => return ApiResult::err(code, format!("没换成，旧版还能用：{message}")),
    };

    let list: Vec<InstalledRecord> = records
        .iter()
        .map(|r| if r.id == id { next.clone() } else { r.clone() })
        .collect();

    match rebuild_from(list).await {
        Ok(pak) => {
            if let Err((code, message)) = commit_upgrade(&old, &next, &tmp) {
                return ApiResult::err(
                    code,
                    format!("游戏里的合并包已经换成新版了，但记账没写成功：{message}。再点一次「更新」可以补上"),
                );
            }
            ApiResult::ok(InstallOutcome {
                id: next.id,
                name: next.name,
                installed: true,
                pak,
            })
        }
        Err(message) => {
            let _ = std::fs::remove_dir_all(upgrade_tmp_dir().join(id.to_string()));
            ApiResult::err("integrate", format!("没换成，旧版还能用：{message}"))
        }
    }
}

/// 「全部更新」要处理哪几条：从 mod.io 装的、记着文件 id、文件还在、是数据包。
///
/// 本地导入的包没有在线版本可问；老记账（v0.1.4 之前）没有 `file_id`，也问不出新旧；
/// 文件已经找不到的，先让用户处理「文件没了」这件事，别混在升级里。
fn upgrade_candidates(records: &[InstalledRecord]) -> Vec<InstalledRecord> {
    records
        .iter()
        .filter(|r| r.source == "modio" && r.file_id != 0 && !r.missing && r.kind == "pak")
        .cloned()
        .collect()
}

/// 一次把能换的 mod 全换掉：先全部下载校验，再合并一次，最后统一改记账。
///
/// 任何一条下载失败就整体放弃（临时文件清掉、旧版全留着），不搞「换了一半」。
#[tauri::command]
pub async fn upgrade_all() -> ApiResult<Vec<InstallOutcome>> {
    if let Some((code, message)) = blocked_by_running_game() {
        return ApiResult::err(code, message);
    }
    let records = load_records(&records_dir());
    let candidates = upgrade_candidates(&records);
    if candidates.is_empty() {
        return ApiResult::err("nothing", "现在没有可换的 mod（本地导入的包没有在线版本）");
    }
    snapshot_before("升级之前");

    let c = match client() {
        Ok(c) => c,
        Err(e) => return ApiResult::err(e.code(), e.message()),
    };

    let mut prepared: Vec<(InstalledRecord, InstalledRecord, PathBuf)> = Vec::new();
    for old in &candidates {
        match prepare_upgrade(&c, old).await {
            Ok((next, tmp)) => prepared.push((old.clone(), next, tmp)),
            Err((code, message)) => {
                let _ = std::fs::remove_dir_all(upgrade_tmp_dir());
                return ApiResult::err(code, format!("没换成，旧版都还在：{message}"));
            }
        }
    }
    if prepared.is_empty() {
        return ApiResult::err("already_latest", "你装的 mod 都是最新的了");
    }

    let mut next_records = records.clone();
    for (_, next, _) in &prepared {
        if let Some(slot) = next_records.iter_mut().find(|r| r.id == next.id) {
            *slot = next.clone();
        }
    }

    match rebuild_from(next_records).await {
        Ok(pak) => {
            let mut out = Vec::new();
            for (old, next, tmp) in &prepared {
                if let Err((code, message)) = commit_upgrade(old, next, tmp) {
                    return ApiResult::err(
                        code,
                        format!("合并包已经换好了，但有一条记账没写成功：{message}"),
                    );
                }
                out.push(InstallOutcome {
                    id: next.id,
                    name: next.name.clone(),
                    installed: true,
                    pak: pak.clone(),
                });
            }
            ApiResult::ok(out)
        }
        Err(message) => {
            let _ = std::fs::remove_dir_all(upgrade_tmp_dir());
            ApiResult::err("integrate", format!("没换成，旧版都还在：{message}"))
        }
    }
}

/// 导入本地文件：.pak、装着 .pak 的 .zip，或者解包后的文件夹。
///
/// 认下来之后和从 mod.io 装的走同一条路：复制到自己目录 → 记账 → 重建合并包。
/// 所以「断网也能装东西」就是这么来的——这一步完全不碰网络。
///
/// 带 `main.dll` / `main.lua` / `main.js` 的是**脚本类**，走另一条路（`ue4ss\Mods\<名字>\`），
/// 不进合并包。`replace` / `dir_name` 只在脚本类撞上同名目录时用得到。
#[tauri::command]
pub async fn import_local(
    path: String,
    replace: Option<bool>,
    dir_name: Option<String>,
) -> ApiResult<InstallOutcome> {
    if let Some((code, message)) = blocked_by_running_game() {
        return ApiResult::err(code, message);
    }
    snapshot_before("导入之前");

    match import_script(Path::new(&path), replace.unwrap_or(false), dir_name) {
        Ok(Some(outcome)) => return ApiResult::ok(outcome),
        Ok(None) => {}
        Err((code, message)) => return ApiResult::err(code, message),
    }

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
            kind: "pak".to_owned(),
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

/// 脚本类 mod 的导入：解出来 → 落到 `ue4ss\Mods\<名字>\`，不进合并包。
///
/// 返回 `Ok(None)` 表示「这东西不是脚本类」，交给上层按数据包那条路走。
fn import_script(
    src: &Path,
    replace: bool,
    dir_name: Option<String>,
) -> Result<Option<InstallOutcome>, (String, String)> {
    if !src.exists() {
        return Err(("not_found".to_owned(), format!("找不到这个路径：{}", src.display())));
    }

    let is_zip = src.is_file()
        && src
            .extension()
            .map(|e| e.to_string_lossy().eq_ignore_ascii_case("zip"))
            .unwrap_or(false);
    let script = if src.is_dir() {
        crate::ue4ss_mods::dir_looks_script(src)
    } else if is_zip {
        let entries =
            crate::ue4ss_mods::zip_entries(src).map_err(|e| (e.code.to_owned(), e.message))?;
        crate::ue4ss_mods::looks_script(&entries)
    } else {
        false
    };
    if !script {
        return Ok(None);
    }

    let raw_name = src
        .file_stem()
        .or_else(|| src.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "script-mod".to_owned());
    let fp = crate::local::fingerprint(src).map_err(|e| ("import".to_owned(), e))?;
    let id = fp.id;

    // 留一份在我们自己目录里：卸载时按记账走，重装时也不用再问用户要文件
    let staged = paths::mods_cache_dir()
        .join("local")
        .join(format!("{}-{}.script", crate::ue4ss_mods::safe_dir_name(&raw_name), id % 1_000_000));
    if staged.exists() {
        let _ = std::fs::remove_dir_all(&staged);
    }
    if is_zip {
        crate::ue4ss_mods::extract_zip(src, &staged).map_err(|e| (e.code.to_owned(), e.message))?;
    } else {
        crate::ue4ss_install::copy_into(src, &staged).map_err(|e| (e.code.to_owned(), e.message))?;
    }
    let content = crate::ue4ss_mods::content_root(&staged);

    let Some(game) = game::find_installation() else {
        return Err(("no_game".to_owned(), "没找到游戏，装不了脚本类 mod".to_owned()));
    };
    let win64 = game.binaries_directory();
    let wanted = dir_name
        .map(|n| crate::ue4ss_mods::safe_dir_name(&n))
        .unwrap_or_else(|| crate::ue4ss_mods::safe_dir_name(&raw_name));

    // 撞名时如果换了名字，按新名字登记，界面上一眼能对上
    let entry = crate::ue4ss_mods::install(&win64, &paths::data_root(), id, &wanted, &content, replace)
        .map_err(|e| (e.code.to_owned(), e.message))?;

    let filename = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| entry.name.clone());
    let size = std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);
    let dir = records_dir();
    let installed_at = load_records(&dir)
        .into_iter()
        .find(|r| r.id == id)
        .and_then(|r| r.installed_at)
        .or_else(|| Some(now_ms()));

    save_record(
        &dir,
        &InstalledRecord {
            id,
            name: entry.name.clone(),
            file_id: 0,
            filename,
            size,
            path: staged.display().to_string(),
            source: "local".to_owned(),
            original_path: Some(src.display().to_string()),
            content_hash: Some(fp.hash.clone()),
            missing: false,
            installed_at,
            kind: "script".to_owned(),
        },
    )
    .map_err(|e| (e.code().to_owned(), e.message()))?;

    Ok(Some(InstallOutcome {
        id,
        name: entry.name,
        installed: true,
        pak: None,
    }))
}

/// 卸载：删下载的文件和记账，然后重建合并包（没剩下就清干净）。
#[tauri::command]
pub async fn uninstall_mod(id: u64) -> ApiResult<InstallOutcome> {
    if let Some((code, message)) = blocked_by_running_game() {
        return ApiResult::err(code, message);
    }
    snapshot_before("卸载之前");
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

    // 脚本类：先把游戏里那个目录撤掉（只删我们登记的），再删本地留的那份
    if rec.kind == "script" {
        if let Err(e) = crate::ue4ss_mods::uninstall(&paths::data_root(), rec.id) {
            return ApiResult::err(e.code, e.message);
        }
        let _ = std::fs::remove_dir_all(&rec.path);
    } else {
        let _ = std::fs::remove_file(&rec.path);
    }
    if let Err(e) = std::fs::remove_file(&path) {
        return ApiResult::err("io", format!("删记账文件失败：{e}"));
    }

    // 脚本类不影响合并包，但剩下的数据包 mod 可能还等着重建（比如刚删的是最后一个 pak）
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
    let win64 = crate::game::find_installation().map(|g| g.binaries_directory());
    let list = records
        .iter()
        .map(|r| {
            let mut item = r.to_installed();
            // 脚本类要现读一次日志判「这次起来没有」——不许缓存结论，
            // 游戏更新恰恰发生在我们没动启动器的时候（计划 §0 第 32 条）。
            if item.kind == "script" {
                if let Some(win64) = win64.as_deref() {
                    let health = crate::ue4ss_health::check_mod(win64, &r.name, r.installed_at);
                    item.script_state = health.state;
                    item.script_detail = health.detail;
                }
            }
            item
        })
        .collect();
    ApiResult::ok(list)
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
            kind: "pak".to_owned(),
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
            description_plaintext: String::new(),
            downloads: 1,
            version: "1.0".to_owned(),
            logo_320: String::new(),
            logo_640: String::new(),
            creator: String::new(),
            date_updated: 0,
            tags: Vec::new(),
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

    #[test]
    fn 全部更新只挑从modio装的那些() {
        let mut modio = sample_record();
        modio.file_id = 777;

        let mut local = sample_record();
        local.id = 202;
        local.source = "local".to_owned();

        let got = upgrade_candidates(&[modio, local]);
        assert_eq!(got.len(), 1, "本地导入的包没有在线版本可问");
        assert_eq!(got[0].id, 101);
    }

    #[test]
    fn 没记文件id的老记账不参与升级() {
        let mut old = sample_record();
        old.file_id = 0;

        assert!(upgrade_candidates(&[old]).is_empty(), "不知道装的是哪个文件，就比不出新旧");
    }

    #[test]
    fn 文件没了的和脚本类都不进升级列表() {
        let mut missing = sample_record();
        missing.file_id = 888;
        missing.missing = true;

        let mut script = sample_record();
        script.id = 303;
        script.file_id = 999;
        script.kind = "script".to_owned();

        let got = upgrade_candidates(&[missing, script]);
        assert!(got.is_empty(), "缺文件的先修「文件没了」，脚本类走 UE4SS 那条路");
    }
}
