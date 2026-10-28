//! 快照相关的命令：列出、拍一份、回滚、删掉。
//!
//! 真正的活都在 [`crate::snapshot`] 里，这里只做「翻译错误 + 包成 ApiResult」。

use super::ApiResult;
use crate::snapshot;

/// 列出所有快照，新的在前。
#[tauri::command]
pub fn list_snapshots() -> ApiResult<Vec<snapshot::Snapshot>> {
    match snapshot::list() {
        Ok(list) => ApiResult::ok(list),
        Err(e) => ApiResult::err(error_code(&e), e),
    }
}

/// 手动存一个保存点（不会被自动清理）。
#[tauri::command]
pub fn create_snapshot(reason: Option<String>) -> ApiResult<snapshot::Snapshot> {
    let reason = reason.unwrap_or_else(|| "手动保存点".to_owned());
    match snapshot::create(&reason, true) {
        Ok(snap) => ApiResult::ok(snap),
        Err(e) => ApiResult::err(error_code(&e), e),
    }
}

/// 退回到某份快照。
#[tauri::command]
pub fn rollback_snapshot(id: String) -> ApiResult<snapshot::RollbackOutcome> {
    if let Err(message) = crate::proc::guard() {
        return ApiResult::err("game_running", message);
    }
    match snapshot::rollback(&id) {
        Ok(outcome) => ApiResult::ok(outcome),
        Err(e) => ApiResult::err(error_code(&e), e),
    }
}

/// 删掉某份快照。
#[tauri::command]
pub fn delete_snapshot(id: String) -> ApiResult<String> {
    match snapshot::delete(&id) {
        Ok(()) => ApiResult::ok(id),
        Err(e) => ApiResult::err(error_code(&e), e),
    }
}

/// 把快照模块抛上来的中文错误分成几类，前端好决定给什么建议。
///
/// 错误串是自家写的，所以这里按关键字判断就够（不去解析结构）。
fn error_code(message: &str) -> String {
    if message.contains("找不到这份快照") {
        "not_found".to_owned()
    } else if message.contains("没找到《深岩银河》") {
        "no_game".to_owned()
    } else if message.contains("拒绝访问") || message.contains("os error 5") {
        "locked".to_owned()
    } else {
        "snapshot".to_owned()
    }
}
