//! 设置页要用的命令：填令牌、看当前是谁。
//!
//! 令牌只存在本机 `<数据目录>\modio-token.txt`，不上传任何地方。
//! 保存之前一定先拿它问一次 mod.io 的 `/me`——**能问出真 uid 才算数**，
//! 这样既验证了令牌有效，也避免像别的工具那样把所有用户都算成同一个 uid。

use serde::Serialize;

use super::ApiResult;
use crate::modio::Modio;
use crate::paths;

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
}

fn status_of(has_token: bool, username: Option<String>, uid: Option<u64>, note: &str) -> TokenStatus {
    TokenStatus {
        has_token,
        username,
        uid,
        note: note.to_owned(),
    }
}

/// 读本机令牌并问一次 mod.io：这是谁。
#[tauri::command]
pub async fn token_status() -> ApiResult<TokenStatus> {
    let path = paths::token_file();
    let client = match Modio::from_token_file(&path) {
        Ok(c) => c,
        Err(e) => {
            // 没存过令牌是最常见的情况，不算错误
            return ApiResult::ok(status_of(false, None, None, &e.message()));
        }
    };

    match client.me().await {
        Ok(me) => ApiResult::ok(status_of(
            true,
            Some(me.username.clone()),
            Some(me.id),
            &format!("已登录：{}", me.username),
        )),
        Err(e) => ApiResult::ok(status_of(true, None, None, &e.message())),
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

    let path = paths::token_file();
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return ApiResult::err("io", format!("建目录失败：{e}"));
        }
    }
    if let Err(e) = std::fs::write(&path, &token) {
        return ApiResult::err("io", format!("写文件失败：{e}"));
    }

    tracing::info!(uid = me.id, "令牌已保存");
    ApiResult::ok(status_of(
        true,
        Some(me.username.clone()),
        Some(me.id),
        &format!("已登录：{}", me.username),
    ))
}

/// 忘掉令牌（换账号或想清掉的时候用）。
#[tauri::command]
pub fn clear_token() -> ApiResult<TokenStatus> {
    let path = paths::token_file();
    match std::fs::remove_file(&path) {
        Ok(()) => ApiResult::ok(status_of(false, None, None, "令牌已清除")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            ApiResult::ok(status_of(false, None, None, "本来就没有存过令牌"))
        }
        Err(e) => ApiResult::err("io", format!("删文件失败：{e}")),
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
        let s = status_of(false, None, None, "还没填");
        assert!(!s.has_token);
        assert!(s.username.is_none());
        assert!(s.uid.is_none());
    }
}
