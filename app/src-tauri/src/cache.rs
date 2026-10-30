//! 离线缓存：把搜索结果和 mod 详情各存一份在本地，断网时还能看。
//!
//! 存法很朴素——一个 JSON 一个文件，放在 `<数据目录>\\cache\\` 下面：
//!
//! - 搜索结果：`cache\\search\\<关键词>-<短哈希>-p<页码>.json`
//! - mod 详情：`cache\\detail\\<mod id>.json`
//!
//! 每个文件里除了数据本身，还记着「什么时候抓的」（`cached_at`，unix 毫秒），
//! 界面据此告诉用户「这是几点几分的数据」。
//!
//! 为什么不用数据库：现在数据量很小（一页 20 条 + 本地几个已装 mod），
//! 用文件零依赖、出问题能直接打开看。等以后存档看板要查大量数据了再换 SQLite——
//! 到那时只改这一个文件，外面看到的接口不变。

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::modio::{ModDetail, SearchPage};
use crate::paths;

/// 缓存里的一条：值本身 + 抓取时间。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fresh<T> {
    /// 抓下来的时刻（unix 毫秒）
    pub cached_at: u64,
    pub value: T,
}

/// 缓存目录的门把手。
///
/// 正常用 [`Cache::at_data_dir`]；测试里给一个临时目录，别碰真数据。
#[derive(Debug, Clone)]
pub struct Cache {
    root: PathBuf,
}

impl Cache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// 正式用的那个：`<数据目录>\\cache\\`。
    pub fn at_data_dir() -> Self {
        Self::new(paths::data_root().join("cache"))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn search_dir(&self) -> PathBuf {
        self.root.join("search")
    }

    fn detail_dir(&self) -> PathBuf {
        self.root.join("detail")
    }

    // ---- 搜索结果 ----

    /// 存一页搜索结果。写失败不影响主流程，调用方可以不管返回值。
    pub fn save_search(&self, query: &str, page: u32, data: &SearchPage) -> io::Result<PathBuf> {
        let path = self
            .search_dir()
            .join(search_file_name(query, page));
        write_json(&path, &Fresh::stamp(data))?;
        Ok(path)
    }

    /// 读一页缓存。没存过、或者文件坏了，都返回 `None`（坏文件不该让程序崩）。
    pub fn load_search(&self, query: &str, page: u32) -> Option<Fresh<SearchPage>> {
        let path = self
            .search_dir()
            .join(search_file_name(query, page));
        read_json(&path)
    }

    // ---- mod 详情 ----

    pub fn save_detail(&self, detail: &ModDetail) -> io::Result<PathBuf> {
        let path = self.detail_dir().join(format!("{}.json", detail.id));
        write_json(&path, &Fresh::stamp(detail))?;
        Ok(path)
    }

    pub fn load_detail(&self, id: u64) -> Option<Fresh<ModDetail>> {
        read_json(&self.detail_dir().join(format!("{id}.json")))
    }
}

impl<T> Fresh<T> {
    pub fn stamp(value: T) -> Self {
        Self {
            cached_at: now_ms(),
            value,
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 搜索词 + 页码 → 文件名。
///
/// 关键词是用户随手打的，可能有 `/`、`:`、空格甚至换行，不能直接当文件名；
/// 先把它们换成 `_`，再拼一个短哈希——这样「钻石 矿」和「钻石-矿」不会撞到同一个文件。
fn search_file_name(query: &str, page: u32) -> String {
    let trimmed = query.trim();
    let mut safe: String = if trimmed.is_empty() {
        "热门".to_owned()
    } else {
        trimmed
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .take(32)
            .collect()
    };
    if safe.is_empty() {
        safe.push_str("热门");
    }
    format!("{}-{}-p{}.json", safe, short_hash(trimmed), page)
}

/// FNV-1a 32 位：够短、够散，纯粹用来防同名，不是安全用途。
fn short_hash(text: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in text.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("{hash:08x}")
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string(value).map_err(io::Error::other)?;
    std::fs::write(path, text)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modio::{ModSummary, SearchPage};

    /// 测试用的临时目录放在构建产物目录下，不写 C 盘。
    fn test_cache(name: &str) -> Cache {
        let base = std::env::var("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("target"));
        let dir = base.join("rock-sprout-tests").join("cache").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        Cache::new(dir)
    }

    fn sample_page() -> SearchPage {
        SearchPage {
            query: "钻石".to_owned(),
            page: 1,
            total: 1,
            items: vec![ModSummary {
                id: 1179441,
                name: "Brighter Objects".to_owned(),
                summary: "更亮的矿物".to_owned(),
                downloads: 2_329_030,
            }],
            stale: false,
            cached_at: None,
        }
    }

    #[test]
    fn 搜索结果写进去能读回来() {
        let cache = test_cache("search-roundtrip");
        cache
            .save_search("钻石", 1, &sample_page())
            .expect("写搜索缓存");

        let fresh = cache.load_search("钻石", 1).expect("读搜索缓存");
        assert_eq!(fresh.value.total, 1);
        assert_eq!(fresh.value.items[0].name, "Brighter Objects");
        assert!(fresh.cached_at > 0, "要记下抓取时间");
    }

    #[test]
    fn 换一页就换一个文件不会串() {
        let cache = test_cache("search-pages");
        cache.save_search("钻石", 1, &sample_page()).expect("第 1 页");
        assert!(cache.load_search("钻石", 2).is_none(), "第 2 页没存过");
    }

    #[test]
    fn 关键词里有斜杠也只落在缓存目录里() {
        let cache = test_cache("search-slashes");
        let path = cache
            .save_search("../../evil", 1, &sample_page())
            .expect("写搜索缓存");
        assert_eq!(path.parent(), Some(cache.root().join("search").as_path()));
    }

    #[test]
    fn 详情写进去能读回来() {
        let cache = test_cache("detail-roundtrip");
        let detail = ModDetail {
            id: 7,
            name: "测试 mod".to_owned(),
            summary: "一句话".to_owned(),
            description: "长描述".to_owned(),
            description_plaintext: "长描述".to_owned(),
            downloads: 12,
            version: "1.0.0".to_owned(),
            logo_320: String::new(),
            logo_640: String::new(),
            creator: "someone".to_owned(),
            date_updated: 0,
            tags: Vec::new(),
            stale: false,
            cached_at: None,
        };
        cache.save_detail(&detail).expect("写详情缓存");

        let fresh = cache.load_detail(7).expect("读详情缓存");
        assert_eq!(fresh.value.version, "1.0.0");
    }

    #[test]
    fn 缓存文件被写坏时返回空而不是崩溃() {
        let cache = test_cache("corrupt");
        let path = cache
            .save_search("钻石", 1, &sample_page())
            .expect("先写一份");
        std::fs::write(&path, "{ 这不是合法 JSON").expect("故意写坏");

        assert!(cache.load_search("钻石", 1).is_none());
    }
}
