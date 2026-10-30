//! mod.io 接口客户端（M2）。
//!
//! 设计取舍：
//!
//! - **用令牌当身份，不写死 uid**。所有请求打到 `https://api.mod.io/v1`，带着用户自己的
//!   个人访问令牌；令牌是谁的，服务器就按谁算。另外提供 [`Modio::me`] 显式查一次 `/me`，
//!   界面可以显示「当前登录：某某」，也用来验证令牌有没有过期。
//! - **一条命令只发一次请求**，不做并发；被限速（429）时按 `Retry-After` 等一次再试，
//!   避免把用户 IP 打到临时封禁。
//! - 游戏号只出现在 [`GAME_ID`] 这一个常量里，换游戏只改一行。

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// 深岩银河在 mod.io 上的游戏号。
pub const GAME_ID: u64 = 2475;

/// mod.io 的 Type 组只有这 9 个真实标签名。
///
/// 筛选参数只认真实名字：写一个不存在的（比如 `Weapon`）整条请求直接 422。
/// 所以界面上只允许从这张表里挑，不允许手输。
pub const TYPE_TAGS: [&str; 9] = [
    "Cosmetic",
    "Audio",
    "Framework",
    "Gameplay",
    "QoL",
    "Visual",
    "Tools",
    "Auto-Verified",
    "Animation",
];

const API_ROOT: &str = "https://api.mod.io/v1";

/// 一次请求遇到的所有失败。前端把它翻成 [`crate::ipc::ApiError`]。
#[derive(Debug)]
pub enum ModioError {
    /// 还没配置令牌（用户没登录）
    NoToken,
    /// 令牌无效、过期，或没有权限
    Unauthorized,
    /// 被 mod.io 限速
    RateLimited { retry_after_secs: Option<u64> },
    /// 连不上：断网、DNS 污染、超时
    Network(String),
    /// 对象不存在
    NotFound,
    /// 其它非 2xx
    Http { status: u16, body: String },
    /// 本地读写失败（落盘、读令牌文件）
    Io(String),
}

impl ModioError {
    /// 给前端判断用的错误码，和 `ApiError.code` 一致。
    pub fn code(&self) -> &'static str {
        match self {
            ModioError::NoToken => "no_token",
            ModioError::Unauthorized => "unauthorized",
            ModioError::RateLimited { .. } => "rate_limited",
            ModioError::Network(_) => "network",
            ModioError::NotFound => "not_found",
            ModioError::Http { .. } => "http",
            ModioError::Io(_) => "io",
        }
    }

    /// 给人看的中文说明。断网和没登录要分开说，用户才知道该去检查什么。
    pub fn message(&self) -> String {
        match self {
            ModioError::NoToken => "还没填 mod.io 访问令牌，请到设置页粘贴".to_owned(),
            ModioError::Unauthorized => "令牌无效或已过期，请到 mod.io 重新生成".to_owned(),
            ModioError::RateLimited { retry_after_secs } => match retry_after_secs {
                Some(s) => format!("请求太频繁，等 {s} 秒后自动重试"),
                None => "请求太频繁，稍后再试".to_owned(),
            },
            ModioError::Network(msg) => {
                format!("连不上 mod.io（{msg}）。请检查网络，必要时开启本地代理工具")
            }
            ModioError::NotFound => "这个 mod 不存在，可能已被作者删除".to_owned(),
            // 422 基本都是筛选条件里有不存在的标签名。原样甩一段 JSON 给用户看没用，
            // 这里说人话，细节留在日志里。
            ModioError::Http { status: 422, .. } => {
                "筛选条件里有 mod.io 不认识的标签，换一个再试".to_owned()
            }
            ModioError::Http { status, body } => format!("mod.io 返回 {status}：{body}"),
            ModioError::Io(msg) => format!("本地文件读写失败：{msg}"),
        }
    }
}

/// `/me` 返回的登录身份。
#[derive(Debug, Clone, Deserialize)]
pub struct Me {
    pub id: u64,
    #[serde(default)]
    pub username: String,
}

/// 搜索结果里的一条。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModSummary {
    pub id: u64,
    pub name: String,
    pub summary: String,
    pub downloads: u64,
}

