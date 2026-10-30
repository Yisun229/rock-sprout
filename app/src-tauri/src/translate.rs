//! 把英文简介翻成中文。
//!
//! 走 Google 那个免密钥的 GET 接口（`translate_a/single`），不花钱也不用注册。
//! 三件事这里管：**按句分块**（长文本一次塞不进 URL）、**串行请求**（不并发打扰对方）、
//! **翻过的存本地**（同一段原文再翻直接给缓存，不再发请求）。

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 默认端点。用户可以在设置里换成自己的（形状一样：GET + `q=`）。
pub const DEFAULT_ENDPOINT: &str =
    "https://translate.googleapis.com/translate_a/single?client=gtx&sl=en&tl=zh-CN&dt=t&q=";

/// 默认目标语言。
pub const DEFAULT_TARGET: &str = "zh-CN";

/// 单块上限（字符）。实测 3,400 字符的 URL 也还能用，1,500 留足余量。
pub const MAX_CHUNK: usize = 1500;

/// 原文超过这么多字符直接拒，不发请求。
pub const MAX_INPUT: usize = 10_000;

/// 翻译出的错，分五类——界面上说的话不一样，用户该做的事也不一样。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TranslateError {
    /// 机器可读的分类：`unauthorized` / `rate_limited` / `network` / `decode` / `too_long`
    pub code: &'static str,
    /// 给人看的一句话
    pub message: String,
    /// 被限速时对方建议等多久（秒）
    pub retry_after: Option<u64>,
}

impl TranslateError {
    fn unauthorized() -> Self {
        Self {
            code: "unauthorized",
            message: "翻译服务不认这个请求（这个地址要密钥？换成默认地址试试）".to_owned(),
            retry_after: None,
        }
    }

    fn rate_limited(retry_after: Option<u64>) -> Self {
        let message = match retry_after {
            Some(sec) => format!("翻译请求太勤，被限速了，{sec} 秒后再试"),
            None => "翻译请求太勤，被限速了，等一会儿再试".to_owned(),
        };
        Self {
            code: "rate_limited",
            message,
            retry_after,
        }
    }

    fn network(msg: impl Into<String>) -> Self {
        Self {
            code: "network",
            message: format!("连不上翻译服务：{}", msg.into()),
            retry_after: None,
        }
    }

    fn decode(msg: impl Into<String>) -> Self {
        Self {
            code: "decode",
            message: format!("翻译服务回了看不懂的内容：{}", msg.into()),
            retry_after: None,
        }
    }

    fn too_long(len: usize) -> Self {
        Self {
            code: "too_long",
            message: format!("这段太长了（{len} 字），超过 {MAX_INPUT} 字不翻"),
            retry_after: None,
        }
    }
}

/// 按句切块：每块不超过 [`MAX_CHUNK`] 个字符，句号/感叹号/问号处优先断开。
///
/// 单句本身就超长时硬切——总得有兜底，否则一整段没有标点的简介就发不出去了。
pub fn split_chunks(text: &str) -> Vec<String> {
    let sentences = split_sentences(text);
    let mut chunks: Vec<String> = Vec::new();
    let mut cur = String::new();
    for s in sentences {
        if s.chars().count() > MAX_CHUNK {
            // 先把攒着的发出去，再把这个超长句硬切成几段
            if !cur.is_empty() {
                chunks.push(std::mem::take(&mut cur));
            }
            chunks.extend(hard_split(&s, MAX_CHUNK));
            continue;
        }
        if cur.chars().count() + s.chars().count() > MAX_CHUNK {
            chunks.push(std::mem::take(&mut cur));
        }
        cur.push_str(&s);
    }
    if !cur.trim().is_empty() {
        chunks.push(cur);
    }
    chunks
}

