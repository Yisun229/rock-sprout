// Windows 上正式版不弹黑色控制台窗口；带 `pnpm tauri dev` 调试时保留输出。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    rock_sprout::run();
}