/// 一页搜索结果。
///
/// `stale` 和 `cached_at` 只有走离线缓存时才不是默认值：断网时后端会回放上次存下的结果，
/// 并把抓取时间一起带上来，界面据此提示「这是几点几分的数据」。
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchPage {
    pub query: String,
    pub page: u32,
    pub total: u32,
    pub items: Vec<ModSummary>,
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub cached_at: Option<u64>,
}

/// mod 详情。
#[derive(Debug, Serialize, Deserialize)]
pub struct ModDetail {
    pub id: u64,
    pub name: String,
    pub summary: String,
    pub description: String,
    /// 纯文本简介。比 `description` 的富文本好排版，面板直接用这一份。
    #[serde(default)]
    pub description_plaintext: String,
    pub downloads: u64,
    pub version: String,
    /// 封面小图（320×180），面板和列表都能用
    #[serde(default)]
    pub logo_320: String,
    /// 封面大图（640×360），面板宽的时候用
    #[serde(default)]
    pub logo_640: String,
    /// 作者用户名（`submitted_by.username`）
    #[serde(default)]
    pub creator: String,
    /// 最近一次更新时刻（unix 秒），0 表示接口没给
    #[serde(default)]
    pub date_updated: u64,
    /// 只保留 Type 组那 9 个真实标签名，别的组（版本号、审核状态、内部标记）不要
    #[serde(default)]
    pub tags: Vec<String>,
    /// 见 [`SearchPage::stale`]
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub cached_at: Option<u64>,
}

/// 某个 mod 的一个可下载文件。
#[derive(Debug, Clone, Serialize)]
pub struct ModFile {
    pub id: u64,
    pub filename: String,
    pub size: u64,
    /// 下载地址。mod.io 的下载链接带过期时间，不要存起来复用。
    #[serde(skip)]
    pub url: String,
}

/// 下载结果，给前端显示进度用。
#[derive(Debug, Serialize)]
pub struct DownloadOutcome {
    pub file_id: u64,
    pub filename: String,
    pub size: u64,
    /// 落地位置（绝对路径）
    pub path: String,
}

// ---- mod.io 的原始返回结构（只取用得到的字段，其余靠 serde 忽略）----

#[derive(Debug, Deserialize)]
struct ListEnvelope<T> {
    data: Vec<T>,
    /// 这一页里有几条。**不是总数**，别拿它当总数用。
    #[serde(default)]
    result_count: u64,
    /// 符合条件的总条数。翻页提示「共 N 个」要用这个。
    #[serde(default)]
    result_total: u64,
}

#[derive(Debug, Deserialize)]
struct RawMod {
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    description_plaintext: String,
    #[serde(default)]
    stats: RawStats,
    /// 上架时刻（unix 秒）。老接口没给就是 0
    #[serde(default)]
    date_added: u64,
    /// 最近一次更新时刻（unix 秒）
    #[serde(default)]
    date_updated: u64,
    #[serde(default)]
    logo: RawLogo,
    #[serde(default)]
    submitted_by: RawUser,
    /// 全部标签（按组分好，我们只挑 Type 组那 9 个）
    #[serde(default)]
    tags: Vec<RawTag>,
}

#[derive(Debug, Default, Deserialize)]
struct RawLogo {
    #[serde(default, rename = "thumb_320x180")]
    thumb_320: String,
    #[serde(default, rename = "thumb_640x360")]
    thumb_640: String,
}

#[derive(Debug, Default, Deserialize)]
struct RawUser {
    #[serde(default)]
    username: String,
}

#[derive(Debug, Default, Deserialize)]
struct RawTag {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Default, Deserialize)]
struct RawStats {
    #[serde(default)]
    downloads_total: u64,
}

#[derive(Debug, Deserialize)]
struct RawFile {
    id: u64,
    #[serde(default)]
    filename: String,
    #[serde(default)]
    filesize: u64,
    #[serde(default)]
    download: RawDownload,
}

#[derive(Debug, Default, Deserialize)]
struct RawDownload {
    #[serde(default)]
    binary_url: String,
}

