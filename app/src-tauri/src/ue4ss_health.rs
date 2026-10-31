//! UE4SS 到底有没有生效（计划 §M7-3）。
//!
//! 判据只有一条：`UE4SS.log` 里存在**晚于本次游戏启动时间**的 `Event loop start`。
//! 不许模糊匹配一大片，也不许「装完判一次就缓存」——游戏更新恰恰发生在我们没动启动器的时候，
//! 缓存一次就等于把检测变成摆设。所以这里只做纯读取，谁调谁现算。
//!
//! 两个容易踩的点：
//! 1. `UE4SS.log` 的时间戳是 **UTC**（实测日志写 `07:24` 而本地是 `15:24`）。文件 mtime 拿到的
//!    是自 1970 起的绝对时刻，两边都当 UTC 比，才不会永远判成「没加载」。
//! 2. UE4SS 没加载时**根本不写日志**，所以「日志旧」和「这次没跑游戏」长得一样，得靠游戏自己
//!    留下的痕迹（`Saved\Logs\Modio.log`、`Saved\Config\...\Engine.ini` 的 mtime）来判断
//!    「上一次运行是几点」。实测：把 `UE4SS.dll` 改名后启动游戏，这两个文件都会更新，日志不出现。

use std::path::Path;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::ue4ss_install;

/// 允许的误差：UE4SS 大约在游戏启动后 4 秒才打出 `Event loop start`，
/// 游戏自己那几份痕迹也有几秒延迟，留一分钟足够，又不会把「上一次玩」误当成「这一次」。
pub const TOLERANCE_MS: u64 = 60_000;

/// 一次判定的结论。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Health {
    /// `not_installed` / `unknown` / `ok` / `broken`
    pub state: String,
    /// 给人看的一句话
    pub detail: String,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            state: "unknown".to_owned(),
            detail: String::new(),
        }
    }
}

impl Health {
    fn of(state: &str, detail: impl Into<String>) -> Self {
        Self {
            state: state.to_owned(),
            detail: detail.into(),
        }
    }

    /// 空结论：还没判过。「今天」页的缓存里这一项会被抹成空，序列化时直接省掉，
    /// 免得缓存文件里留一个看着像结论的 `unknown`。
    pub fn is_blank(&self) -> bool {
        self.detail.is_empty()
    }
}

/// 某个脚本 mod 在最近一次启动里的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModHealth {
    /// `ok` = 起来了；`failed` = 它自己载不进去；`unknown` = 还没启动过 / 这次没看到它
    pub state: String,
    /// 给人看的一句话（界面直接显示，不甩错误码）
    pub detail: String,
}

impl ModHealth {
    fn of(state: &str, detail: impl Into<String>) -> Self {
        Self {
            state: state.to_owned(),
            detail: detail.into(),
        }
    }
}

/// 判某一个脚本 mod：读 `Win64\UE4SS.log`，看它这次是起来了还是载不进去。
///
/// 计划 §0 第 35 条把口径钉在 3.0.1 的真实日志上（`Load Mod '…'` 这串在 3.0.1 里不存在）：
/// - 成功：`Mod 'X' has enabled.txt, starting mod.` 或 `Starting Lua mod 'X'`，且没有它的失败行
/// - 失败：`Failed to load dll <路径> for mod X, error code: 0xNN`
pub fn check_mod(win64: &Path, name: &str, installed_at: Option<u64>) -> ModHealth {
    let log = win64.join("UE4SS.log");
    let log_ms = std::fs::metadata(&log).ok().and_then(|m| {
        m.modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
    });
    // 装完之后游戏没启动过：日志不存在，或者日志比安装时间还早
    if log_ms.is_none() {
        return ModHealth::of("unknown", "启动一次游戏才知道有没有生效");
    }
    if let (Some(log_ms), Some(at)) = (log_ms, installed_at) {
        if log_ms + TOLERANCE_MS < at {
            return ModHealth::of("unknown", "装完之后还没启动过游戏");
        }
    }
    let Ok(text) = std::fs::read_to_string(&log) else {
        return ModHealth::of("unknown", "启动一次游戏才知道有没有生效");
    };
    judge_mod(&text, name)
}

