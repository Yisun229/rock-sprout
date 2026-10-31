//! 主窗口的导航拦截。
//!
//! 点一个漏网的 `<a href="https://…">`，WebView 会自己把整个应用界面换成一个网页，
//! 而且没有返回入口——用户看到的是白屏，不知道在加载还是卡死。
//!
//! 这里在窗口层面兜底：除应用自己的页面，任何地址一律不许在窗口里打开，
//! 转而交给系统浏览器。这样将来谁再写一个 `<a>`，也带不走应用。

use tauri::Url;

/// 这个地址是不是应用自己的页面。
///
/// - 生产环境从 `tauri://localhost` 或 `http://tauri.localhost` 加载界面；
/// - 开发环境从 vite 的 `http://localhost:1420` 加载（只在 debug 构建里放行）；
/// - 前后端通信走 `ipc://localhost`（Windows 上是 `http://ipc.localhost`），
///   这不是页面导航，但宁可放行也不要误伤。
pub fn is_app_url(url: &Url) -> bool {
    match url.scheme() {
        "tauri" | "ipc" | "asset" => true,
        "http" | "https" => match url.host_str() {
            Some("tauri.localhost") | Some("ipc.localhost") => true,
            // 开发模式下 vite 的地址；正式包里这段判断根本不会成立
            Some("localhost") if cfg!(debug_assertions) => url.port() == Some(1420),
            _ => false,
        },
        _ => false,
    }
}

/// `on_navigation` 的回调：返回 true 让窗口自己加载，false 拦下来。
///
/// 拦下来的同时交系统浏览器，用户想看的那个页面还是会打开，只是不在应用里打开。
pub fn handle(url: &Url) -> bool {
    if is_app_url(url) {
        return true;
    }

    let text = url.as_str().to_owned();
    tracing::warn!(url = %text, "主窗口想离开应用，转交系统浏览器");
    if let Err(e) = crate::ipc::settings::open_external(&text) {
        tracing::warn!(url = %text, %e, "系统浏览器没打开");
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).expect("测试里的地址应该是合法的")
    }

    #[test]
    fn 生产环境的界面地址算应用自己的() {
        assert!(is_app_url(&url("http://tauri.localhost/")));
        assert!(is_app_url(&url("http://tauri.localhost/index.html?a=1")));
        assert!(is_app_url(&url("tauri://localhost/")));
    }

    #[test]
    fn 前后端通信的地址不算跳走() {
        assert!(is_app_url(&url("ipc://localhost/something")));
        assert!(is_app_url(&url("http://ipc.localhost/something")));
    }

    #[test]
    fn 外面的网址一律不放行() {
        assert!(!is_app_url(&url("https://mod.io/me/access")));
        assert!(!is_app_url(&url("http://example.com/")));
        assert!(!is_app_url(&url("file:///C:/Windows/win.ini")));
        assert!(!is_app_url(&url("mailto:someone@example.com")));
    }

    #[test]
    fn 别的本机端口骗不过去() {
        // 只看主机名容易把 localhost 上的别的服务也放进来，端口要一起看
        assert!(!is_app_url(&url("http://localhost:9999/")));
    }
}