/// 「最近有变化」的一个 mod：给「今天」页用，只要展示需要的那几项。
#[derive(Debug, Clone, Serialize)]
pub struct RecentMod {
    pub id: u64,
    pub name: String,
    pub summary: String,
    pub downloads: u64,
    /// 上架时刻（unix 秒）
    pub date_added: u64,
    /// 最近更新时刻（unix 秒）
    pub date_updated: u64,
}

/// 分页大小。mod.io 单页最多 100，取 20 够界面一屏，也少占流量。
pub const PAGE_SIZE: u32 = 20;

/// 客户端。`token` 是用户自己的个人访问令牌。
pub struct Modio {
    token: String,
    http: reqwest::Client,
    /// 下载用：不设总时长上限，否则大 mod 传到一半会被掐断。
    http_file: reqwest::Client,
}

impl Modio {
    pub fn new(token: impl Into<String>) -> Self {
        let ua = format!("RockSprout/{}", env!("CARGO_PKG_VERSION"));
        let http = reqwest::Client::builder()
            .user_agent(ua.clone())
            // 走本地加速工具时单次请求可能要二三十秒，给足余量
            .timeout(Duration::from_secs(90))
            .build()
            .unwrap_or_default();
        let http_file = reqwest::Client::builder()
            .user_agent(ua)
            .connect_timeout(Duration::from_secs(20))
            .build()
            .unwrap_or_default();
        Self {
            token: token.into(),
            http,
            http_file,
        }
    }