/// 从日志正文里判一个 mod；单测直接喂文本。
pub fn judge_mod(text: &str, name: &str) -> ModHealth {
    let quoted = format!("'{name}'");
    let mut loaded = false;
    let mut code: Option<u32> = None;
    for line in text.lines() {
        if let Some(rest) = line.split_once(&format!("for mod {name}, error code: 0x")).map(|x| x.1) {
            if let Some(hex) = rest.split(|c: char| !c.is_ascii_hexdigit()).next() {
                if let Ok(v) = u32::from_str_radix(hex, 16) {
                    code = Some(v);
                }
            }
            continue;
        }
        if line.contains(&format!("Mod {quoted} has enabled.txt, starting mod."))
            || line.contains(&format!("Starting Lua mod {quoted}"))
        {
            loaded = true;
        }
    }
    if let Some(code) = code {
        // 有失败行就以失败为准：3.0.1 里同一个 mod 不会又成功又失败
        return ModHealth::of("failed", reason_of(code));
    }
    if loaded {
        return ModHealth::of("ok", "");
    }
    ModHealth::of("unknown", "这次启动没看到它，可能还没启动过游戏")
}

/// 把 Windows 的加载错误码翻成人话。界面上只显示这句话，不显示码。
fn reason_of(code: u32) -> String {
    match code {
        // ERROR_MOD_NOT_FOUND：依赖的那个 dll 找不到——mod.io 上一批 C++ 脚本 mod 是为
        // MintCat 那套 fork（UE4SSL）编的，官方 UE4SS 里没有那个依赖，属于「它不是给我们这版编的」
        0x7e => "它缺一个运行库（这批 mod 是为另一套 UE4SS 编的），官方版跑不起来".to_owned(),
        0xc1 => "它不是为这版 UE4SS 编的（要在别的版本下用）".to_owned(),
        _ => format!("加载失败（错误码 0x{code:x}），多半是缺依赖"),
    }
}

/// 现在这一套有没有在起作用。
pub fn check(data_root: &Path) -> Health {
    let Some(game) = crate::game::find_installation() else {
        return Health::of("not_installed", "没找到游戏，没法判");
    };
    let win64 = game.binaries_directory();

    // 没装（或装的是别人那套）就不检测、不显示；
    // 但「我们装过、现在四样不见了」是另一回事：那说明被删了或被游戏更新覆盖，要报出来
    if ue4ss_install::detect(&win64).kind != "ours" {
        if installed_at(data_root).is_some() {
            return Health::of(
                "broken",
                "四样不在游戏目录里了",
            );
        }
        return Health::of("not_installed", "游戏目录里没有我们装的 UE4SS");
    }

    let installed_at = installed_at(data_root);
    let launched = last_launch(win64.as_path());
    let loaded = last_event_loop(&win64.join(ue4ss_install::RUNTIME_LOG));

    judge(installed_at, launched, loaded)
}

/// 纯判定，给单测用。三个入参都是 unix 毫秒。
pub fn judge(installed_at: Option<u64>, last_launch: Option<u64>, loaded: Option<u64>) -> Health {
    let Some(launch) = last_launch else {
        return Health::of("unknown", "还没看到这次安装之后启动过游戏");
    };
    if let Some(born) = installed_at {
        if launch + TOLERANCE_MS < born {
            return Health::of("unknown", "这套装好之后游戏还没启动过");
        }
    }
    match loaded {
        Some(loaded) if launch <= loaded + TOLERANCE_MS => Health::of("ok", "上次启动加载成功"),
        Some(_) => Health::of("broken", "日志停在更早以前"),
        None => Health::of("broken", "上次启动没写日志"),
    }
}

