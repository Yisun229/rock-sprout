//! 启动前先看一眼：这台机器上有没有 WebView2 运行时。
//!
//! Tauri 的界面是 WebView2 渲染的。Windows 10/11 一般自带，但精简版系统或
//! 很久没更新的机器可能没有——那种情况下直接起窗口只会白屏或者干脆什么都不显示，
//! 用户完全看不出发生了什么。所以这里先查一下：没有就弹个中文说明，带他去装，
//! 然后正常退出，而不是让他对着一片空白猜。
//!
//! 查的是 WebView2 自己写在注册表里的版本号（EdgeUpdate 的 Clients 键），
//! 64 位系统上它在 WOW6432Node 下面，有的机器会写到 HKCU（按用户安装的那种）。
//! 直接调 Win32 API，不为了这一件事多引一个 crate。

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegGetValueW(
            hkey: isize,
            sub_key: *const u16,
            value: *const u16,
            flags: u32,
            kind: *mut u32,
            data: *mut c_void,
            data_size: *mut u32,
        ) -> i32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(hwnd: isize, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }

    pub const HKEY_LOCAL_MACHINE: isize = 0x8000_0002u32 as isize;
    pub const HKEY_CURRENT_USER: isize = 0x8000_0001u32 as isize;
    const RRF_RT_REG_SZ: u32 = 0x0000_0002;
    const MB_OK: u32 = 0x0000_0000;
    const MB_ICONWARNING: u32 = 0x0000_0030;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// 读一个字符串值；读不到返回 None（键不存在、值不存在、类型不对都算 None）。
    pub fn read_string(hkey: isize, sub_key: &str, value: &str) -> Option<String> {
        let sub = wide(sub_key);
        let val = wide(value);
        let mut buf = [0u16; 256];
        let mut size = (buf.len() * std::mem::size_of::<u16>()) as u32;
        let rc = unsafe {
            RegGetValueW(
                hkey,
                sub.as_ptr(),
                val.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut c_void,
                &mut size,
            )
        };
        if rc != 0 {
            return None;
        }
        let len = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        String::from_utf16(&buf[..len]).ok()
    }

    /// 弹一个只有一个「确定」的中文提示窗。用户点掉它才会继续。
    pub fn alert(title: &str, text: &str) {
        let t = wide(title);
        let m = wide(text);
        unsafe {
            MessageBoxW(0, m.as_ptr(), t.as_ptr(), MB_OK | MB_ICONWARNING);
        }
    }
}

/// WebView2 的下载页（微软官方的常青链接，装的是 Evergreen Runtime）。
pub const DOWNLOAD_URL: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";

/// WebView2 客户端在注册表里的位置（三个键位都要试）。
const CLIENT_KEY: &str =
    "SOFTWARE\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
const CLIENT_KEY_WOW: &str =
    "SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

/// 注册表里读到的版本号算不算「装了」。
///
/// 微软在没装的机器上也可能留下一个 `0.0.0.0`，所以不能只看键在不在。
pub fn version_is_usable(raw: &str) -> bool {
    let v = raw.trim();
    if v.is_empty() {
        return false;
    }
    // 任何一段是非零数字就算装上了（0.0.0.0 这种全零的不算）
    v.split('.').any(|part| part.parse::<u32>().map(|n| n > 0).unwrap_or(false))
}

/// 查出装了哪个版本的 WebView2；没装返回 None。
#[cfg(windows)]
pub fn installed_version() -> Option<String> {
    for hkey in [win::HKEY_LOCAL_MACHINE, win::HKEY_CURRENT_USER] {
        for key in [CLIENT_KEY_WOW, CLIENT_KEY] {
            if let Some(raw) = win::read_string(hkey, key, "pv") {
                if version_is_usable(&raw) {
                    return Some(raw.trim().to_owned());
                }
            }
        }
    }
    None
}

#[cfg(not(windows))]
pub fn installed_version() -> Option<String> {
    // 非 Windows 上没有 WebView2 这回事（那是 Linux/macOS 的 WebKit），不用挡
    Some("n/a".to_owned())
}

/// 用系统默认浏览器打开一个网址。
fn open_in_browser(url: &str) {
    #[cfg(windows)]
    let spawned = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();

    #[cfg(not(windows))]
    let spawned = std::process::Command::new("xdg-open").arg(url).spawn();

    if let Err(e) = spawned {
        tracing::warn!(url, error = %e, "打不开浏览器");
    }
}

/// 启动前查一次：没有 WebView2 就说明原因、带他去下载，然后退出（退出码 1）。
///
/// 有的话什么都不做，正常往下走。
pub fn ensure_available() {
    let Some(version) = installed_version() else {
        tracing::warn!("没找到 WebView2 运行时，界面起不来");
        #[cfg(windows)]
        {
            win::alert(
                "岩笋 · Rock Sprout 缺少一个组件",
                "这台电脑上没装 WebView2 运行时，界面显示不出来。\n\n\
                 点「确定」会用浏览器打开微软的下载页，装好那一个小程序之后再双击岩笋就好。\n\
                 （Windows 10/11 一般自带这个组件，只有精简系统或者很久没更新的机器会缺。）",
            );
            open_in_browser(DOWNLOAD_URL);
        }
        #[cfg(not(windows))]
        eprintln!("缺少 WebView2 运行时，界面显示不出来。请先安装：{DOWNLOAD_URL}");
        std::process::exit(1);
    };
    tracing::info!(version = %version, "WebView2 就绪");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 全零版本号当成没装() {
        assert!(!version_is_usable("0.0.0.0"));
        assert!(!version_is_usable("   "));
        assert!(!version_is_usable(""));
    }

    #[test]
    fn 正常的版本号认作已装() {
        assert!(version_is_usable("156.0.4314.50"));
        assert!(version_is_usable("120.0.2210.91"));
        assert!(version_is_usable(" 109.0.1518.78 "));
    }

    #[test]
    fn 下载页是微软官方的常青链接() {
        assert!(DOWNLOAD_URL.starts_with("https://go.microsoft.com/"));
    }
}
