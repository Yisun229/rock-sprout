//! 命令层的公共口径。
//!
//! 所有给前端的命令都用 [`ApiResult`] 返回：`{ ok, data, error }`。
//! 前端只写一次判断分支，不会出现「这个命令返回字符串、那个命令抛异常」的混乱。

pub mod game;
pub mod mods;
pub mod settings;

use serde::Serialize;

/// 出错时给前端的结构化信息。`message` 是给人看的中文，`code` 给程序判断用。
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ApiResult<T> {
    pub ok: bool,
    pub data: Option<T>,
    pub error: Option<ApiError>,
}

impl<T> ApiResult<T> {
    pub fn ok(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(ApiError {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}
