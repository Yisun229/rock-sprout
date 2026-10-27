//! 从 mint 搬来的「整合」代码需要的两个类型垫片。
//!
//! mint 里这些类型定义在它自己的 providers 模块里（跟 mod.io、缓存机制绑在一起）。
//! 我们只搬了整合那一段，所以这里只留下最小的一层：能被 `integrate` 和 `mod_lints` 编译通过。

use std::io::{Read, Seek};

pub use mint_lib::mod_info::*;

/// 既能读又能前后跳的输入流（pak 解析需要）。
pub trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

/// 取 mod 时可能出的错。我们自己的下载流程有自己的错误类型（`crate::modio::ModioError`），
/// 这个只用于满足整合代码签名，不会真的被构造出很多变体。
#[derive(Debug, snafu::Snafu)]
pub enum ProviderError {
    #[snafu(display("this provider is not supported here"))]
    Unsupported,
}

impl ProviderError {
    pub fn opt_mod_id(&self) -> Option<u32> {
        None
    }
}
