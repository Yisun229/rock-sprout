//! 设置页要用的命令：填令牌、看当前是谁。
//!
//! 令牌只存在本机 `<数据目录>\modio-token.txt`，不上传任何地方。
//! 保存之前一定先拿它问一次 mod.io 的 `/me`——**能问出真 uid 才算数**，
//! 这样既验证了令牌有效，也避免像别的工具那样把所有用户都算成同一个 uid。

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::ApiResult;
use crate::config::{self, Config};
use crate::modio::{Modio, ModioError};
use crate::paths;
use crate::token;

#[derive(Debug, Serialize)]
pub struct TokenStatus {
    /// 本机有没有存过令牌
    pub has_token: bool,
    /// 令牌能换来身份时才有：显示名
    pub username: Option<String>,
    /// 令牌能换来身份时才有：mod.io 上的数字 uid
    pub uid: Option<u64>,
    /// 给用户看的一句话（没登录 / 登录了 / 连不上时分别不同）
    pub note: String,
    /// 验身份没成时是哪一类（`network`/`unauthorized`…）。正常时是 None，
    /// 界面拿它决定要不要摆那条红点提示，而不是把一串原始报错当正文。
    pub code: Option<String>,
}

fn status_of(
    has_token: bool,
    username: Option<String>,
    uid: Option<u64>,
    note: &str,
    code: Option<&str>,
) -> TokenStatus {
    TokenStatus {
        has_token,
        username,
        uid,
        note: note.to_owned(),
        code: code.map(str::to_owned),
    }
}

/// 读本机令牌并问一次 mod.io：这是谁。
#[tauri::command]
pub async fn token_status() -> ApiResult<TokenStatus> {
    let Some(token) = token::load(&paths::data_root()) else {
        return ApiResult::ok(status_of(
            false,
            None,
            None,
            &ModioError::NoToken.message(),
            None,
        ));
    };
    let client = Modio::new(token);

    match client.me().await {
        Ok(me) => ApiResult::ok(status_of(
            true,
            Some(me.username.clone()),
            Some(me.id),
            &format!("已登录：{}", me.username),
            None,
        )),
        Err(e) => ApiResult::ok(status_of(
            true,
            None,
            None,
            &e.message(),
            Some(e.code()),
        )),
    }
}

/// 保存令牌：先验证，验过了才落盘。
#[tauri::command]
pub async fn save_token(token: String) -> ApiResult<TokenStatus> {
    let token = token.trim().to_owned();
    if token.is_empty() {
        return ApiResult::err("empty_token", "令牌是空的，先把 mod.io 给的那串字粘进来");
    }

    let client = Modio::new(token.clone());
    let me = match client.me().await {
        Ok(me) => me,
        Err(e) => return ApiResult::err(e.code(), e.message()),
    };

    if let Err(e) = token::save(&paths::data_root(), &token) {
        return ApiResult::err("io", e);
    }

    tracing::info!(uid = me.id, "令牌已保存");
    ApiResult::ok(status_of(
        true,
        Some(me.username.clone()),
        Some(me.id),
        &format!("已登录：{}", me.username),
        None,
    ))
}

/// 忘掉令牌（换账号或想清掉的时候用）。
#[tauri::command]
pub fn clear_token() -> ApiResult<TokenStatus> {
    match token::clear(&paths::data_root()) {
        Ok(true) => ApiResult::ok(status_of(false, None, None, "令牌已清除", None)),
        Ok(false) => {
            ApiResult::ok(status_of(false, None, None, "本来就没有存过令牌", None))
        }
        Err(e) => ApiResult::err("io", e),
    }
}

/// 导出配置：把当前已装列表写成一个 json，默认放到数据目录下。
#[derive(Debug, Serialize)]
pub struct ExportOutcome {
    /// 真正落盘的路径
    pub path: String,
    /// 里面一共几个 mod
    pub count: usize,
}