/// 这套是什么时候铺上去的（install 时写的小记账文件）。
pub fn installed_at(data_root: &Path) -> Option<u64> {
    let text = std::fs::read_to_string(marker(data_root)).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value.get("installed_at")?.as_u64()
}

/// 记账：铺完写一份，撤掉时删掉。
pub fn remember(data_root: &Path, at_ms: u64) -> std::io::Result<()> {
    let value = serde_json::json!({
        "installed_at": at_ms,
        "version": crate::ue4ss::VERSION,
    });
    std::fs::write(marker(data_root), value.to_string())
}

pub fn forget(data_root: &Path) {
    let _ = std::fs::remove_file(marker(data_root));
}

fn marker(data_root: &Path) -> std::path::PathBuf {
    data_root.join("ue4ss-installed.json")
}

/// 游戏自己留下的「上次运行」痕迹：`Saved\Logs\Modio.log`、`Saved\Config\...\Engine.ini`。
///
/// 两个取最晚的那个。实测 UE4SS 没加载时这两个照样更新，正好用来当「这次启动」的锚点。
fn last_launch(win64: &Path) -> Option<u64> {
    // Win64 → FSD：`FSD\Binaries\Win64`
    let fsd = win64.parent()?.parent()?;
    let saved = fsd.join("Saved");
    let candidates = [
        saved.join("Logs").join("Modio.log"),
        saved.join("Config").join("WindowsNoEditor").join("Engine.ini"),
    ];
    candidates.iter().filter_map(|p| modified_ms(p)).max()
}

