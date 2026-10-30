//! Steam 商店公告：拿《深岩银河》官方补丁说明给「今天」页用。
//!
//! 走 Steam 商店的 RSS（`feeds/news/app/548430`），一个字没改的原始公告都在里面。
//! 不引入 XML 解析库：RSS 的结构几十年没变过，按 `<item>` 切块再取标签内容就够了，
//! 少一个依赖，也少一处升级时要跟着动的风险。
//!
//! 时间戳自己算：`pubDate` 是 RFC 2822（`Fri, 30 Oct 2026 18:20:00 +0000`），
//! 只为这一处解析去开 `time` 的 `parsing` 特性不划算，几十行写完更省事。

use std::time::Duration;

/// 《深岩银河》在 Steam 上的 appid。
pub const GAME_APPID: u32 = 548_430;

/// 一条公告。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NewsItem {
    /// Steam 给的公告编号，界面上拿来当 key
    pub gid: String,
    pub title: String,
    pub url: String,
    /// 发布时刻（unix 秒）；解析不出来就是 0
    pub date: u64,
    /// 纯文本正文，已去标签、已按上限截断
    pub contents: String,
}

fn feed_url(appid: u32) -> String {
    format!("https://store.steampowered.com/feeds/news/app/{appid}/?cc=CN&l=schinese&snr=100601")
}

/// 取最新几条公告。
///
/// 失败时把原因当字符串返回——调用方只记一笔日志、把这一块留空，不该因为公告拉不到
/// 就让整页报错。
pub async fn fetch_news(appid: u32, count: u32, maxlen: u32) -> Result<Vec<NewsItem>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("rock-sprout/0.1 (+https://github.com/Yisun229/rock-sprout)")
        .build()
        .map_err(|e| format!("建 HTTP 客户端失败：{e}"))?;
    let resp = client
        .get(feed_url(appid))
        .send()
        .await
        .map_err(|e| format!("连不上 Steam：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Steam 返回 {}", resp.status().as_u16()));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读公告正文失败：{e}"))?;
    Ok(parse_feed(&text, count, maxlen))
}

/// 从 RSS 正文里切出前 `count` 条。
pub fn parse_feed(xml: &str, count: u32, maxlen: u32) -> Vec<NewsItem> {
    let mut out = Vec::new();
    let mut rest = xml;
    while out.len() < count as usize {
        let Some(start) = rest.find("<item>") else {
            break;
        };
        let after = &rest[start + "<item>".len()..];
        let Some(end) = after.find("</item>") else {
            break;
        };
        let block = &after[..end];
        rest = &after[end + "</item>".len()..];

        let title = text_of(block, "title");
        let url = text_of(block, "link");
        // 没有链接的条目在界面上没法点，跳过
        if title.is_empty() || url.is_empty() {
            continue;
        }
        // Steam 的条目里没有 <gid>，用链接末尾那串编号当唯一键
        let mut gid = text_of(block, "gid");
        if gid.is_empty() {
            gid = url.rsplit('/').next().unwrap_or("").to_owned();
        }
        out.push(NewsItem {
            gid,
            title,
            url,
            date: parse_rfc2822(&text_of(block, "pubDate")).unwrap_or(0),
            contents: shorten(&plain_text(&text_of(block, "description")), maxlen),
        });
    }
    out
}

/// 取 `<tag>…</tag>` 里的文字，剥掉 CDATA 包装。
fn text_of(block: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let Some(start) = block.find(&open) else {
        return String::new();
    };
    let after = &block[start + open.len()..];
    let Some(end) = after.find(&close) else {
        return String::new();
    };
    let mut v = after[..end].trim().to_owned();
    if let Some(inner) = v.strip_prefix("<![CDATA[") {
        if let Some(inner) = inner.strip_suffix("]]>") {
            v = inner.trim().to_owned();
        }
    }
    v
}