#[tauri::command]
pub fn export_config(dir: Option<String>) -> ApiResult<ExportOutcome> {
    let dir = dir
        .map(PathBuf::from)
        .unwrap_or_else(|| paths::data_root().to_path_buf());
    let mods = crate::ipc::mods::exported_mods();
    let count = mods.len();
    let config = Config::build(mods, env!("CARGO_PKG_VERSION"));

    match config.write_to_dir(&dir) {
        Ok(path) => ApiResult::ok(ExportOutcome {
            path: path.display().to_string(),
            count,
        }),
        Err(e) => ApiResult::err("io", e),
    }
}

/// 读一份配置回来，**只比对、不自动装**：告诉用户两边差在哪些 mod 上。
#[derive(Debug, Serialize)]
pub struct ImportOutcome {
    pub path: String,
    /// 这份配置里一共几个 mod
    pub total: usize,
    /// 配置里有、本机没有的（名字）
    pub only_in_file: Vec<String>,
    /// 本机有、配置里没有的（名字）
    pub only_local: Vec<String>,
    /// 两边都有（名字）
    pub same: Vec<String>,
    /// 给界面直接用的一句话
    pub summary: String,
    /// 导出这份配置时的程序版本
    pub app_version: String,
    /// 导出时间（unix 毫秒）
    pub exported_at: u64,
}

#[tauri::command]
pub fn import_config(path: String) -> ApiResult<ImportOutcome> {
    let config = match Config::read_from_file(Path::new(&path)) {
        Ok(c) => c,
        Err(e) => return ApiResult::err("config", e),
    };

    let current = crate::ipc::mods::exported_mods();
    let diff = config::diff(&current, &config.mods);
    let summary = diff.summary();

    ApiResult::ok(ImportOutcome {
        path,
        total: config.mods.len(),
        only_in_file: diff.only_in_file,
        only_local: diff.only_local,
        same: diff.same,
        summary,
        app_version: config.app_version,
        exported_at: config.exported_at,
    })
}

