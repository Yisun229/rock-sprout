//! 低层读取：容器头、属性标签、各类值。
//!
//! 这里所有函数都只读传进来的字节，不改任何文件。读到不认识的属性时按标签里的
//! `size` 跳过并记一条说明，不抛错——只有「真读不下去」（截断、长度非法、
//! 数量离谱）才返回错误。

use super::{ErrorKind, ParseError};

/// 嵌套上限。正常存档最深不过十来层，这里是防御性的。
pub const MAX_DEPTH: usize = 64;

/// 节点预算。某个数量字段读错时，宁可停下也不要把内存吃光。
pub const NODE_BUDGET: usize = 400_000;

const MAX_CUSTOM_VERSIONS: i32 = 4096;

pub type Guid = [u8; 16];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EngineVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
    pub changelist: u32,
    pub branch: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CustomVersion {
    pub guid: Guid,
    pub version: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    pub save_game_file_version: i32,
    pub package_ue_version: i32,
    pub engine: EngineVersion,
    pub custom_version_format: i32,
    pub custom_versions: Vec<CustomVersion>,
    pub save_class: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tag {
    pub ty: String,
    pub size: i64,
    pub array_index: i32,
    pub struct_name: Option<String>,
    pub struct_guid: Option<Guid>,
    pub inner_type: Option<String>,
    pub value_type: Option<String>,
    pub bool_value: Option<u8>,
    pub enum_name: Option<String>,
    pub property_guid: Option<Guid>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub name: String,
    pub tag: Tag,
    /// 属性值在文件里的起点（排查时用）
    pub offset: usize,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructValue {
    pub name: Option<String>,
    pub props: Vec<Property>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Array {
    pub count: i32,
    pub elem_type: String,
    pub elem_struct: Option<String>,
    pub items: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Map {
    pub count: i32,
    pub key_type: String,
    pub value_type: String,
    pub entries: Vec<(Value, Value)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Set {
    pub count: i32,
    pub elem_type: String,
    pub items: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Binary {
    /// 结构体名（Guid / Vector / DateTime …）
    pub ty: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Skipped {
    pub ty: String,
    pub size: i64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ByteValue {
    pub name: Option<String>,
    pub number: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i32),
    Int64(i64),
    UInt32(u32),
    UInt64(u64),
    Float(f32),
    Double(f64),
    Bool(bool),
    Str(String),
    Object(String),
    Enum(String),
    Byte(ByteValue),
    Array(Array),
    Map(Map),
    Set(Set),
    Struct(StructValue),
    /// 裸二进制结构体（Guid/Vector/…）
    Binary(Binary),
    /// 不认识的属性，按标签里的 size 跳过
    Skipped(Skipped),
}

/// 一条「遇到了但不算失败」的说明。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    /// 不认识的类型，按 size 跳过
    Skipped,
    /// 元素读完的位置和标签说的不一样，按标签对齐
    Aligned,
    /// 收尾比标签说的多/少，记录实际差值
    Tail,
    /// 按已知规律做了假设（例如 Map 的键是 Guid）
    Assumed,
}

impl NoteKind {
    pub fn code(self) -> &'static str {
        match self {
            NoteKind::Skipped => "skipped",
            NoteKind::Aligned => "aligned",
            NoteKind::Tail => "tail",
            NoteKind::Assumed => "assumed",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub path: String,
    pub kind: NoteKind,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParseStats {
    /// 走过多少个属性/元素节点
    pub nodes: usize,
    /// 跳过多少个不认识的属性
    pub skipped: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseOutcome {
    pub header: Header,
    pub props: Vec<Property>,
    /// 读完属性树的绝对位置（正常存档应是「文件尾 − 4」）
    pub consumed: usize,
    pub stats: ParseStats,
    pub notes: Vec<Note>,
}

/// 这些结构体在存档里是裸二进制，不是属性表。
pub(super) fn binary_struct_size(name: &str) -> Option<usize> {
    Some(match name {
        "Guid" => 16,
        "Vector" => 12,
        "Vector2D" => 8,
        "Rotator" => 12,
        "Quat" => 16,
        "Color" => 4,
        "LinearColor" => 16,
        "DateTime" => 8,
        "Timespan" => 8,
        "IntPoint" => 8,
        "IntVector" => 12,
        _ => return None,
    })
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    /// 与 JS 原型同口径的节点数（属性和数组/集合元素都算）
    nodes: usize,
    /// 内部步数，只用来做预算保护
    steps: usize,
    skipped: usize,
    notes: Vec<Note>,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0, nodes: 0, steps: 0, skipped: 0, notes: Vec::new() }
    }

    fn left(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    fn err(&self, kind: ErrorKind, detail: impl Into<String>) -> ParseError {
        ParseError::new(kind, self.pos, detail)
    }

    fn need(&self, n: usize, what: &str) -> Result<(), ParseError> {
        if self.left() < n {
            return Err(self.err(
                ErrorKind::Truncated,
                format!("要 {n} 字节（{what}），只剩 {}", self.left()),
            ));
        }
        Ok(())
    }

    fn take(&mut self, n: usize, what: &str) -> Result<&'a [u8], ParseError> {
        self.need(n, what)?;
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn u8(&mut self, what: &str) -> Result<u8, ParseError> {
        Ok(self.take(1, what)?[0])
    }

    fn u16(&mut self, what: &str) -> Result<u16, ParseError> {
        let b = self.take(2, what)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self, what: &str) -> Result<u32, ParseError> {
        let b = self.take(4, what)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn i32(&mut self, what: &str) -> Result<i32, ParseError> {
        Ok(self.u32(what)? as i32)
    }

    fn u64(&mut self, what: &str) -> Result<u64, ParseError> {
        let b = self.take(8, what)?;
        Ok(u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    fn i64(&mut self, what: &str) -> Result<i64, ParseError> {
        Ok(self.u64(what)? as i64)
    }

    fn f32(&mut self, what: &str) -> Result<f32, ParseError> {
        Ok(f32::from_bits(self.u32(what)?))
    }

    fn f64(&mut self, what: &str) -> Result<f64, ParseError> {
        Ok(f64::from_bits(self.u64(what)?))
    }

    fn guid(&mut self, what: &str) -> Result<Guid, ParseError> {
        let b = self.take(16, what)?;
        let mut g = [0u8; 16];
        g.copy_from_slice(b);
        Ok(g)
    }

    /// UE 的 FString：正数是 ASCII/UTF-8（含结尾 0），负数是 UTF-16（含结尾 0）。
    fn fstring(&mut self, what: &str) -> Result<String, ParseError> {
        let len_pos = self.pos;
        let len = self.i32(what)?;
        if len == 0 {
            return Ok(String::new());
        }
        if len > 0 {
            let n = len as usize;
            if n > self.left() {
                return Err(ParseError::new(
                    ErrorKind::BadString,
                    len_pos,
                    format!("{what} 说 {n} 字节，只剩 {}", self.left()),
                ));
            }
            let raw = self.take(n, what)?;
            Ok(String::from_utf8_lossy(raw).trim_end_matches('\0').to_string())
        } else {
            let chars = (-(len as i64)) as usize;
            let bytes = chars.saturating_mul(2);
            if bytes > self.left() {
                return Err(ParseError::new(
                    ErrorKind::BadString,
                    len_pos,
                    format!("{what} 说 {chars} 个 UTF-16 字符，只剩 {}", self.left()),
                ));
            }
            let raw = self.take(bytes, what)?;
            let units: Vec<u16> =
                raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            Ok(String::from_utf16_lossy(&units).trim_end_matches('\0').to_string())
        }
    }

    fn spend(&mut self) -> Result<(), ParseError> {
        self.steps += 1;
        self.nodes += 1;
        if self.steps > NODE_BUDGET {
            return Err(self.err(
                ErrorKind::Budget,
                format!("已处理 {} 步还没读完，可能某个数量字段读错了", self.steps),
            ));
        }
        Ok(())
    }

    fn note(&mut self, path: &str, kind: NoteKind, detail: impl Into<String>) {
        self.notes.push(Note { path: path.to_string(), kind, detail: detail.into() });
    }

    fn seek(&mut self, to: usize) {
        self.pos = to.min(self.buf.len());
    }

    fn skip(&mut self, size: usize, what: &str) -> Result<(), ParseError> {
        if size > self.left() {
            return Err(self.err(
                ErrorKind::Truncated,
                format!("{what} 要跳 {size} 字节，只剩 {}", self.left()),
            ));
        }
        self.pos += size;
        Ok(())
    }

    fn header(&mut self) -> Result<Header, ParseError> {
        let magic = self.take(4, "魔数")?;
        if magic != b"GVAS" {
            return Err(self.err(
                ErrorKind::NotGvas,
                format!("开头是 {:?}，不是 GVAS", String::from_utf8_lossy(magic)),
            ));
        }
        let save_game_file_version = self.i32("saveGameFileVersion")?;
        if !(1..=4).contains(&save_game_file_version) {
            // 这是版本问题，不是文件坏了——两句话对用户的意义完全不同（等更新 / 查文件）。
            return Err(self.err(
                ErrorKind::UnknownVersion,
                format!("存档格式版本是 {save_game_file_version}，不在认识的 1~4 里"),
            ));
        }
        let package_ue_version = self.i32("packageUEVersion")?;
        let engine = EngineVersion {
            major: self.u16("引擎主版本")?,
            minor: self.u16("引擎次版本")?,
            patch: self.u16("引擎补丁号")?,
            changelist: self.u32("引擎 changelist")?,
            branch: self.fstring("引擎分支")?,
        };
        let custom_version_format = self.i32("customVersionFormat")?;
        let count = self.i32("自定义版本条数")?;
        if count < 0 || count > MAX_CUSTOM_VERSIONS {
            return Err(self.err(ErrorKind::BadHeader, format!("自定义版本条数是 {count}")));
        }
        let mut custom_versions = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let guid = self.guid("自定义版本 GUID")?;
            let version = self.i32("自定义版本号")?;
            custom_versions.push(CustomVersion { guid, version });
        }
        let save_class = self.fstring("存档类名")?;
        Ok(Header {
            save_game_file_version,
            package_ue_version,
            engine,
            custom_version_format,
            custom_versions,
            save_class,
        })
    }

    /// 读一段属性标签（不含值）。`None` 表示这一层属性表到此结束。
    fn tag(&mut self) -> Result<Option<(String, Tag)>, ParseError> {
        if self.left() < 4 {
            return Ok(None);
        }
        let name = self.fstring("属性名")?;
        if name == "None" {
            return Ok(None);
        }
        let ty = self.fstring("属性类型")?;
        let size = self.i32("属性大小")? as i64;
        let array_index = self.i32("数组下标")?;
        let mut tag = Tag { ty, size, array_index, ..Tag::default() };
        match tag.ty.as_str() {
            "StructProperty" => {
                tag.struct_name = Some(self.fstring("结构体名")?);
                // 模块文档第 1 条：结构体名之后还有 16 字节 struct GUID
                tag.struct_guid = Some(self.guid("结构体 GUID")?);
            }
            "BoolProperty" => tag.bool_value = Some(self.u8("布尔值")?),
            "ByteProperty" | "EnumProperty" => {
                tag.enum_name = Some(self.fstring("枚举名")?);
            }
            "ArrayProperty" | "SetProperty" => {
                tag.inner_type = Some(self.fstring("元素类型")?);
            }
            "MapProperty" => {
                tag.inner_type = Some(self.fstring("键类型")?);
                tag.value_type = Some(self.fstring("值类型")?);
            }
            _ => {}
        }
        let has_guid = self.u8("属性 GUID 标记")?;
        if has_guid == 1 {
            tag.property_guid = Some(self.guid("属性 GUID")?);
        }
        Ok(Some((name, tag)))
    }

    fn property_list(&mut self, depth: usize, path: &str) -> Result<Vec<Property>, ParseError> {
        if depth > MAX_DEPTH {
            return Err(self.err(ErrorKind::TooDeep, format!("{path} 嵌套超过 {MAX_DEPTH} 层")));
        }
        let mut props = Vec::new();
        loop {
            let (name, tag) = match self.tag() {
                Ok(Some(t)) => t,
                Ok(None) => break,
                Err(e) => {
                    // 个别存档收尾多出几个字节：已经到文件尾就算了
                    if e.kind == ErrorKind::Truncated && self.left() == 0 {
                        break;
                    }
                    return Err(e);
                }
            };
            self.spend()?;
            let offset = self.pos;
            let child = if path.is_empty() { name.clone() } else { format!("{path}.{name}") };
            let value = self.value(&tag, depth, &child)?;
            props.push(Property { name, tag, offset, value });
        }
        Ok(props)
    }

    fn value(&mut self, tag: &Tag, depth: usize, path: &str) -> Result<Value, ParseError> {
        let expected_end =
            if tag.size > 0 { Some(self.pos + tag.size as usize) } else { None };
        let value = self.raw_value(tag, depth, path).map_err(|e| {
            // 带上路径，排查时一眼能看出是哪一段读不下去
            if e.detail.starts_with(path) {
                e
            } else {
                ParseError::new(e.kind, e.at, format!("{path}: {}", e.detail))
            }
        })?;
        if let Some(end) = expected_end {
            if end <= self.buf.len() && self.pos != end {
                let got = self.pos;
                self.note(
                    path,
                    NoteKind::Aligned,
                    format!(
                        "读值读到 {got}，标签说应该到 {end}（差 {} 字节），按标签对齐",
                        end as i64 - got as i64
                    ),
                );
                self.seek(end);
            }
        }
        Ok(value)
    }

    fn raw_value(&mut self, tag: &Tag, depth: usize, path: &str) -> Result<Value, ParseError> {
        Ok(match tag.ty.as_str() {
            "IntProperty" | "Int32Property" => Value::Int(self.i32("Int")?),
            "Int64Property" => Value::Int64(self.i64("Int64")?),
            "UInt32Property" => Value::UInt32(self.u32("UInt32")?),
            "UInt64Property" => Value::UInt64(self.u64("UInt64")?),
            "FloatProperty" => Value::Float(self.f32("Float")?),
            "DoubleProperty" => Value::Double(self.f64("Double")?),
            "BoolProperty" => match tag.bool_value {
                Some(v) => Value::Bool(v != 0),
                // 作为 Map/Set/裸数组的元素时，标签里没有值，体里占 1 字节
                None => Value::Bool(self.u8("布尔值")? != 0),
            },
            "StrProperty" | "NameProperty" => Value::Str(self.fstring("字符串")?),
            "ObjectProperty" | "SoftObjectProperty" | "ClassProperty" | "InterfaceProperty" => {
                Value::Object(self.fstring("对象路径")?)
            }
            "EnumProperty" => {
                if tag.size == 4 {
                    Value::Enum(self.u32("枚举数值")?.to_string())
                } else {
                    Value::Enum(self.fstring("枚举名")?)
                }
            }
            "ByteProperty" => {
                let has_enum = tag.enum_name.as_deref().unwrap_or("").is_empty();
                if has_enum && tag.size <= 1 {
                    Value::Byte(ByteValue { name: None, number: self.u8("字节")? })
                } else {
                    Value::Byte(ByteValue { name: Some(self.fstring("字节枚举")?), number: 0 })
                }
            }
            "ArrayProperty" => Value::Array(self.array(tag, depth, path)?),
            "SetProperty" => Value::Set(self.set(tag, depth, path)?),
            "MapProperty" => Value::Map(self.map(tag, depth, path)?),
            "StructProperty" => Value::Struct(self.structured(tag, depth, path)?),
            "MulticastInlineDelegateProperty" | "MulticastDelegateProperty" | "DelegateProperty" => {
                // 委托（蓝图事件绑定）在这份存档里都是空的：内容不解释，按 size 原样吃掉
                let size = tag.size.max(0) as usize;
                self.skip(size, "委托字段")?;
                Value::Binary(Binary { ty: tag.ty.clone(), bytes: Vec::new() })
            }
            other => {
                let size = tag.size.max(0) as usize;
                self.skip(size, "未知类型")?;
                self.skipped += 1;
                self.note(
                    path,
                    NoteKind::Skipped,
                    format!("跳过：{path}（类型 {other} 不认识，按标签大小跳 {size} 字节）"),
                );
                Value::Skipped(Skipped {
                    ty: other.to_string(),
                    size: tag.size,
                    reason: "未知类型".to_string(),
                })
            }
        })
    }

    /// 结构体：要么是裸二进制（Guid/Vector/…），要么是一段属性表。
    fn structured(&mut self, tag: &Tag, depth: usize, path: &str) -> Result<StructValue, ParseError> {
        let name = tag.struct_name.clone().unwrap_or_default();
        if let Some(size) = binary_struct_size(&name) {
            let start = self.pos;
            let bytes = self.take(size, "裸结构体")?.to_vec();
            return Ok(StructValue {
                name: Some(name.clone()),
                props: vec![Property {
                    name: "$binary".to_string(),
                    tag: Tag { ty: name.clone(), size: size as i64, ..Tag::default() },
                    offset: start,
                    value: Value::Binary(Binary { ty: name, bytes }),
                }],
            });
        }

        let body_start = self.pos;
        let body_end = if tag.size > 0 { Some(body_start + tag.size as usize) } else { None };
        let first = self.property_list(depth + 1, path)?;
        if let Some(end) = body_end {
            if self.pos == end || end > self.buf.len() {
                return Ok(StructValue { name: Some(name), props: first });
            }
            // 收尾对不上时，只在浅层试「前缀多 16 字节」这条老式写法
            if depth <= 2 && body_start + 16 <= self.buf.len() {
                let after_first = self.pos;
                self.seek(body_start + 16);
                if let Ok(second) = self.property_list(depth + 1, path) {
                    if self.pos == end {
                        self.note(path, NoteKind::Assumed, "结构体前面多 16 字节，按老式写法读的");
                        return Ok(StructValue { name: Some(name), props: second });
                    }
                }
                self.seek(after_first);
            }
            self.note(path, NoteKind::Tail, format!("结构体读到 {}，标签说应该到 {end}", self.pos));
        }
        Ok(StructValue { name: Some(name), props: first })
    }

    fn array(&mut self, tag: &Tag, depth: usize, path: &str) -> Result<Array, ParseError> {
        if depth > MAX_DEPTH {
            return Err(self.err(ErrorKind::TooDeep, format!("{path} 嵌套太深")));
        }
        let body_start = self.pos;
        let count = self.i32("数组元素个数")?;
        let elem_type = tag.inner_type.clone().unwrap_or_default();
        if count < 0 {
            return Err(self.err(ErrorKind::BadCount, format!("{path} 元素个数是 {count}")));
        }
        if count == 0 {
            // 空数组后面还可能跟着一段元素标签（实测 size 会大于 4），整段吃掉
            if tag.size > 4 {
                self.skip(tag.size as usize - 4, "空数组的尾巴")?;
            }
            return Ok(Array { count, elem_type, elem_struct: None, items: Vec::new() });
        }

        // 模块文档第 2 条：只有结构体数组才写一段元素标签（size 是总和）；
        // 整数/浮点/字符串这类元素是裸值，直接首尾相接。
        let (elem_tag, value_start) = if elem_type == "StructProperty" {
            let (_, et) = self.tag()?.ok_or_else(|| {
                self.err(ErrorKind::BadCount, format!("{path} 有 {count} 个结构体元素却没有元素标签"))
            })?;
            (Some(et), self.pos)
        } else {
            (None, self.pos)
        };

        let body_end = if tag.size > 0 { Some(body_start + tag.size as usize) } else { None };
        let inner_end = match (elem_tag.as_ref(), body_end) {
            (Some(et), _) if et.size > 0 => Some(value_start + et.size as usize),
            (None, Some(end)) => Some(end),
            _ => None,
        };
        let per = inner_end
            .map(|end| end.saturating_sub(value_start) / count.max(1) as usize)
            .unwrap_or(0);

        let mut items = Vec::new();
        for i in 0..count {
            let before = self.pos;
            self.spend()?;
            let item_path = format!("{path}[{i}]");
            let item = match elem_tag.as_ref() {
                Some(et) => {
                    let sname = et.struct_name.clone().unwrap_or_default();
                    if let Some(size) = binary_struct_size(&sname) {
                        let bytes = self.take(size, "裸结构体元素")?.to_vec();
                        Value::Binary(Binary { ty: sname, bytes })
                    } else {
                        let props = self.property_list(depth + 1, &item_path)?;
                        Value::Struct(StructValue { name: Some(sname), props })
                    }
                }
                None => {
                    let bare = Tag { ty: elem_type.clone(), size: per as i64, ..Tag::default() };
                    self.raw_value(&bare, depth + 1, &item_path)?
                }
            };
            items.push(item);
            if self.pos == before {
                if let Some(end) = inner_end {
                    self.seek(end);
                }
                self.note(path, NoteKind::Tail, format!("第 {i} 个元素没消耗字节，按标签收尾"));
                break;
            }
        }

        if let Some(end) = inner_end {
            if self.pos != end {
                self.note(
                    path,
                    NoteKind::Aligned,
                    format!("元素读完在 {}，元素标签说 {end}，按标签对齐", self.pos),
                );
                self.seek(end);
            }
        }
        Ok(Array {
            count,
            elem_type,
            elem_struct: elem_tag.and_then(|t| t.struct_name),
            items,
        })
    }

    /// 模块文档第 3 条：体最前面多一个恒 0 的 int32。
    fn set(&mut self, tag: &Tag, depth: usize, path: &str) -> Result<Set, ParseError> {
        if depth > MAX_DEPTH {
            return Err(self.err(ErrorKind::TooDeep, format!("{path} 嵌套太深")));
        }
        let body_start = self.pos;
        let pad = self.i32("集合前导 0")?;
        if pad != 0 {
            self.note(path, NoteKind::Assumed, format!("集合前导 4 字节是 {pad}，不是 0"));
        }
        let count = self.i32("集合元素个数")?;
        let elem_type = tag.inner_type.clone().unwrap_or_default();
        if count < 0 {
            return Err(self.err(ErrorKind::BadCount, format!("{path} 元素个数是 {count}")));
        }
        let body_end = if tag.size > 0 { Some(body_start + tag.size as usize) } else { None };
        let per = match body_end {
            Some(end) if count > 0 => end.saturating_sub(self.pos) / count as usize,
            _ => 16,
        };
        let mut items = Vec::new();
        for i in 0..count {
            self.spend()?;
            let here = self.pos;
            if per == 16 {
                let bytes = self.take(16, "集合元素 GUID")?.to_vec();
                items.push(Value::Binary(Binary { ty: "Guid".to_string(), bytes }));
            } else {
                let bare = Tag { ty: elem_type.clone(), size: per as i64, ..Tag::default() };
                items.push(self.raw_value(&bare, depth + 1, &format!("{path}[{i}]"))?);
            }
            if self.pos == here {
                break;
            }
        }
        if let Some(end) = body_end {
            if end <= self.buf.len() && self.pos != end {
                self.note(path, NoteKind::Aligned, format!("集合读完在 {}，标签说 {end}，按标签对齐", self.pos));
                self.seek(end);
            }
        }
        Ok(Set { count, elem_type, items })
    }

    fn map(&mut self, tag: &Tag, depth: usize, path: &str) -> Result<Map, ParseError> {
        if depth > MAX_DEPTH {
            return Err(self.err(ErrorKind::TooDeep, format!("{path} 嵌套太深")));
        }
        let body_start = self.pos;
        let pad = self.i32("映射前导 0")?;
        if pad != 0 {
            self.note(path, NoteKind::Assumed, format!("映射前导 4 字节是 {pad}，不是 0"));
        }
        let count = self.i32("映射条目数")?;
        let key_type = tag.inner_type.clone().unwrap_or_default();
        let value_type = tag.value_type.clone().unwrap_or_default();
        if count < 0 {
            return Err(self.err(ErrorKind::BadCount, format!("{path} 条目数是 {count}")));
        }
        let mut entries = Vec::new();
        for i in 0..count {
            self.spend()?;
            let here = self.pos;
            let item_path = format!("{path}{{{i}}}");
            let key = if key_type == "StructProperty" {
                let bytes = self.take(16, "映射键 GUID")?.to_vec();
                Value::Binary(Binary { ty: "Guid".to_string(), bytes })
            } else {
                let bare = Tag { ty: key_type.clone(), ..Tag::default() };
                self.raw_value(&bare, depth + 1, &item_path)?
            };
            let value = if value_type == "StructProperty" {
                let props = self.property_list(depth + 1, &item_path)?;
                Value::Struct(StructValue { name: None, props })
            } else {
                let bare = Tag { ty: value_type.clone(), ..Tag::default() };
                self.raw_value(&bare, depth + 1, &item_path)?
            };
            entries.push((key, value));
            if self.pos == here {
                break;
            }
        }
        if tag.size > 0 {
            let end = body_start + tag.size as usize;
            if end <= self.buf.len() && self.pos != end {
                self.note(path, NoteKind::Aligned, format!("映射读完在 {}，标签说 {end}，按标签对齐", self.pos));
                self.seek(end);
            }
        }
        Ok(Map { count, key_type, value_type, entries })
    }
}

pub fn parse(bytes: &[u8]) -> Result<ParseOutcome, ParseError> {
    let mut r = Reader::new(bytes);
    let header = r.header()?;
    let props = r.property_list(0, "")?;
    Ok(ParseOutcome {
        header,
        props,
        consumed: r.pos,
        stats: ParseStats { nodes: r.nodes, skipped: r.skipped },
        notes: r.notes,
    })
}
