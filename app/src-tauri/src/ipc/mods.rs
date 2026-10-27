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
struct InstalledRecord {
    id: u64,
    name: String,
    file_id: u64,
    filename: String,
    size: u64,
    path: String,
}

impl InstalledRecord {
    fn to_installed(&self) -> InstalledMod {
        InstalledMod {
            id: self.id,
            name: self.name.clone(),
            version: self.filename.clone(),
        }
    }
}

fn records_dir() -> PathBuf {
    paths::mods_cache_dir().join("installed")
}

fn record_path(dir: &Path, id: u64) -> PathBuf {
    dir.join(format!("{id}.json"))
}

fn save_record(dir: &Path, rec: &InstalledRecord) -> Result<(), ModioError> {
    std::fs::create_dir_all(dir).map_err(|e| ModioError::Io(e.to_string()))?;
    let text = serde_json::to_string_pretty(rec).map_err(|e| ModioError::Io(e.to_string()))?;
    std::fs::write(record_path(dir, rec.id), text).map_err(|e| ModioError::Io(e.to_string()))
}

fn load_records(dir: &Path) -> Vec<InstalledRecord> {
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

#[tauri::command]
pub async fn search_mods(query: String, page: u32) -> ApiResult<SearchPage> {
    to_api(async {
        let c = client()?;
        c.search(&query, page).await
    }
    .await)
}

#[tauri::command]
pub async fn get_mod_detail(id: u64) -> ApiResult<ModDetail> {
    to_api(async {
        let c = client()?;
        c.detail(id).await
    }
    .await)
}

/// mod.io 上这个 mod 的页面地址。pak 里记的「来源」就是它，不用另存字段。
fn mod_page_url(id: u64) -> String {
    format!("https://mod.io/g/drg/m/{id}")
}

/// 把「已下载并记账」的那批 mod 翻成合并器认识的输入。
fn pending_mods() -> Vec<game::ModToInstall> {
    load_records(&records_dir())
        .into_iter()
        .map(|r| game::ModToInstall {
            name: r.name.clone(),
            url: mod_page_url(r.id),
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

/// 下载 → 记账 → 合并进游戏。
#[tauri::command]
pub async fn install_mod(id: u64) -> ApiResult<InstallOutcome> {
    let c = match client() {
        Ok(c) => c,
        Err(e) => return ApiResult::err(e.code(), e.message()),
    };

    let detail = match c.detail(id).await {
        Ok(d) => d,
        Err(e) => return ApiResult::err(e.code(), e.message()),
    };
    let latest = match c.files(id).await {
        Ok(files) => match files.into_iter().next() {
            Some(f) => f,
            None => {
                return ApiResult::err("no_file", "这个 mod 没有可下载的文件");
            }
        },
        Err(e) => return ApiResult::err(e.code(), e.message()),
    };

    let outcome = match c.download(&latest, &paths::mods_cache_dir()).await {
        Ok(o) => o,
        Err(e) => return ApiResult::err(e.code(), e.message()),
    };

    if let Err(e) = save_record(
        &records_dir(),
        &InstalledRecord {
            id: detail.id,
            name: detail.name.clone(),
            file_id: latest.id,
            filename: outcome.filename.clone(),
            size: outcome.size,
            path: outcome.path.clone(),
        },
    ) {
        return ApiResult::err(e.code(), e.message());
    }

    match rebuild_pak().await {
        Ok(pak) => ApiResult::ok(InstallOutcome {
            id: detail.id,
            name: detail.name,
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

#[tauri::command]
pub async fn list_installed() -> ApiResult<Vec<InstalledMod>> {
    let records = load_records(&records_dir());
    ApiResult::ok(records.iter().map(|r| r.to_installed()).collect())
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
}
