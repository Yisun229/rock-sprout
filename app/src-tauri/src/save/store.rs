//! 只读四步 + 缓存：这是这一版对用户的**核心承诺**，所以每一步都留痕。
//!
//! ```text
//! 1. compute_sha256   先给原档算一次 sha256
//! 2. copy_to_cache    复制一份到缓存目录，原档此后不再被碰
//! 3. parse_copy       解析的是**副本**，不是原档
//! 4. verify_sha256    再给原档算一次，和第一步比；变了就中止
//! ```
//!
//! 解析结果按「原档 sha256 的前 16 位」缓存成 `<缓存根>\saves\<前16位>.json`，
//! 第二次读同一份存档直接命中，不再复制、不再解析。
//! 缓存里不含时间戳，同一份存档两次写出来的字节完全一致——可以直接比对字节证明。

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::model::{self, SaveData};
use super::{parse, ErrorKind, ParseError};

/// 读存档失败的三类。**不要合并成一句「解析失败」**：用户下一步该做什么完全不同。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadClass {
    /// 文件根本不是这个游戏的存档
    NotSave,
    /// 是这个游戏的存档，但版本不认识（多半是游戏更新了）
    UnknownVersion,
    /// 版本认得，但内容读不通（文件坏了 / 被别的工具改过）
    BadStructure,
}

impl LoadClass {
    pub fn code(self) -> &'static str {
        match self {
            LoadClass::NotSave => "not_save",
            LoadClass::UnknownVersion => "unknown_version",
            LoadClass::BadStructure => "bad_structure",
        }
    }

    pub fn human(self) -> &'static str {
        match self {
            LoadClass::NotSave => "这不是《深岩银河》的存档",
            LoadClass::UnknownVersion => "存档版本不认识，可能是游戏更新了",
            LoadClass::BadStructure => "存档内容读不通，可能已经损坏",
        }
    }
}

/// 读存档的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadError {
    pub class: LoadClass,
    pub detail: String,
}

impl LoadError {
    pub fn new(class: LoadClass, detail: impl Into<String>) -> Self {
        Self { class, detail: detail.into() }
    }

    fn io(what: &str, err: io::Error) -> Self {
        Self::new(LoadClass::BadStructure, format!("{what}：{err}"))
    }
}

impl From<ParseError> for LoadError {
    fn from(e: ParseError) -> Self {
        let class = match e.kind {
            ErrorKind::NotGvas => LoadClass::NotSave,
            ErrorKind::UnknownVersion => LoadClass::UnknownVersion,
            _ => LoadClass::BadStructure,
        };
        Self::new(class, e.detail)
    }
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}（{}）：{}", self.class.human(), self.class.code(), self.detail)
    }
}

impl std::error::Error for LoadError {}

/// 一次读取的过程记录：两个哈希 + 是不是命中缓存 + 解析数字。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadReport {
    /// 第 1 步算出来的原档 sha256
    pub sha256_before: String,
    /// 第 4 步再算一次的原档 sha256（必须和上面一样）
    pub sha256_after: String,
    /// 副本的 sha256（只有新解析时有；命中缓存是 `None`）
    pub sha256_copy: Option<String>,
    /// true = 命中缓存，没有复制也没有解析
    pub from_cache: bool,
    /// 缓存文件位置
    pub cache_path: String,
    /// 缓存文件里的字节数
    pub cache_bytes: usize,
    pub nodes: usize,
    pub consumed: usize,
    pub file_bytes: usize,
    pub skipped: usize,
    pub notes: usize,
}

impl LoadReport {
    /// 原档在读取前后是否完全没变。
    pub fn untouched(&self) -> bool {
        self.sha256_before == self.sha256_after
    }

    /// 这次读取的性质，给日志和自检工具用。
    pub fn kind_text(&self) -> &'static str {
        if self.from_cache { "命中缓存" } else { "新解析" }
    }
}

/// 缓存格式版本。解析器行为一变就要 +1：缓存按存档 sha256 命名，跟代码版本无关，
/// 不升这个号就会一直读到旧解析器写下的结果（比如「GUID 全是 0」那种）。
pub const CACHE_VERSION: u32 = 2;

/// 一份存档的解析结果（也就是缓存文件的内容）。
///
/// **字段顺序固定、不含时间戳**——同一份存档写出来的字节永远一样，
/// 所以「跑两次结果字节级相同」可以直接比对字节来证明。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Doc {
    /// 写这份缓存时的缓存格式版本
    pub cache_version: u32,
    /// 原档完整 sha256
    pub sha256: String,
    pub file_bytes: usize,
    pub nodes: usize,
    pub consumed: usize,
    pub skipped: usize,
    pub notes: usize,
    pub data: SaveData,
}