    /// 从令牌文件读令牌建客户端。文件不存在就当作「没登录」。
    pub fn from_token_file(path: &Path) -> Result<Self, ModioError> {
        let raw = std::fs::read_to_string(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ModioError::NoToken
            } else {
                ModioError::Io(e.to_string())
            }
        })?;
        let token = raw.trim().to_owned();
        if token.is_empty() {
            return Err(ModioError::NoToken);
        }
        Ok(Self::new(token))
    }

    /// 查一次 `/me`，确认令牌有效并拿到自己的 uid。
    pub async fn me(&self) -> Result<Me, ModioError> {
        self.get_json::<Me>("/me").await
    }

    /// 搜索。`query` 为空时按下载量列出热门 mod。
    pub async fn search(&self, query: &str, page: u32) -> Result<SearchPage, ModioError> {
        self.list(Some(query), &[], None, "downloads", page).await
    }

    /// 带筛选的列表。关键字、类型标签、游戏版本、排序四样可以任意组合。
    ///
    /// 三处容易写错、接口却**不报错只当没写**的地方，这里都写死了：
    ///
    /// - 搜索参数是 `_q`，写成 `q=` 会被静默忽略；
    /// - 筛选参数是 `tags`（复数），写成 `tag=` 同样静默忽略；
    /// - 排序参数是 `_sort`，且值必须是真实字段名（`-downloads_total`），
    ///   `-downloads` 不存在，会被静默忽略。
    ///
    /// 多个标签之间是「同时具备」（实测 `tags=1.40,Cosmetic` 的条数是两者的交集，
    /// 不是并集），所以「版本 + 类型」能一次请求问完。
    pub async fn list(
        &self,
        keyword: Option<&str>,
        tags: &[String],
        version: Option<&str>,
        sort: &str,
        page: u32,
    ) -> Result<SearchPage, ModioError> {
        let page = page.max(1);
        let offset = u64::from(page - 1) * u64::from(PAGE_SIZE);
        let mut url = format!("{API_ROOT}/games/{GAME_ID}/mods?_limit={PAGE_SIZE}&_offset={offset}");
        if let Some(param) = sort_param(sort) {
            url.push_str("&_sort=");
            url.push_str(param);
        }
        let keyword = keyword.unwrap_or("").trim();
        if !keyword.is_empty() {
            url.push_str("&_q=");
            url.push_str(&urlencode(keyword));
        }

        let mut wanted: Vec<String> = Vec::new();
        if let Some(v) = version.map(str::trim).filter(|v| !v.is_empty()) {
            wanted.push(v.to_owned());
        }
        wanted.extend(
            tags.iter()
                .map(|t| t.trim().to_owned())
                .filter(|t| !t.is_empty()),
        );
        if !wanted.is_empty() {
            url.push_str("&tags=");
            url.push_str(&urlencode(&wanted.join(",")));
        }

        let list: ListEnvelope<RawMod> = self.get_json(&url).await?;
        let total = if list.result_total > 0 {
            list.result_total
        } else {
            // 老缓存/夹具里没有这个字段时，退回用这一页的条数，至少不会显示成 0
            list.result_count
        };
        let items: Vec<ModSummary> = list.data.into_iter().map(ModSummary::from).collect();
        Ok(SearchPage {
            query: keyword.to_owned(),
            page,
            total: total as u32,
            items,
            stale: false,
            cached_at: None,
        })
    }

    /// 按指定排序取前几个 mod。
    ///
    /// 参数是 mod.io 那套**带下划线**的写法（`_sort=-date_added`），不是 `sort`。
    /// 「今天」页拿它取「刚上架」与「刚更新」两批，再自己混排。
    ///
    /// 注意：这个接口对**不认识的排序字段不报错**，是静默退回默认排序（实测
    /// `_sort=-bogus_field` 仍回 200）。所以判断参数对不对要看结果的顺序，别只看状态码。
    pub async fn recent(&self, sort: &str, limit: u32) -> Result<Vec<RecentMod>, ModioError> {
        let limit = limit.clamp(1, 100);
        let url = format!(
            "{API_ROOT}/games/{GAME_ID}/mods?_limit={limit}&_sort={}",
            urlencode(sort)
        );
        let list: ListEnvelope<RawMod> = self.get_json(&url).await?;
        Ok(list
            .data
            .into_iter()
            .map(|m| RecentMod {
                id: m.id,
                name: m.name,
                summary: m.summary,
                downloads: m.stats.downloads_total,
                date_added: m.date_added,
                date_updated: m.date_updated,
            })
            .collect())
    }

    /// 详情。顺带把文件列表拿出来，算出「最新版本」显示给用户。
    pub async fn detail(&self, mod_id: u64) -> Result<ModDetail, ModioError> {
        let url = format!("{API_ROOT}/games/{GAME_ID}/mods/{mod_id}");
        let raw: RawMod = self.get_json(&url).await?;
        let files = self.files(mod_id).await?;
        let tags = pick_type_tags(&raw.tags);
        Ok(ModDetail {
            id: raw.id,
            name: raw.name,
            summary: raw.summary,
            description: raw.description,
            description_plaintext: raw.description_plaintext,
            downloads: raw.stats.downloads_total,
            version: match files.first() {
                Some(f) => f.filename.clone(),
                None => "无可用文件".to_owned(),
            },
            logo_320: raw.logo.thumb_320,
            logo_640: raw.logo.thumb_640,
            creator: raw.submitted_by.username,
            date_updated: raw.date_updated,
            tags,
            stale: false,
            cached_at: None,
        })
    }

    /// 文件列表（新的在前）。
    pub async fn files(&self, mod_id: u64) -> Result<Vec<ModFile>, ModioError> {
        let url = format!("{API_ROOT}/games/{GAME_ID}/mods/{mod_id}/files?_sort=-date_added");
        let list: ListEnvelope<RawFile> = self.get_json(&url).await?;
        Ok(list
            .data
            .into_iter()
            .map(|f| ModFile {
                id: f.id,
                filename: f.filename,
                size: f.filesize,
                url: f.download.binary_url,
            })
            .collect())
    }

    /// 最新一个文件（新的在前，取第一个）。
    ///
    /// 「今天」页拿它跟记账里的 `file_id` 比：id 不一样就是有新文件，也就是有新版。
    /// 一个 mod 一个请求，最多十个；取不到就交给调用方把那一块标成降级，不猜。
    pub async fn latest_file(&self, mod_id: u64) -> Result<Option<ModFile>, ModioError> {
        let url = format!(
            "{API_ROOT}/games/{GAME_ID}/mods/{mod_id}/files?_limit=1&_sort=-date_added"
        );
        let list: ListEnvelope<RawFile> = self.get_json(&url).await?;
        Ok(list.data.into_iter().next().map(|f| ModFile {
            id: f.id,
            filename: f.filename,
            size: f.filesize,
            url: f.download.binary_url,
        }))
    }

    /// 下载文件到 `dir`，返回落地后的路径。
    pub async fn download(&self, file: &ModFile, dir: &Path) -> Result<DownloadOutcome, ModioError> {
        if file.url.is_empty() {
            return Err(ModioError::Http {
                status: 200,
                body: "文件没有下载地址".to_owned(),
            });
        }
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|e| ModioError::Io(e.to_string()))?;

        let safe_name = if file.filename.trim().is_empty() {
            format!("mod-{}.zip", file.id)
        } else {
            sanitize_filename(&file.filename)
        };
        let dest = dir.join(&safe_name);

        let resp = self
            .http_file
            .get(&file.url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| ModioError::Network(describe(&e)))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(match status.as_u16() {
                401 | 403 => ModioError::Unauthorized,
                404 => ModioError::NotFound,
                other => ModioError::Http {
                    status: other,
                    body: clip(&body, 200),
                },
            });
        }
        let mut out = tokio::fs::File::create(&dest)
            .await
            .map_err(|e| ModioError::Io(e.to_string()))?;

        let mut resp = resp;
        let mut written: u64 = 0;
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| ModioError::Network(describe(&e)))?
        {
            tokio::io::AsyncWriteExt::write_all(&mut out, &chunk)
                .await
                .map_err(|e| ModioError::Io(e.to_string()))?;
            written += chunk.len() as u64;
        }
        tokio::io::AsyncWriteExt::flush(&mut out)
            .await
            .map_err(|e| ModioError::Io(e.to_string()))?;

        Ok(DownloadOutcome {
            file_id: file.id,
            filename: safe_name,
            size: written,
            path: dest.display().to_string(),
        })
    }

    // ---- 内部 ----

    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T, ModioError> {
        let full = if url.starts_with("http") {
            url.to_owned()
        } else {
            format!("{API_ROOT}{url}")
        };
        let resp = self.send(&full).await?;
        resp.json::<T>()
            .await
            .map_err(|e| ModioError::Http {
                status: 200,
                body: format!("返回内容不是预期的 JSON：{e}"),
            })
    }

    /// 发请求 + 处理状态码；429 时按 Retry-After 等一次再试。
    async fn send(&self, url: &str) -> Result<reqwest::Response, ModioError> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            let resp = self
                .http
                .get(url)
                .bearer_auth(&self.token)
                .header("Accept", "application/json")
                .send()
                .await
                .map_err(|e| ModioError::Network(describe(&e)))?;

            let status = resp.status();
            if status.is_success() {
                return Ok(resp);
            }

            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok());

            if status.as_u16() == 429 {
                if attempt == 1 {
                    let wait = retry_after.unwrap_or(5).min(60);
                    tracing::warn!(wait, "被 mod.io 限速，等一会儿重试");
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                    continue;
                }
                return Err(ModioError::RateLimited {
                    retry_after_secs: retry_after,
                });
            }

            let code = status.as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(match code {
                401 | 403 => ModioError::Unauthorized,
                404 => ModioError::NotFound,
                _ => ModioError::Http {
                    status: code,
                    body: clip(&body, 200),
                },
            });
        }
    }
}