/// RSS 的描述里带 HTML，而且**是转义过的**（Steam 实际回的是
/// `&lt;div class=&quot;bb_h1&quot;&gt;` 这种），所以顺序是：先还原实体 → 再削标签 →
/// 再还原一次文字里的实体（`&amp;nbsp;` 这种套了两层的）→ 空白压成一个空格。
///
/// 只在**块级标签**处补空格（`</p>`、`<br>`、`</li>` 这类），行内标签（`<b>`、`<a>`）
/// 直接删掉——不然「<b>若干</b>问题」会被拆成「若干 问题」。
pub fn plain_text(html: &str) -> String {
    let decoded = decode_entities(html);
    let stripped = strip_tags(&decoded);
    collapse_ws(&decode_entities(&stripped))
}

fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        let after = &rest[lt..];
        let Some(gt) = after.find('>') else {
            // 半个标签，剩下的当正文
            out.push_str(after);
            rest = "";
            break;
        };
        let tag = after[1..gt].trim();
        let name = tag
            .trim_start_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_end_matches('/')
            .to_ascii_lowercase();
        if matches!(
            name.as_str(),
            "br" | "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4"
        ) {
            out.push(' ');
        }
        rest = &after[gt + 1..];
    }
    out.push_str(rest);
    out
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space {
            pending_space = false;
            let prev = out.chars().last();
            // 中文里块级标签换行不该留空格：「你好，」+「我们」要连起来；
            // 英文两侧都不是宽字符，空格照留。
            let keep = match prev {
                Some(p) => !(is_wide(p) && is_wide(ch)),
                None => false,
            };
            if keep {
                out.push(' ');
            }
        }
        out.push(ch);
    }
    out
}

/// 粗略判断「宽字符」：汉字与中文标点。只用来决定标签处要不要留空格。
fn is_wide(c: char) -> bool {
    matches!(c as u32,
        0x3000..=0x303F | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xFF00..=0xFFEF)
}