/// 把整段拆成句子（保留句子末尾的空白，拼起来跟原文一样）。
fn split_sentences(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        cur.push(c);
        let boundary = matches!(c, '.' | '!' | '?') && chars.peek().map(|n| n.is_whitespace()).unwrap_or(true);
        if boundary {
            // 把紧跟的空白一起收进来，避免拼接时丢空格
            while let Some(n) = chars.peek() {
                if n.is_whitespace() {
                    cur.push(*n);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// 按字符数硬切（不切坏多字节字符）。
fn hard_split(text: &str, size: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut n = 0;
    for c in text.chars() {
        cur.push(c);
        n += 1;
        if n == size {
            out.push(std::mem::take(&mut cur));
            n = 0;
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// 原文的指纹（sha256 前 16 位十六进制）。缓存键里用它。
pub fn hash16(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    hex.chars().take(16).collect()
}

/// 缓存里存的东西。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedTranslation {
    /// 原文指纹，读缓存时再核一遍，防止键撞上
    pub source_hash: String,
    pub target: String,
    pub text: String,
    /// 翻好的时刻（unix 秒）
    pub translated_at: u64,
}

/// 缓存文件路径：`<目录>/<键>.json`。
pub fn cache_path(dir: &Path, key: &str) -> PathBuf {
    dir.join(format!("{key}.json"))
}

/// 读缓存。指纹或目标语言对不上就当没命中。
pub fn load_cached(dir: &Path, key: &str, source_hash: &str, target: &str) -> Option<String> {
    let raw = std::fs::read_to_string(cache_path(dir, key)).ok()?;
    let cached: CachedTranslation = serde_json::from_str(&raw).ok()?;
    if cached.source_hash != source_hash || cached.target != target {
        return None;
    }
    Some(cached.text)
}

/// 写缓存。写不进去只让调用方记日志，不影响这次结果。
pub fn save_cached(dir: &Path, key: &str, entry: &CachedTranslation) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let body = serde_json::to_string(entry)?;
    std::fs::write(cache_path(dir, key), body)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 把端点里的 `tl=` 换成目标语言；端点里没有就补一个。
fn with_target(endpoint: &str, target: &str) -> String {
    if let Some(pos) = endpoint.find("tl=") {
        let rest = &endpoint[pos + 3..];
        let end = rest.find('&').map(|i| pos + 3 + i).unwrap_or(endpoint.len());
        format!("{}tl={}{}", &endpoint[..pos], target, &endpoint[end..])
    } else if endpoint.contains('?') {
        format!("{endpoint}&tl={target}")
    } else {
        format!("{endpoint}?tl={target}")
    }
}

/// 从 Google 那个形状的返回里抠出译文。
///
/// 形状是 `[[["中文","原文",…], ["中文","原文",…]], null, "en", …]`：
/// 取 `[0]` 这个数组，把每一段的第 0 项按顺序拼起来。
fn parse_translation(body: &str) -> Result<String, TranslateError> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| TranslateError::decode(e.to_string()))?;
    let segments = value
        .get(0)
        .and_then(|v| v.as_array())
        .ok_or_else(|| TranslateError::decode("返回里没有译文数组"))?;
    let mut out = String::new();
    for seg in segments {
        if let Some(piece) = seg.get(0).and_then(|v| v.as_str()) {
            out.push_str(piece);
        }
    }
    if out.is_empty() {
        return Err(TranslateError::decode("译文是空的"));
    }
    Ok(out)
}

/// 去掉 HTML 标签、压掉多余空白。拿 `description`（富文本）兜底时才用得上。
///
/// 标签的位置换成空格（`a<br>b` 要变成 `a b` 而不是 `ab`），再按空白归一。
pub fn plain_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 翻一段文本。空文本直接回空串，不发请求。
pub async fn translate(
    text: &str,
    target: &str,
    endpoint: &str,
) -> Result<String, TranslateError> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(String::new());
    }
    let len = text.chars().count();
    if len > MAX_INPUT {
        return Err(TranslateError::too_long(len));
    }

    let client = reqwest::Client::builder()
        .user_agent(format!("RockSprout/{}", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| TranslateError::network(e.to_string()))?;

    let base = with_target(endpoint, target);
    let mut out = String::new();
    // 逐块串行：并行请求更容易被限速，也不利于把错对应到块上
    for chunk in split_chunks(text) {
        if chunk.trim().is_empty() {
            continue;
        }
        let url = format!("{base}{}", crate::modio::urlencode(&chunk));
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| TranslateError::network(crate::modio::describe(&e)))?;
        let status = resp.status();
        if status.is_success() {
            let body = resp
                .text()
                .await
                .map_err(|e| TranslateError::network(crate::modio::describe(&e)))?;
            out.push_str(&parse_translation(&body)?);
            continue;
        }
        return Err(match status.as_u16() {
            401 | 403 => TranslateError::unauthorized(),
            429 => {
                let retry = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.trim().parse::<u64>().ok());
                TranslateError::rate_limited(retry)
            }
            other => TranslateError::network(format!("翻译服务返回 HTTP {other}")),
        });
    }
    Ok(out)
}

/// 带缓存的翻译：先看本地，没有才发请求，翻好再存。
///
/// `key` 由调用方给（详情页给的是 `<mod id>-<原文指纹>`），这样同一个 mod 的不同版本
/// 各存各的，不会互相顶掉。
pub async fn translate_cached(
    dir: &Path,
    key: &str,
    text: &str,
    target: &str,
    endpoint: &str,
) -> Result<(String, bool), TranslateError> {
    let source_hash = hash16(text);
    if let Some(hit) = load_cached(dir, key, &source_hash, target) {
        return Ok((hit, true));
    }
    let translated = translate(text, target, endpoint).await?;
    let entry = CachedTranslation {
        source_hash,
        target: target.to_owned(),
        text: translated.clone(),
        translated_at: now_secs(),
    };
    if let Err(e) = save_cached(dir, key, &entry) {
        tracing::warn!(%e, "写翻译缓存失败，这次结果照常返回");
    }
    Ok((translated, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 空文本不翻() {
        assert!(split_chunks("").is_empty());
        assert!(split_chunks("   ").is_empty());
    }

    #[test]
    fn 短句拼成一块() {
        let chunks = split_chunks("One. Two. Three.");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "One. Two. Three.");
    }

    #[test]
    fn 超过上限就另起一块() {
        // 造两句各自 900 字，合起来 1800 超过 1500，应该切成两块
        let sentence = |c: char| format!("{}{}", c.to_string().repeat(900), ". ");
        let text = format!("{}{}", sentence('a'), sentence('b'));
        let chunks = split_chunks(&text);
        assert_eq!(chunks.len(), 2, "应该切成两块");
        assert!(chunks.iter().all(|c| c.chars().count() <= MAX_CHUNK + 2));
    }

    #[test]
    fn 单句超长也硬切() {
        let text = "x".repeat(4000);
        let chunks = split_chunks(&text);
        assert!(chunks.len() >= 3, "4000 字按 1500 切至少三块");
        let total: usize = chunks.iter().map(|c| c.chars().count()).sum();
        assert_eq!(total, 4000, "切完拼回来字数不能少");
    }

    #[test]
    fn 目标语言会替换端点里的tl() {
        let ep = "https://translate.googleapis.com/translate_a/single?client=gtx&sl=en&tl=zh-CN&dt=t&q=";
        let out = with_target(ep, "ja");
        assert!(out.contains("tl=ja"), "应该换成 ja：{out}");
        assert!(!out.contains("tl=zh-CN"));
        assert!(out.ends_with("&q="), "结尾的 q= 要留着");
    }

    #[test]
    fn 端点没有tl就补一个() {
        let out = with_target("https://example.com/t?sl=en&q=", "zh-CN");
        assert!(out.contains("tl=zh-CN"));
    }

    #[test]
    fn 解析谷歌那种返回形状() {
        let body = r#"[[["你好","Hello",null,null,10],["世界","World",null,null,10]],null,"en"]"#;
        assert_eq!(parse_translation(body).unwrap(), "你好世界");
    }

    #[test]
    fn 返回不认识的形状要说清楚() {
        let err = parse_translation(r#"{"error":"no"}"#).unwrap_err();
        assert_eq!(err.code, "decode");
    }

    #[test]
    fn 超长原文不发请求直接拒() {
        let long = "a".repeat(MAX_INPUT + 1);
        let err = futures_lite_block_on(translate(&long, "zh-CN", DEFAULT_ENDPOINT)).unwrap_err();
        assert_eq!(err.code, "too_long");
    }

    #[test]
    fn 缓存命中要指纹和目标语言都对得上() {
        let dir = std::env::temp_dir().join(format!("rs-tr-{}", hash16("cache-test")));
        let _ = std::fs::remove_dir_all(&dir);
        let text = "Hello world.";
        let key = format!("42-{}", hash16(text));
        save_cached(
            &dir,
            &key,
            &CachedTranslation {
                source_hash: hash16(text),
                target: "zh-CN".to_owned(),
                text: "你好，世界。".to_owned(),
                translated_at: 1,
            },
        )
        .unwrap();
        assert_eq!(
            load_cached(&dir, &key, &hash16(text), "zh-CN").as_deref(),
            Some("你好，世界。")
        );
        // 目标语言变了就不该命中
        assert!(load_cached(&dir, &key, &hash16(text), "ja").is_none());
        // 原文变了也不该命中
        assert!(load_cached(&dir, &key, &hash16("Other text."), "zh-CN").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 富文本兜底去标签() {
        assert_eq!(plain_text("<p>a<br>b</p>  c"), "a b c");
    }

    /// 测试里要跑一次 async 判断（超长那条），用最小 executor，不引新依赖。
    fn futures_lite_block_on<F: std::future::Future>(fut: F) -> F::Output {
        use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
        fn noop(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker {
            RawWaker::new(std::ptr::null(), &VTABLE)
        }
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
        let waker = unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) };
        let mut cx = Context::from_waker(&waker);
        let mut fut = std::pin::pin!(fut);
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                return v;
            }
        }
    }
}
