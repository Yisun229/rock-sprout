//! 翻译相关的命令。
//!
//! 两条给界面用：翻一整个 mod 的简介、翻一段随手给的文本；另外两条读写翻译设置。
//! 翻过的内容存在 `<数据目录>\cache\zh\`，同一个 mod 的同一段原文再翻不再发请求。

use super::ApiResult;
use crate::paths;
use crate::settings::{self, Settings};
use crate::translate::{self, TranslateError};

/// 翻好的东西，连同「是不是从本地缓存拿的」，界面据此可以说一句「上次翻过」。
#[derive(Debug, serde::Serialize)]
pub struct TranslateResult {
    pub text: String,
    pub cached: bool,
}

fn to_api<T>(result: Result<T, TranslateError>) -> ApiResult<T> {
    match result {
        Ok(v) => ApiResult::ok(v),
        Err(e) => ApiResult::err(e.code, e.message),
    }
}

/// 翻译缓存目录：`<数据目录>\cache\zh`
fn cache_dir() -> std::path::PathBuf {
    paths::data_root().join("cache").join("zh")
}

/// 翻一个 mod 的简介（原文取「纯文本简介」，空了退回摘要）。
#[tauri::command]
pub async fn translate_detail(mod_id: u64, target: String) -> ApiResult<TranslateResult> {
    let settings = settings::load_at(&paths::data_root());
    let target = if target.trim().is_empty() {
        settings.translate_target
    } else {
        target
    };

    // 拿原文：详情接口里已经有纯文本简介，不用另存一份
    let detail = match crate::token::client(&paths::data_root()) {
        Ok(c) => match c.detail(mod_id).await {
            Ok(d) => d,
            Err(e) => return ApiResult::err(e.code(), e.message()),
        },
        Err(e) => return ApiResult::err(e.code(), e.message()),
    };
    let source = if detail.description_plaintext.trim().is_empty() {
        detail.summary
    } else {
        detail.description_plaintext
    };

    let key = format!("{mod_id}-{}", translate::hash16(&source));
    let result = translate::translate_cached(
        &cache_dir(),
        &key,
        &source,
        &target,
        &settings.translate_endpoint,
    )
    .await;
    to_api(result.map(|(text, cached)| TranslateResult { text, cached }))
}

/// 翻一段文本（界面里选中的摘要、说明之类）。
#[tauri::command]
pub async fn translate_text(text: String, target: String) -> ApiResult<String> {
    let settings = settings::load_at(&paths::data_root());
    let target = if target.trim().is_empty() {
        settings.translate_target
    } else {
        target
    };
    let result = translate::translate(&text, &target, &settings.translate_endpoint).await;
    to_api(result)
}

/// 读翻译设置。
#[tauri::command]
pub fn get_translate_settings() -> ApiResult<Settings> {
    ApiResult::ok(settings::load_at(&paths::data_root()))
}

/// 写翻译设置。只改传进来的那几项，没传的保持原样。
#[tauri::command]
pub fn set_translate_settings(
    endpoint: Option<String>,
    target: Option<String>,
) -> ApiResult<Settings> {
    let root = paths::data_root();
    let mut current = settings::load_at(&root);
    if let Some(e) = endpoint {
        current.translate_endpoint = e;
    }
    if let Some(t) = target {
        current.translate_target = t;
    }
    let current = current.normalized();
    match settings::save_at(&root, &current) {
        Ok(_) => ApiResult::ok(current),
        Err(e) => ApiResult::err("io", format!("设置存不下来：{e}")),
    }
}
