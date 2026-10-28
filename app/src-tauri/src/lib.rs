//! 岩笋的后端逻辑都在这个库里，`main.rs` 只是启动器，`bin/peek.rs` 是命令行自测工具。
//!
//! 拆成库 + 两个可执行文件，是为了能不开界面就测接口（`cargo run --bin peek -- 关键词`）。

pub mod cache;
pub mod config;
pub mod game;
pub mod integrate;
pub mod ipc;
pub mod local;
pub mod logging;
pub mod manifest;
pub mod migrate;
pub mod mod_lints;
pub mod modio;
pub mod paklist;
pub mod paths;
pub mod providers;

/// 启动桌面窗口。
pub fn run() {
    logging::init();
    migrate_old_records();

    let result = tauri::Builder::default()
        // 选文件/选文件夹的对话框（导入本地 mod、导入配置要用）
        .plugin(tauri_plugin_dialog::init())
        // 这里注册的每个名字，都要在 ipc 下有一个同名函数
        .invoke_handler(tauri::generate_handler![
            ipc::mods::search_mods,
            ipc::mods::get_mod_detail,
            ipc::mods::install_mod,
            ipc::mods::download_mod,
            ipc::mods::apply_all,
            ipc::mods::import_local,
            ipc::mods::uninstall_mod,
            ipc::mods::list_installed,
            ipc::mods::precheck_install,
            ipc::mods::scan_conflicts,
            ipc::maintenance::delete_record_only,
            ipc::game::launch_game,
            ipc::settings::token_status,
            ipc::settings::save_token,
            ipc::settings::clear_token,
            ipc::settings::export_config,
            ipc::settings::import_config,
            ipc::settings::diagnostics,
        ])
        .run(tauri::generate_context!());

    if let Err(err) = result {
        tracing::error!(%err, "岩笋启动失败");
        eprintln!("岩笋启动失败：{err}");
    }
}

/// 把 v0.1 的本地记账迁到按内容算 id 的新格式（只跑一次，见 [`migrate`]）。
///
/// 迁移失败不影响启动：记账还是老格式，界面照样能用，只是本地 mod 可能显示成两条。
fn migrate_old_records() {
    match migrate::run_if_needed(&paths::data_root()) {
        Ok(Some(report)) => tracing::info!(summary = %report.summary(), "老记账迁移完成"),
        Ok(None) => {}
        Err(err) => tracing::warn!(%err, "老记账迁移没做成，先按老格式接着用"),
    }
}