fn modified_ms(path: &Path) -> Option<u64> {
    let meta = std::fs::metadata(path).ok()?;
    let time = meta.modified().ok()?;
    time.duration_since(UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}

/// 日志里最后一条 `Event loop start` 的时刻（当 UTC 解析）。
pub fn last_event_loop(log: &Path) -> Option<u64> {
    let text = std::fs::read_to_string(log).ok()?;
    last_event_loop_in(&text)
}

/// 同上，但直接从文本里挑——单测用。
pub fn last_event_loop_in(text: &str) -> Option<u64> {
    let mut latest = None;
    for line in text.lines() {
        if !line.contains("Event loop start") {
            continue;
        }
        if let Some(at) = parse_utc(line) {
            latest = Some(latest.map_or(at, |old: u64| old.max(at)));
        }
    }
    latest
}

/// 解析 `[2026-10-31 07:24:23] …` 这种行首时间，按 UTC 算。
fn parse_utc(line: &str) -> Option<u64> {
    let rest = line.strip_prefix('[')?;
    let stamp = rest.get(..19)?;
    let bytes = stamp.as_bytes();
    if bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') || bytes.get(10) != Some(&b' ')
        || bytes.get(13) != Some(&b':') || bytes.get(16) != Some(&b':')
    {
        return None;
    }
    let year: i64 = stamp.get(0..4)?.parse().ok()?;
    let month: u32 = stamp.get(5..7)?.parse().ok()?;
    let day: u32 = stamp.get(8..10)?.parse().ok()?;
    let hour: u64 = stamp.get(11..13)?.parse().ok()?;
    let minute: u64 = stamp.get(14..16)?.parse().ok()?;
    let second: u64 = stamp.get(17..19)?.parse().ok()?;
    let days = days_from_civil(year, month, day);
    let secs = days * 86_400 + hour * 3_600 + minute * 60 + second;
    Some(secs * 1_000)
}

/// 公历日期 → 1970-01-01 起的天数（Howard Hinnant 那套算法，纯整数）。
fn days_from_civil(year: i64, month: u32, day: u32) -> u64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let m = month as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era as i64 * 146_097 + doe as i64 - 719_468;
    days.max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 日志时间按utc算() {        // 2026-10-31 07:24:23 UTC = 1793431463 秒
        let at = parse_utc("[2026-10-31 07:24:23] Event loop start").unwrap();
        assert_eq!(at, 1_793_431_463_000);
    }

    #[test]
    fn 挑最后一条event_loop() {
        let text = "\
[2026-10-31 07:24:19] Console created
[2026-10-30 07:24:23] Event loop start
[2026-10-31 07:24:23] Event loop start
[2026-10-31 07:24:25] [Lua] done
";
        assert_eq!(last_event_loop_in(text), Some(1_793_431_463_000));
    }

    #[test]
    fn 没有日志就没有加载时间() {
        assert_eq!(last_event_loop_in(""), None);
        assert_eq!(last_event_loop_in("[2026-10-31 07:24:19] Console created"), None);
    }

    #[test]
    fn 这次启动之后加载了就算好() {
        // 启动 10:00:00，加载 10:00:04
        let launch = 1_793_431_200_000;
        let loaded = launch + 4_000;
        let health = judge(None, Some(launch), Some(loaded));
        assert_eq!(health.state, "ok");
    }

    #[test]
    fn 日志停在更早已前就是失效() {
        let launch = 1_793_431_200_000;
        let old = launch - 3_600_000; // 一小时前那次
        let health = judge(None, Some(launch), Some(old));
        assert_eq!(health.state, "broken");
    }

    #[test]
    fn 根本没日志也算失效() {
        let launch = 1_793_431_200_000;
        assert_eq!(judge(None, Some(launch), None).state, "broken");
    }

    #[test]
    fn 装好之后没启动过游戏就不下结论() {
        let born = 1_793_431_200_000;
        let older = born - 600_000;
        assert_eq!(judge(Some(born), Some(older), None).state, "unknown");
        assert_eq!(judge(None, None, None).state, "unknown");
    }

    #[test]
    fn 差一分钟以内不算失效() {
        let launch = 1_793_431_200_000;
        let loaded = launch - TOLERANCE_MS + 1_000;
        assert_eq!(judge(None, Some(launch), Some(loaded)).state, "ok");
    }

    #[test]
    fn 日志里说它起来了就算生效() {
        let text = "\
[2026-10-31 09:56:16] Mod 'ProSproutProbe' has enabled.txt, starting mod.
[2026-10-31 09:56:16] Starting Lua mod 'ConsoleCommandsMod'
";
        let h = judge_mod(text, "ProSproutProbe");
        assert_eq!(h.state, "ok");
    }

    #[test]
    fn lua类也能认() {
        let text = "[2026-10-31 09:56:16] Starting Lua mod 'SomeLua'\n";
        assert_eq!(judge_mod(text, "SomeLua").state, "ok");
    }

    #[test]
    fn 载不进dll时把错误码翻成人话() {
        let text = "\
[2026-10-31 09:56:12] Failed to load dll <D:\\Steam\\…\\Mods\\4454464\\dlls\\main.dll> for mod 4454464, error code: 0x7e
[2026-10-31 09:56:12] Was unable to install mod '4454464' for unknown reasons. Mod is not installable.
";
        let h = judge_mod(text, "4454464");
        assert_eq!(h.state, "failed");
        assert!(h.detail.contains("另一套 UE4SS"), "要说清是为别的版本编的：{}", h.detail);
        assert!(!h.detail.contains("0x"), "界面文案里不该有错误码：{}", h.detail);
    }

    #[test]
    fn 别家mod的失败行不能算到我们头上() {
        let text = "[2026-10-31 09:56:12] Failed to load dll <…> for mod 999, error code: 0x7e\n";
        let h = judge_mod(text, "4454464");
        assert_eq!(h.state, "unknown");
    }

    #[test]
    fn 日志里没提到它就说不确定() {
        let text = "[2026-10-31 09:56:16] Starting Lua mod 'Other'\n";
        assert_eq!(judge_mod(text, "4454464").state, "unknown");
    }
}