/// 按字符截断（不会把一个汉字/emoji 劈成两半），超了补省略号。
pub fn shorten(text: &str, maxlen: u32) -> String {
    let max = maxlen as usize;
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// 解析 `Fri, 30 Oct 2026 18:20:00 +0000`。
///
/// 只认这种固定形状；认不出来返回 `None`，界面上就显示「时间未知」。
pub fn parse_rfc2822(raw: &str) -> Option<u64> {
    let s = raw.trim();
    // 去掉开头的星期几
    let s = match s.find(',') {
        Some(i) => s[i + 1..].trim(),
        None => s,
    };
    let mut it = s.split_whitespace();
    let day: i64 = it.next()?.parse().ok()?;
    let month = match it.next()? {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let year: i64 = it.next()?.parse().ok()?;
    let clock = it.next()?;
    let mut hm = clock.split(':');
    let hour: i64 = hm.next()?.parse().ok()?;
    let minute: i64 = hm.next()?.parse().ok()?;
    let second: i64 = hm.next().unwrap_or("0").parse().ok()?;
    let offset_secs = parse_offset(it.next().unwrap_or("+0000"));

    let days = days_from_civil(year, month, day);
    let secs = days * 86_400 + hour * 3_600 + minute * 60 + second - offset_secs;
    if secs < 0 {
        return None;
    }
    Some(secs as u64)
}

/// `+0800` / `-0530` → 相对 UTC 的秒数。
fn parse_offset(raw: &str) -> i64 {
    let b = raw.as_bytes();
    if b.len() != 5 {
        return 0;
    }
    let sign = if b[0] == b'-' { -1 } else { 1 };
    let digits = |s: &str| s.parse::<i64>().unwrap_or(0);
    let h = digits(&raw[1..3]);
    let m = digits(&raw[3..5]);
    sign * (h * 3_600 + m * 60)
}

/// 公历日期 → 1970-01-01 起的天数（Howard Hinnant 的算法，够用且不受时区影响）。
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel><title>548430 RSS 新闻</title>
<item>
  <title><![CDATA[Update 39 补丁说明]]></title>
  <link><![CDATA[https://store.steampowered.com/news/app/548430/view/716793092553837019]]></link>
  <description>&lt;div class=&quot;bb_h1&quot;&gt;&lt;b&gt;各位矿工&lt;/b&gt;你们好，&lt;/div&gt;&lt;p&gt;修复了若干问题 &amp; 调整平衡。&lt;/p&gt;</description>
  <pubDate>Fri, 30 Oct 2026 18:20:00 +0000</pubDate>
</item>
<item>
  <title>第二个</title>
  <link>https://store.steampowered.com/news/app/548430/view/2</link>
  <description>没有 CDATA 的描述</description>
  <pubDate>Thu, 29 Oct 2026 09:00:00 +0800</pubDate>
  <gid>2</gid>
</item>
<item>
  <title>没链接的条目</title>
  <description>界面上点不动，应该被跳过</description>
  <gid>3</gid>
</item>
</channel></rss>"#;

    #[test]
    fn 能切出带链接的公告并去掉标签() {
        let items = parse_feed(SAMPLE, 3, 300);
        assert_eq!(items.len(), 2, "没链接的那条要被跳过");
        assert_eq!(items[0].title, "Update 39 补丁说明");
        assert_eq!(items[0].contents, "各位矿工你们好，修复了若干问题 & 调整平衡。");
        assert_eq!(
            items[0].url,
            "https://store.steampowered.com/news/app/548430/view/716793092553837019"
        );
    }

    #[test]
    fn 没有gid时用链接末尾编号当键() {
        let items = parse_feed(SAMPLE, 2, 300);
        assert_eq!(items[0].gid, "716793092553837019");
        assert_eq!(items[1].gid, "2", "本来就给了 gid 就用它");
    }

    #[test]
    fn 最多只取要求的条数() {
        assert_eq!(parse_feed(SAMPLE, 1, 300).len(), 1);
    }

    #[test]
    fn 摘要按字符截断不劈开汉字() {
        let long = "岩".repeat(400);
        let cut = shorten(&long, 10);
        assert_eq!(cut.chars().count(), 10);
        assert!(cut.ends_with('…'));
        assert_eq!(shorten("短", 10), "短");
    }

    #[test]
    fn 时间戳按rfc2822算对() {
        // 2026-10-30 18:20:00 UTC
        assert_eq!(
            parse_rfc2822("Fri, 30 Oct 2026 18:20:00 +0000"),
            Some(1_793_384_400)
        );
        // 带 +0800 的同一时刻要早 8 小时
        assert_eq!(
            parse_rfc2822("Fri, 30 Oct 2026 18:20:00 +0800"),
            Some(1_793_384_400 - 8 * 3_600)
        );
    }

    #[test]
    fn 认不出的时间不瞎猜() {
        assert_eq!(parse_rfc2822(""), None);
        assert_eq!(parse_rfc2822("昨天"), None);
    }

    #[test]
    fn 中文里块级标签不留空格英文照留() {
        assert_eq!(
            plain_text("&lt;div&gt;各位矿工你们好，&lt;/div&gt;&lt;p&gt;我们刚刚宣布。&lt;/p&gt;"),
            "各位矿工你们好，我们刚刚宣布。"
        );
        assert_eq!(
            plain_text("Hello Miners,</div><p>Hoxxes is live"),
            "Hello Miners, Hoxxes is live"
        );
    }

    #[test]
    fn 整段公告的正文转纯文字时块级标签留空格行内标签不拆词() {
        assert_eq!(
            plain_text("<p>第一段</p><p>第二段<b>强调</b>收尾</p>"),
            "第一段第二段强调收尾"
        );
        assert_eq!(plain_text("a<br/>b"), "a b");
        assert_eq!(plain_text("没有标签"), "没有标签");
    }

    #[test]
    fn 公告地址带对appid() {
        assert!(feed_url(GAME_APPID).contains("/app/548430/"));
    }
}
