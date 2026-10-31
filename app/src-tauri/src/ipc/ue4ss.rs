//! UE4SS 运行时的界面命令：看状态、下载、取消。
//!
//! 界面上要的是一个能边下边看的进度，而不是一个卡住三分钟的按钮。所以这里的做法是：
//! 下载跑在后台任务里，进度存在一个进程内的小盒子里（[`Progress`]），
//! 前端每 300 ms 问一次 [`ue4ss_status`] 就把进度读走了——不需要事件通道，也不会丢进度。

use std::sync::OnceLock;

use serde::Serialize;

use super::ApiResult;
use crate::paths;
use crate::ue4ss::{self, Progress};

static PROGRESS: OnceLock<Progress> = OnceLock::new();

fn progress() -> &'static Progress {
    PROGRESS.get_or_init(Progress::default)
}

#[derive(Debug, Serialize)]
pub struct Ue4ssStatus {
    /// 钉死的版本号（界面直接显示）
    pub version: String,
    /// 官方下载地址（手动下载时给用户）
    pub url: String,
    /// 压缩包应有的字节数
    pub zip_size: u64,
    /// 压缩包现在在哪儿（不管有没有）
    pub zip_path: String,
    /// 压缩包已经在本地、而且核对过
    pub zip_ready: bool,
    /// 手动下载时把 zip 放到这个目录
    pub manual_dir: String,
    /// 解出来的包齐了没有
    pub package_ready: bool,
    /// 解出来的包放在哪儿
    pub package_dir: String,
    /// 正在下载
    pub downloading: bool,
    /// 已下多少字节（`downloading` 为真时有意义）
    pub downloaded: u64,
}

fn snapshot() -> Ue4ssStatus {
    let root = paths::data_root();
    let zip = ue4ss::zip_path(&root);
    Ue4ssStatus {
        version: ue4ss::VERSION.to_owned(),
        url: ue4ss::DOWNLOAD_URL.to_owned(),
        zip_size: ue4ss::ZIP_SIZE,
        zip_path: zip.display().to_string(),
        zip_ready: zip.is_file() && ue4ss::verify_zip(&zip).is_ok(),
        manual_dir: ue4ss::manual_hint(&root),
        package_ready: ue4ss::package_ready(&root),
        package_dir: ue4ss::package_dir(&root).display().to_string(),
        downloading: progress().running(),
        downloaded: progress().done(),
    }
}

/// 现在的状态。纯读，不碰网络，界面可以随便轮询。
#[tauri::command]
pub fn ue4ss_status() -> ApiResult<Ue4ssStatus> {
    ApiResult::ok(snapshot())
}

/// 备齐 UE4SS 的包：本地有就解，没有就下载；已备齐就直接返回。
///
/// 失败一律是中文一句 + 稳定 code，界面据此决定给不给「手动下载」入口。
#[tauri::command]
pub async fn ue4ss_prepare() -> ApiResult<Ue4ssStatus> {
    let root = paths::data_root();
    if ue4ss::package_ready(&root) {
        return ApiResult::ok(snapshot());
    }
    if progress().running() {
        return ApiResult::err("busy", "正在下载，等它下完再说");
    }

    let zip = ue4ss::zip_path(&root);
    // 本地已经有（可能是用户手动放的）就只核对+解包，不联网
    let has_local = zip.is_file();
    if !has_local {
        if let Err(err) = ue4ss::download(&root, progress()).await {
            return ApiResult::err(err.code, err.message);
        }
    } else if let Err(err) = ue4ss::verify_zip(&zip) {
        return ApiResult::err(err.code, err.message);
    }

    // 解包是同步 IO，别占着异步线程
    let result = tauri::async_runtime::spawn_blocking(move || {
        let root = paths::data_root();
        ue4ss::extract(&ue4ss::zip_path(&root), &ue4ss::package_dir(&root))
    })
    .await;

    match result {
        Ok(Ok(_)) => ApiResult::ok(snapshot()),
        Ok(Err(err)) => ApiResult::err(err.code, err.message),
        Err(err) => ApiResult::err("extract_io", format!("解包任务没跑起来：{err}")),
    }
}

/// 取消正在进行的下载。没在下载时返回 `false`（不算错）。
#[tauri::command]
pub fn ue4ss_cancel() -> ApiResult<bool> {
    if !progress().running() {
        return ApiResult::ok(false);
    }
    progress().request_cancel();
    ApiResult::ok(true)
}
