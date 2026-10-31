//! 岩笋的后端逻辑都在这个库里，`main.rs` 只是启动器，`bin/peek.rs` 是命令行自测工具。
//!
//! 拆成库 + 两个可执行文件，是为了能不开界面就测接口（`cargo run --bin peek -- 关键词`）。

pub mod audit;
pub mod cache;
pub mod config;
pub mod game;
pub mod game_state;
pub mod integrate;
pub mod ipc;
pub mod legacy;
pub mod local;
pub mod logging;
pub mod manifest;
pub mod migrate;
pub mod mod_lints;
pub mod modio;
pub mod paklist;
pub mod paths;
pub mod proc;
pub mod providers;
pub mod secret;
pub mod settings;
pub mod snapshot;
pub mod steam;
pub mod token;
pub mod translate;
pub mod ue4ss;
pub mod ui_state;
pub mod webview2;

/// 启动桌面窗口。
pub fn run() {
    logging::init();
    // 界面要靠 WebView2 才能显示出来，缺了就先把话说明白再退出（见 webview2.rs）
    webview2::ensure_available();
    prepare_webview_dir();
    migrate_old_records();
    clean_stale_paks();

    let result = tauri::Builder::default()
        // 选文件/选文件夹的对话框（导入本地 mod、导入配置要用）
        .plugin(tauri_plugin_dialog::init())
        // 这里注册的每个名字，都要在 ipc 下有一个同名函数
        .invoke_handler(tauri::generate_handler![
            ipc::mods::search_mods,
            ipc::mods::list_mods,
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
            ipc::snapshots::list_snapshots,
            ipc::snapshots::create_snapshot,
            ipc::snapshots::rollback_snapshot,
            ipc::snapshots::delete_snapshot,
            ipc::settings::audit_game,
            ipc::settings::fix_audit_issue,
            ipc::settings::data_location,
            ipc::settings::open_data_dir,
            ipc::settings::legacy_data,
            ipc::settings::resolve_legacy,
            ipc::settings::open_url,
            ipc::game::launch_game,
            ipc::game::game_status,
            ipc::settings::token_status,
            ipc::settings::save_token,
            ipc::settings::clear_token,
            ipc::settings::export_config,
            ipc::settings::import_config,
            ipc::settings::diagnostics,
            ipc::today::today_digest,
            ipc::today::ui_state,
            ipc::today::save_ui_state,
            ipc::translate::translate_detail,
            ipc::translate::translate_text,
            ipc::translate::get_translate_settings,
            ipc::translate::set_translate_settings,
            ipc::ue4ss::ue4ss_status,
            ipc::ue4ss::ue4ss_prepare,
            ipc::ue4ss::ue4ss_cancel,
        ])
        .run(tauri::generate_context!());

    if let Err(err) = result {
        tracing::error!(%err, "岩笋启动失败");
        eprintln!("岩笋启动失败：{err}");
    }
}

/// 指定 WebView2 把界面缓存放哪儿，并把绝对路径写进日志。
///
/// 为什么不靠安装器塞环境变量：注册表里那种 `%LOCALAPPDATA%\...` 的字面值
/// 拿回来还要自己展开，多一层就多一种出错方式。这里由程序自己按同样的规则算，
/// 算出来是绝对路径，写日志、卸载、下次启动重建都用同一个值。
///
/// 位置跟 WebView2 的默认位置一致（`%LOCALAPPDATA%\com.rocksprout.client`），
/// 所以老用户不会因为这次改动丢掉缓存、也不需要重新登录什么。
fn prepare_webview_dir() {
    #[cfg(windows)]
    {
        let Some(base) = std::env::var_os("LOCALAPPDATA") else {
            tracing::warn!("找不到 LOCALAPPDATA，WebView2 缓存目录交给运行时自己定");
            return;
        };
        let dir = std::path::PathBuf::from(base).join("com.rocksprout.client");
        // SAFETY: 这里还在 main 的启动阶段，Tauri 的线程都还没起来，改环境变量不会跟别人抢。
        // 设这个变量是 WebView2 官方认的指定缓存目录的方式。
        unsafe { std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", &dir) };
        tracing::info!(dir = %dir.display(), "WebView2 缓存目录");
    }
}

/// 清理上次没写完留下的 `mods_P.pak.tmp` / `.prev`（见 [`integrate::clean_stale`]）。
/// 清不掉不影响启动：那只是游戏目录里多两个文件，下次装 mod 时还会再试。
fn clean_stale_paks() {    let Some(installation) = game::find_installation() else {
        return;
    };
    match integrate::clean_stale(&installation.paks_path()) {
        Ok(0) => {}
        Ok(n) => tracing::info!(cleaned = n, "清掉了上次留下的临时文件"),
        Err(err) => tracing::warn!(%err, "清理上次的临时文件没做成"),
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
