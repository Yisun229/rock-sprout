//! 岩笋的后端逻辑都在这个库里，`main.rs` 只是启动器，`bin/peek.rs` 是命令行自测工具。
//!
//! 拆成库 + 两个可执行文件，是为了能不开界面就测接口（`cargo run --bin peek -- 关键词`）。

pub mod game;
pub mod integrate;
pub mod ipc;
pub mod logging;
pub mod mod_lints;
pub mod modio;
pub mod paklist;
pub mod paths;
pub mod providers;

/// 启动桌面窗口。
pub fn run() {
    logging::init();

    let result = tauri::Builder::default()
        // 这里注册的每个名字，都要在 ipc 下有一个同名函数
        .invoke_handler(tauri::generate_handler![
            ipc::mods::search_mods,
            ipc::mods::get_mod_detail,
            ipc::mods::install_mod,
            ipc::mods::uninstall_mod,
            ipc::mods::list_installed,
            ipc::game::launch_game,
            ipc::settings::token_status,
            ipc::settings::save_token,
            ipc::settings::clear_token,
        ])
        .run(tauri::generate_context!());

    if let Err(err) = result {
        tracing::error!(%err, "岩笋启动失败");
        eprintln!("岩笋启动失败：{err}");
    }
}