impl From<RawMod> for ModSummary {
    fn from(m: RawMod) -> Self {
        Self {
            id: m.id,
            name: m.name,
            summary: m.summary,
            downloads: m.stats.downloads_total,
        }
    }
}

/// 从 mod 的全部标签里挑出 Type 组那几个（顺序按接口给的来）。
fn pick_type_tags(tags: &[RawTag]) -> Vec<String> {
    tags.iter()
        .map(|t| t.name.as_str())
        .filter(|n| TYPE_TAGS.contains(n))
        .map(str::to_owned)
        .collect()
}

/// 界面给的是人话，这里翻成 mod.io 的写法。返回 `None` 表示不带排序（按相关度）。
///
/// 排序值必须是**真实字段名**：写成 `-downloads` 会被静默忽略，照样回 200。
fn sort_param(sort: &str) -> Option<&'static str> {
    match sort {
        "latest" => Some("-date_added"),
        "downloads" => Some("-downloads_total"),
        _ => None,
    }
}

/// 把 reqwest 的错误连同底层原因一起说清楚（证书不受信任、DNS 失败、拒绝连接……）。
///
/// reqwest 自己只报一句「error sending request」，真正有用的原因在 source 链里。
pub(crate) fn describe(e: &reqwest::Error) -> String {
    use std::error::Error as _;
    let mut msg = e.to_string();
    let mut src = e.source();
    while let Some(s) = src {
        msg.push_str("：");
        msg.push_str(&s.to_string());
        src = s.source();
    }
    msg
}

