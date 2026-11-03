//! GVAS 读取器：把《深岩银河》的存档（`.sav`）读成一棵树。
//!
//! 只读：这里只有解析，没有任何写入存档的代码。
//!
//! 三处与常见 GVAS 资料不一样的地方（照 `_tools/save-fields.md` §0 实测写成，不是抄的）：
//!
//! 1. `StructProperty` 的标签在结构体名之后还有 **16 字节 struct GUID**，然后才是
//!    「有没有属性 GUID」那 1 个字节。少读这 16 字节，后面整棵树都会错位。
//! 2. `ArrayProperty` 的**元素标签只写一份**：体 = `int32 数量` + 一段元素标签
//!    （它的 `size` 是**所有元素加起来**的字节数）+ 各元素的值首尾相接。
//! 3. `Map` / `Set` 的体最前面多一个**恒为 0 的 int32**：体 = `int32(0)` + `int32 数量` + 元素。
//!
//! 另外 `Guid` / `Vector` / `DateTime` 这些结构体在存档里是**裸二进制**，
//! 不是属性表；按属性表去读会读飞。

use std::fmt;

pub mod gvas;
pub mod model;
pub mod store;

pub use gvas::{
    Array, Binary, ByteValue, CustomVersion, EngineVersion, Guid, Header, Map, Note, NoteKind,
    ParseOutcome, ParseStats, Property, Set, Skipped, StructValue, Tag, Value, MAX_DEPTH,
    NODE_BUDGET,
};
pub use model::SaveData;
pub use store::{LoadClass, LoadError, LoadReport};

/// 解析失败的种类。每一类都对应一句能直接给用户看的话。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// 开头不是 GVAS 四个字节
    NotGvas,
    /// 容器头读得下去但内容不认识（版本号离谱、自定义版本条数异常等）
    BadHeader,
    /// 读到文件尾还不够
    Truncated,
    /// 字符串长度不合法（负数过大、或超出剩余字节）
    BadString,
    /// 某个数量字段读错导致要循环百万次，主动停下
    Budget,
    /// 嵌套太深
    TooDeep,
    /// 数量字段不合法
    BadCount,
    /// 版本不认识（存档格式 / 包版本 / 自定义版本段变了）
    UnknownVersion,
}

impl ErrorKind {
    /// 稳定的小写代号，方便日志和测试断言。
    pub fn code(self) -> &'static str {
        match self {
            ErrorKind::NotGvas => "not_gvas",
            ErrorKind::BadHeader => "bad_header",
            ErrorKind::Truncated => "truncated",
            ErrorKind::BadString => "bad_string",
            ErrorKind::Budget => "budget",
            ErrorKind::TooDeep => "too_deep",
            ErrorKind::BadCount => "bad_count",
            ErrorKind::UnknownVersion => "unknown_version",
        }
    }

    /// 给用户看的一句话（不带技术细节）。
    pub fn human(self) -> &'static str {
        match self {
            ErrorKind::NotGvas => "这不是《深岩银河》的存档（开头不是 GVAS）",
            ErrorKind::BadHeader => "存档头读不懂，可能不是这个游戏的存档",
            ErrorKind::Truncated => "文件像是被截断了，读到一半就没有了",
            ErrorKind::BadString => "存档里有读不懂的文字段",
            ErrorKind::Budget => "存档里有数量字段读错了，解析主动停下",
            ErrorKind::TooDeep => "存档嵌套太深，解析停下",
            ErrorKind::BadCount => "存档里有不合法的数量",
            ErrorKind::UnknownVersion => "存档版本不认识，可能是游戏更新了",
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.human())
    }
}

/// 解析错误：带种类、出错位置、以及一句细节。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub kind: ErrorKind,
    pub at: usize,
    pub detail: String,
}

impl ParseError {
    pub fn new(kind: ErrorKind, at: usize, detail: impl Into<String>) -> Self {
        Self { kind, at, detail: detail.into() }
    }

    pub fn code(&self) -> &'static str {
        self.kind.code()
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}（{} @ {} 字节）：{}", self.kind.human(), self.kind.code(), self.at, self.detail)
    }
}

impl std::error::Error for ParseError {}

/// 把一段字节解析成存档树。成功时返回树和「读了多少、跳过多少」。
///
/// 认不出**版本**时单独报 [`ErrorKind::UnknownVersion`]，不要混进「结构对不上」——
/// 这两种情况给用户的下一步动作完全不同（一个是等更新，一个是可以查文件）。
pub fn parse(bytes: &[u8]) -> Result<ParseOutcome, ParseError> {
    let out = gvas::parse(bytes)?;

    // 版本闸门：只认实测过的那一套。游戏大版本一变，这里的数字就该跟着抬。
    const SAVE_VERSION: i32 = 2;
    const PACKAGE_UE_VERSION: i32 = 522;
    const CUSTOM_VERSION_FORMAT: i32 = 3;
    const SAVE_CLASS: &str = "/Script/FSD.FSDSaveGame";

    if out.header.save_game_file_version != SAVE_VERSION {
        return Err(ParseError::new(
            ErrorKind::UnknownVersion,
            0,
            format!(
                "存档格式版本是 {}，这一版只认 {}",
                out.header.save_game_file_version, SAVE_VERSION
            ),
        ));
    }
    if out.header.package_ue_version != PACKAGE_UE_VERSION {
        return Err(ParseError::new(
            ErrorKind::UnknownVersion,
            0,
            format!(
                "包版本是 {}，这一版只认 {}",
                out.header.package_ue_version, PACKAGE_UE_VERSION
            ),
        ));
    }
    if out.header.custom_version_format != CUSTOM_VERSION_FORMAT {
        return Err(ParseError::new(
            ErrorKind::UnknownVersion,
            0,
            format!(
                "自定义版本段格式是 {}，这一版只认 {}",
                out.header.custom_version_format, CUSTOM_VERSION_FORMAT
            ),
        ));
    }
    if out.header.save_class != SAVE_CLASS {
        return Err(ParseError::new(
            ErrorKind::NotGvas,
            0,
            format!("存档类名是 {}，不是《深岩银河》", out.header.save_class),
        ));
    }

    Ok(out)
}

/// 在树里找一个顶层属性。
pub fn find<'a>(props: &'a [Property], name: &str) -> Option<&'a Property> {
    props.iter().find(|p| p.name == name)
}

/// 按 `A.B.C` 的路径找属性（只走结构体字段，不进数组/Map）。
pub fn find_path<'a>(props: &'a [Property], path: &str) -> Option<&'a Property> {
    let mut cur = props;
    let mut found = None;
    for part in path.split('.') {
        let p = find(cur, part)?;
        found = Some(p);
        cur = match &p.value {
            Value::Struct(s) => s.props.as_slice(),
            _ => &[],
        };
    }
    found
}

#[cfg(test)]
mod tests;