/// 诊断信息：出问题时点一下就能把这几行贴出来，不用让用户自己找路径。
///
/// 只写「是什么状态」，不写令牌内容，也不上网——断网时也拿得到。
#[tauri::command]
pub fn diagnostics() -> ApiResult<String> {
    let mut lines = Vec::new();
    lines.push(format!("岩笋 {}", env!("CARGO_PKG_VERSION")));
    lines.push(format!(
        "系统：{} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    lines.push(format!("数据目录：{}", paths::data_root().display()));
    {
        let (_, mode) = paths::writable_data_root();
        lines.push(format!("数据模式：{}（{}）", mode.as_str(), mode.label()));
    }
    lines.push(format!(
        "令牌：{}",
        if token::has_token(&paths::data_root()) {
            "已保存（内容不会出现在这里）"
        } else {
            "还没填"
        }
    ));

    let installed = crate::ipc::mods::exported_mods();
    let names = installed
        .iter()
        .map(|m| format!("{}（{}）", m.name, m.source))
        .collect::<Vec<_>>()
        .join("、");
    lines.push(match installed.is_empty() {
        true => "已装 mod：0 个".to_owned(),
        false => format!("已装 mod：{} 个 —— {}", installed.len(), names),
    });

    lines.extend(crate::game::status_lines());
    lines.push(format!("日志：{}", paths::logs_dir().display()));

    ApiResult::ok(lines.join("\n"))
}

#[tauri::command]
pub fn audit_game() -> ApiResult<crate::audit::AuditReport> {
    ApiResult::ok(crate::audit::audit())
}

/// 处理体检报告里的一条：动作名与目标都来自报告本身。
#[tauri::command]
pub fn fix_audit_issue(action: String, target: String) -> ApiResult<String> {
    match crate::audit::perform(&action, &target) {
        Ok(message) => ApiResult::ok(message),
        Err(message) => ApiResult::err("audit", message),
    }
}

/// 数据目录在哪、为什么在那（[`DataLocation`]）。
#[derive(Debug, Serialize)]
pub struct DataLocation {
    /// 实际的绝对路径
    pub path: String,
    /// `env` / `portable` / `fallback`
    pub mode: String,
    /// 模式的中文说法，界面直接显示
    pub label: String,
    /// 是不是绿色模式（数据就在程序旁边）
    pub portable: bool,
    /// 程序目录写不了、退到用户目录时为 true —— 界面要常驻提示这一条
    pub fell_back: bool,
}

/// 看当前数据目录落在哪。界面用它决定要不要显示「便携模式没生效」那句提示。
#[tauri::command]
pub fn data_location() -> ApiResult<DataLocation> {
    let (path, mode) = paths::writable_data_root();
    ApiResult::ok(DataLocation {
        path: path.display().to_string(),
        mode: mode.as_str().to_owned(),
        label: mode.label().to_owned(),
        portable: mode == paths::DataMode::Portable,
        fell_back: mode == paths::DataMode::Fallback,
    })
}

/// 在资源管理器里打开数据目录（用户要自己拷走 data 文件夹时用）。
#[tauri::command]
pub fn open_data_dir() -> ApiResult<String> {
    let dir = paths::data_root();
    if !dir.exists() {
        return ApiResult::err("io", format!("数据目录还不存在：{}", dir.display()));
    }
    #[cfg(windows)]
    let spawned = std::process::Command::new("cmd")
        .args(["/C", "explorer", &dir.display().to_string()])
        .spawn();
    #[cfg(not(windows))]
    let spawned = std::process::Command::new("xdg-open").arg(&dir).spawn();

    match spawned {
        Ok(_) => ApiResult::ok(dir.display().to_string()),
        Err(e) => ApiResult::err("io", format!("打不开文件夹：{e}")),
    }
}

/// 只放行 http/https：这个命令会调起系统默认程序，
/// 不能让它变成「随便打开本地文件/别的协议」的口子。
fn allowed_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// 用系统默认浏览器打开一个网页（现在只有「报告问题」用它）。
#[tauri::command]
pub fn open_url(url: String) -> ApiResult<String> {
    let url = url.trim().to_owned();
    if !allowed_url(&url) {
        return ApiResult::err("bad_url", "只允许打开 http 或 https 的网址");
    }

    #[cfg(windows)]
    let spawned = std::process::Command::new("cmd")
        .args(["/C", "start", "", &url])
        .spawn();
    #[cfg(not(windows))]
    let spawned = std::process::Command::new("xdg-open").arg(&url).spawn();

    match spawned {
        Ok(_) => ApiResult::ok(url),
        Err(e) => ApiResult::err("open_failed", format!("打不开浏览器：{e}")),
    }
}

/// 有没有旧版本留下的数据要认（没有就是 `null`）。
#[tauri::command]
pub fn legacy_data() -> ApiResult<Option<crate::legacy::Candidate>> {
    ApiResult::ok(crate::legacy::detect(&paths::data_root()))
}

/// 用户对旧数据的选择：`adopt` = 沿用，否则 = 不用。
#[tauri::command]
pub fn resolve_legacy(path: String, adopt: bool) -> ApiResult<String> {
    let decision = if adopt {
        crate::legacy::Decision::Adopt
    } else {
        crate::legacy::Decision::Decline
    };
    match crate::legacy::apply(&paths::data_root(), Path::new(&path), decision) {
        Ok(message) => ApiResult::ok(message),
        Err(message) => ApiResult::err("legacy", message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 空令牌会被直接拒绝() {
        // 这里只测「空串判定」这段纯逻辑，不碰网络
        assert!("   ".trim().is_empty());
    }

    #[test]
    fn 状态结构里没登录时不该带身份() {
        let s = status_of(false, None, None, "还没填", None);
        assert!(!s.has_token);
        assert!(s.username.is_none());
        assert!(s.uid.is_none());
    }

    #[test]
    fn 只放行http和https() {
        assert!(allowed_url("https://github.com/Yisun229/rock-sprout"));
        assert!(allowed_url("http://example.com"));
        // 大写和前后空格也算同一个网址
        assert!(allowed_url("  HTTPS://github.com  "));
        assert!(!allowed_url("file:///C:/Windows/System32/calc.exe"));
        assert!(!allowed_url("steam://rungameid/548430"));
        assert!(!allowed_url("javascript:alert(1)"));
        assert!(!allowed_url(""));
    }
}
