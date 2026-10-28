//! 维护类命令：只动记账、不碰游戏目录。
//!
//! 现在只有一个「仅删记录」：那条记账指的文件早就没了（用户自己删的、挪走的），
//! 界面上会标灰，给这个按钮把这条空记录去掉。**不删任何文件，也不重建合并包。**

use super::ApiResult;
use crate::ipc::mods::{load_records, record_path, records_dir};
use crate::paths;

/// 只删掉这一条记账，别的不动。
#[tauri::command]
pub fn delete_record_only(id: u64) -> ApiResult<String> {
    let dir = records_dir();
    let Some(rec) = load_records(&dir).into_iter().find(|r| r.id == id) else {
        return ApiResult::err("not_installed", "这个 mod 不在已装列表里");
    };
    if !rec.missing {
        // 文件还在的话不该走这条路——免得用户以为「删了」，其实文件还躺在缓存里
        return ApiResult::err(
            "still_installed",
            "这个 mod 的文件还在，用普通的卸载就行（那次会顺手清掉合并包）",
        );
    }
    match std::fs::remove_file(record_path(&dir, id)) {
        Ok(()) => {
            tracing::info!("删掉了空记账 {}", paths::data_root().display());
            ApiResult::ok(rec.name)
        }
        Err(e) => ApiResult::err("io", format!("记账删不掉：{e}")),
    }
}
