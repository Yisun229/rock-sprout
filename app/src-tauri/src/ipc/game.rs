//! 游戏启动相关的命令。
//!
//! 走 `steam://rungameid/548430`（548430 是《深岩银河》的 Steam appid）。
//! 这样不用自己拼游戏 exe 路径，Steam 自己的启动参数也照旧生效。

use serde::Serialize;

use super::ApiResult;

const DRG_STEAM_APP_ID: u32 = 548_430;

#[derive(Debug, Serialize)]
pub struct LaunchOutcome {
    pub requested: bool,
    pub message: String,
}

pub fn launch_game_impl() -> ApiResult<LaunchOutcome> {
    let url = format!("steam://rungameid/{DRG_STEAM_APP_ID}");

    // Windows 上最省事的做法就是交给系统打开这个协议链接
    #[cfg(target_os = "windows")]
    let spawned = std::process::Command::new("cmd")
        .args(["/C", "start", "", &url])
        .spawn();

    #[cfg(not(target_os = "windows"))]
    let spawned = std::process::Command::new("xdg-open").arg(&url).spawn();

    match spawned {
        Ok(_) => ApiResult::ok(LaunchOutcome {
            requested: true,
            message: "已让 Steam 启动游戏，等它自己起来就行".to_owned(),
        }),
        Err(e) => ApiResult::err(
            "launch_failed",
            format!("叫不动 Steam：{e}。手动在 Steam 里点一下开始也一样"),
        ),
    }
}

#[tauri::command]
pub fn launch_game() -> ApiResult<LaunchOutcome> {
    tracing::info!("launch_game");
    launch_game_impl()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 启动地址用的是深岩银河的appid() {
        assert_eq!(DRG_STEAM_APP_ID, 548_430);
    }
}