/// 读一份存档。原档只会被「读」，不会被打开成可写。
pub fn load(path: &Path, cache_root: &Path) -> Result<(Doc, LoadReport), LoadError> {
    // 第 1 步：原档算哈希
    let sha_before = sha256_file(path).map_err(|e| LoadError::io("打不开存档", e))?;
    let saves_dir = cache_root.join("saves");
    let cache_path = saves_dir.join(format!("{}.json", &sha_before[..16]));

    // 命中缓存：不复制、不解析，只把原档再算一次哈希证明没动过
    if let Some((doc, cache_bytes)) = read_cache(&cache_path, &sha_before) {
        let sha_after = sha256_file(path).map_err(|e| LoadError::io("读不到存档", e))?;
        let report = LoadReport {
            sha256_before: sha_before,
            sha256_after: sha_after,
            sha256_copy: None,
            from_cache: true,
            cache_path: cache_path.display().to_string(),
            cache_bytes,
            nodes: doc.nodes,
            consumed: doc.consumed,
            file_bytes: doc.file_bytes,
            skipped: doc.skipped,
            notes: doc.notes,
        };
        return Ok((doc, report));
    }

    // 第 2 步：复制一份到缓存目录（临时名，成功后改名，避免半截文件被当成缓存）
    fs::create_dir_all(&saves_dir).map_err(|e| LoadError::io("建不了缓存目录", e))?;
    let tmp_save = saves_dir.join(format!(".tmp-{}.sav", std::process::id()));
    fs::copy(path, &tmp_save).map_err(|e| LoadError::io("复制存档失败", e))?;

    // 复制完先验一次：副本必须和原档一模一样
    let sha_copy = sha256_file(&tmp_save).map_err(|e| LoadError::io("读不到副本", e))?;
    if sha_copy != sha_before {
        let _ = fs::remove_file(&tmp_save);
        return Err(LoadError::new(
            LoadClass::BadStructure,
            "复制出来的副本和原档对不上（原档可能在复制过程中被改动）",
        ));
    }

    // 第 3 步：解析**副本**
    let bytes = fs::read(&tmp_save).map_err(|e| LoadError::io("读不了副本", e))?;
    let file_bytes = bytes.len();
    let parsed = match parse(&bytes) {
        Ok(p) => p,
        Err(e) => {
            let _ = fs::remove_file(&tmp_save);
            return Err(e.into());
        }
    };
    let _ = fs::remove_file(&tmp_save);

    // 第 4 步：原档再算一次，变了就中止（结果不落缓存）
    let sha_after = sha256_file(path).map_err(|e| LoadError::io("读不到存档", e))?;
    if sha_after != sha_before {
        return Err(LoadError::new(
            LoadClass::BadStructure,
            format!("存档在读取过程中变了（{sha_before} → {sha_after}），这次结果不采用"),
        ));
    }

    let doc = Doc {
        cache_version: CACHE_VERSION,
        sha256: sha_before.clone(),
        file_bytes,
        nodes: parsed.stats.nodes,
        consumed: parsed.consumed,
        skipped: parsed.stats.skipped,
        notes: parsed.notes.len(),
        data: model::build(&parsed.props),
    };
    let cache_bytes = write_cache(&cache_path, &doc).map_err(|e| LoadError::io("写不了缓存", e))?;

    let report = LoadReport {
        sha256_before: sha_before,
        sha256_after: sha_after,
        sha256_copy: Some(sha_copy),
        from_cache: false,
        cache_path: cache_path.display().to_string(),
        cache_bytes,
        nodes: doc.nodes,
        consumed: doc.consumed,
        file_bytes,
        skipped: doc.skipped,
        notes: doc.notes,
    };
    Ok((doc, report))
}

fn read_cache(path: &Path, expect_sha: &str) -> Option<(Doc, usize)> {
    let bytes = fs::read(path).ok()?;
    let doc: Doc = serde_json::from_slice(&bytes).ok()?;
    // 缓存格式版本对不上就当作没有缓存（旧解析器的结果不能拿来用）
    if doc.cache_version != CACHE_VERSION {
        return None;
    }
    // 按 sha 前 16 位找文件，这里再核对完整哈希，防止前缀撞车或缓存被换过
    if doc.sha256 != expect_sha {
        return None;
    }
    let len = bytes.len();
    Some((doc, len))
}

fn write_cache(path: &Path, doc: &Doc) -> io::Result<usize> {
    let body = render(doc)?;
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&body)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(body.len())
}

/// 把一份结果渲染成缓存文件该有的字节（末尾带换行）。
///
/// 写缓存和「验证两次结果一模一样」用的是同一个函数，两边不可能各写一套。
pub fn render(doc: &Doc) -> io::Result<Vec<u8>> {
    let mut body = serde_json::to_vec_pretty(doc).map_err(io::Error::other)?;
    body.push(b'\n');
    Ok(body)
}

/// 把存档缓存整个清掉（「清理缓存」用），返回删掉了几个文件。
pub fn clear(cache_root: &Path) -> io::Result<usize> {
    let saves = cache_root.join("saves");
    if !saves.exists() {
        return Ok(0);
    }
    let mut removed = 0;
    for entry in fs::read_dir(&saves)? {
        let entry = entry?;
        if entry.path().is_file() {
            fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// 算文件的 sha256（小写十六进制）。
pub fn sha256_file(path: &Path) -> io::Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(super::model::hex(&hasher.finalize()))
}

/// 缓存文件路径（给测试和诊断用）。
pub fn cache_path_for(cache_root: &Path, sha256: &str) -> PathBuf {
    cache_root.join("saves").join(format!("{}.json", &sha256[..16]))
}
