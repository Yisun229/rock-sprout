// 单实例：一个 Windows 命名互斥量。
//
// 为什么需要：两个实例同时改 `mods_P.pak` 会互相踩，最坏的结果是合并包损坏。
// 进程内的锁挡不住这件事——那是两个进程。所以启动最早期（还没碰任何文件之前）
// 先抢一个命名互斥量，抢不到就说明已经有一个在跑，提示一句就退出。
//
// 不做「把第一个窗口提到前面」这类增强：那要跨进程通信，收益不值这个复杂度。
#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    /// ERROR_ALREADY_EXISTS：互斥量已经存在，说明另一个实例开着
    const ERROR_ALREADY_EXISTS: u32 = 183;
    const MB_OK: u32 = 0x0000_0000;
    const MB_ICONINFORMATION: u32 = 0x0000_0040;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateMutexW(attrs: *mut c_void, initial_owner: i32, name: *const u16) -> *mut c_void;
        fn GetLastError() -> u32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(hwnd: *mut c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// 抢到就返回 true；已经有一个实例在跑就弹一句提示并返回 false。
    pub fn acquire() -> bool {
        // 名字带上产品名：以后改名或多开几个分支互不干扰
        let name = wide(concat!("Local\\", env!("PRODUCT_NAME"), ".single-instance"));
        unsafe {
            let handle = CreateMutexW(std::ptr::null_mut(), 0, name.as_ptr());
            if handle.is_null() {
                // 拿不到互斥量本身（极少见）：宁可放行，也别拦着用户用
                return true;
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                // 提示放在单独的线程里：弹窗是模态的，没人点就一直等着。
                // 主线程最多等 3 秒就退出——用户点了「确定」会更快退出，
                // 自动化脚本也不会被一个没人点的窗口卡住。
                std::thread::spawn(|| {
                    let text = wide("岩笋已经开着了。桌面上那个窗口就是，切过去用它就好。");
                    let caption = wide(concat!("岩笋 · ", env!("PRODUCT_NAME")));
                    unsafe {
                        MessageBoxW(
                            std::ptr::null_mut(),
                            text.as_ptr(),
                            caption.as_ptr(),
                            MB_OK | MB_ICONINFORMATION,
                        );
                    }
                });
                std::thread::sleep(std::time::Duration::from_millis(3000));
                return false;
            }
            // 句柄故意不关：让它跟着进程活到退出，进程一结束系统自动释放
            true
        }
    }
}

/// 非 Windows 平台上不做这件事（本项目只发 Windows 版）
#[cfg(not(windows))]
mod win {
    pub fn acquire() -> bool {
        true
    }
}

/// 在启动最早期调用。返回 false 表示已经有一个实例在跑，调用方应立刻退出。
pub fn acquire() -> bool {
    win::acquire()
}
