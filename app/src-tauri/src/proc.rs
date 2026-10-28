//! 动手之前先看游戏在不在跑。
//!
//! 游戏开着的时候，它自己攥着 `mods_P.pak` 和加载器 DLL 不放，
//! 我们这时候去替换文件只会拿到一句 `os error 32`（另一个程序在用这个文件）。
//! 与其让用户看到系统话，不如先问一句「游戏在跑，关掉再来」。
//!
//! 实现上直接跑系统的 `tasklist`，不引依赖（`sysinfo` 要多编一个 crate，还得跟着升级）。

/// 游戏进程名（Steam 版就是这个；Xbox 版是另一个包，目前不支持）。
pub const PROCESS_NAME: &str = "FSD-Win64-Shipping.exe";

/// 从 `tasklist` 的输出里判断有没有这个进程。
///
/// `tasklist /FI "IMAGENAME eq xxx" /NH` 没找到时输出的是一句
/// `信息: 没有运行的任务匹配指定标准。`（英文系统是 `INFO: No tasks are running...`），
/// 找到了则每行以进程名开头。所以这里只认「某一行以进程名开头」。
fn output_has_process(stdout: &str, name: &str) -> bool {
    stdout
        .lines()
        .any(|line| line.trim_start().to_lowercase().starts_with(&name.to_lowercase()))
}

/// 游戏现在在跑吗？
pub fn game_running() -> bool {
    running_by(PROCESS_NAME)
}

/// 某个进程名在不在跑。查不出来（没装 tasklist、权限不够）一律当「没在跑」——
/// 见 §6 风险：宁可让替换动作去撞文件锁，也不要因为查不到就把用户拦在门外。
fn running_by(name: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("IMAGENAME eq {name}"), "/NH"])
            .output();
        match out {
            Ok(o) => {
                let text = String::from_utf8_lossy(&o.stdout);
                output_has_process(&text, name)
            }
            Err(e) => {
                tracing::warn!(%e, "查进程没查成，就当游戏没在跑");
                false
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = name;
        false
    }
}

/// 要动游戏目录之前先过这一关：游戏在跑就返回一句人话，让前端直接显示。
pub fn guard() -> Result<(), String> {
    if game_running() {
        return Err("《深岩银河》正在运行，请先关掉游戏再操作（游戏开着的时候文件被它占着，改不了）".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 找不到进程时的输出算没在跑() {
        let out = "信息: 没有运行的任务匹配指定标准。\r\n";
        assert!(!output_has_process(out, PROCESS_NAME));
    }

    #[test]
    fn 英文系统的没找到也算没在跑() {
        let out = "INFO: No tasks are running which match the specified criteria.\r\n";
        assert!(!output_has_process(out, PROCESS_NAME));
    }

    #[test]
    fn 有进程行就算在跑() {
        let out = "FSD-Win64-Shipping.exe        12345 Console                    1    123,456 K\r\n";
        assert!(output_has_process(out, PROCESS_NAME));
    }

    #[test]
    fn 名字大小写不一样也认得出来() {
        let out = "fsd-win64-shipping.exe        12345 Console\r\n";
        assert!(output_has_process(out, PROCESS_NAME));
    }

    #[test]
    fn 别的进程不算() {
        let out = "steam.exe        1 Console\r\n";
        assert!(!output_has_process(out, PROCESS_NAME));
    }
}