/// 只保留前 n 个字符，避免把整页 HTML 塞进错误信息里。
fn clip(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}

/// URL 参数编码：只保留 ASCII 字母数字和 `-_.~`，其余（含中文）按 UTF-8 百分号编码。
pub(crate) fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 去掉 Windows 文件名里不允许的字符，防止下载文件名把路径带偏。
fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').to_owned();
    if trimmed.is_empty() {
        "download.bin".to_owned()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 只留类型组那九个标签() {
        let tags = vec![
            RawTag { name: "1.40".to_owned() },
            RawTag { name: "Gameplay".to_owned() },
            RawTag { name: "Verified".to_owned() },
            RawTag { name: "Optional".to_owned() },
            RawTag { name: "QoL".to_owned() },
            RawTag { name: "Weapon".to_owned() },
        ];
        assert_eq!(pick_type_tags(&tags), vec!["Gameplay", "QoL"]);
    }

    #[test]
    fn 没有类型标签时是空列表() {
        let tags = vec![RawTag { name: "1.40".to_owned() }];
        assert!(pick_type_tags(&tags).is_empty());
    }

    #[test]
    fn 排序值必须翻成真实字段名() {
        assert_eq!(sort_param("relevance"), None, "相关度就是不带 _sort");
        assert_eq!(sort_param("latest"), Some("-date_added"));
        // `-downloads` 不存在、会被静默忽略，所以必须是 downloads_total
        assert_eq!(sort_param("downloads"), Some("-downloads_total"));
        assert_eq!(sort_param("bogus"), None);
    }

    #[test]
    fn 筛选标签写错时给人话而不是甩json() {
        let err = ModioError::Http {
            status: 422,
            body: r#"{"error":{"code":422}}"#.to_owned(),
        };
        let msg = err.message();
        assert!(msg.contains("不认识的标签"), "实际：{msg}");
        assert!(!msg.contains("error_ref"), "不该把原始 JSON 甩给用户");
    }

    #[test]
    fn 中文关键词会被百分号编码() {
        assert_eq!(urlencode("钻石"), "%E9%92%BB%E7%9F%B3");
        assert_eq!(urlencode("a b"), "a%20b");
        assert_eq!(urlencode("clearer-laser_1.2"), "clearer-laser_1.2");
    }

    #[test]
    fn 下载文件名不会带出路径() {
        // 斜杠换成下划线后，开头的连续点也会被去掉，避免出现「..」或隐藏文件
        assert_eq!(sanitize_filename("../evil.pak"), "_evil.pak");
        assert_eq!(sanitize_filename("a:b*c?.pak"), "a_b_c_.pak");
        assert_eq!(sanitize_filename("   "), "download.bin");
    }

    #[test]
    fn 错误码和中文提示一一对应() {
        assert_eq!(ModioError::NoToken.code(), "no_token");
        assert!(ModioError::NoToken.message().contains("令牌"));
        assert_eq!(ModioError::Unauthorized.code(), "unauthorized");
        assert!(ModioError::Network("dns".into()).message().contains("连不上"));
    }

    #[test]
    fn 令牌文件不存在时算没登录() {
        // 用 .err() 而不是 unwrap_err()：Modio 故意不实现 Debug（避免把令牌打进日志）
        let err = Modio::from_token_file(Path::new("Z:/nope/modio-token.txt"))
            .err()
            .expect("应该报错");
        assert_eq!(err.code(), "no_token");
    }
}
