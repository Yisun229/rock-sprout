//! 日志：同时写到 `<数据目录>/logs/rock-sprout.log`（按天切分）和终端。
//!
//! 正式版没有控制台窗口，用户遇到问题时让他把日志文件发过来就行。
//!
//! 写进去之前会过一道「打码」：令牌那样的长串一律换成 `<已隐藏>`。
//! 日志是给人看、给别人发的，宁可多打一行码，也不能把令牌带出去。

use std::io::Write;
use std::sync::OnceLock;

use tracing_appender::non_blocking::WorkerGuard;

static GUARD: OnceLock<WorkerGuard> = OnceLock::new();

pub fn init() {
    let dir = crate::paths::data_root().join("logs");
    let _ = std::fs::create_dir_all(&dir);

    let appender = tracing_appender::rolling::daily(&dir, "rock-sprout.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    // guard 必须活到进程结束，否则日志会被丢掉
    let _ = GUARD.set(guard);

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let sink = writer.clone();
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(move || Redacting {
            inner: sink.clone(),
            pending: Vec::new(),
        })
        .with_ansi(false)
        .try_init();

    tracing::info!(dir = %dir.display(), "日志已启动");
}

/// 把令牌那样的长串打码。
///
/// 认两种：`Bearer xxx` 后面那一串，以及任何长度到 32 的字母数字串
/// （mod.io 的令牌是 32 位十六进制，别的服务的令牌只会更长）。
pub fn redact(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // Bearer 后面那一串：不管多长都打码
        if starts_with_ignore_case(&chars, i, "bearer ") {
            out.push_str("Bearer <已隐藏>");
            i += "bearer ".len();
            while i < chars.len() && !chars[i].is_whitespace() && !matches!(chars[i], ',' | '"' | ')') {
                i += 1;
            }
            continue;
        }

        if is_token_char(chars[i]) {
            let start = i;
            while i < chars.len() && is_token_char(chars[i]) {
                i += 1;
            }
            if i - start >= 32 {
                out.push_str("<已隐藏>");
            } else {
                out.extend(&chars[start..i]);
            }
            continue;
        }

        out.push(chars[i]);
        i += 1;
    }
    out
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '+'
}

fn starts_with_ignore_case(chars: &[char], at: usize, word: &str) -> bool {
    let w: Vec<char> = word.chars().collect();
    if at + w.len() > chars.len() {
        return false;
    }
    for (k, want) in w.iter().enumerate() {
        if !chars[at + k].eq_ignore_ascii_case(want) {
            return false;
        }
    }
    true
}

/// 一个「按行打码」的写出器：攒够一行再打码，避免把半行拆坏。
struct Redacting<W: Write> {
    inner: W,
    pending: Vec<u8>,
}

impl<W: Write> Write for Redacting<W> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.pending.extend_from_slice(data);
        while let Some(pos) = self.pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=pos).collect();
            let text = String::from_utf8_lossy(&line[..line.len() - 1]);
            self.inner.write_all(redact(&text).as_bytes())?;
            self.inner.write_all(b"\n")?;
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if !self.pending.is_empty() {
            let text = String::from_utf8_lossy(&self.pending).to_string();
            self.pending.clear();
            self.inner.write_all(redact(&text).as_bytes())?;
        }
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 三十二位的令牌会被打码() {
        let line = "令牌已保存 token=0123456789abcdef0123456789abcdef uid=42";
        let out = redact(line);
        assert!(!out.contains("0123456789abcdef"), "{out}");
        assert!(out.contains("<已隐藏>"), "{out}");
        assert!(out.contains("uid=42"), "别的字不能跟着没：{out}");
    }

    #[test]
    fn bearer_后面那一串不论长短都打码() {
        let out = redact("Authorization: Bearer abc123def");
        assert!(!out.contains("abc123def"), "{out}");
        assert!(out.contains("Bearer <已隐藏>"), "{out}");
    }

    #[test]
    fn 普通短词和中文不受影响() {
        let line = "开始安装 Mod Hub（id=1792770），目标 fsd/content/foo.pak";
        assert_eq!(redact(line), line);
    }

    #[test]
    fn 打码按行做不会把内容弄丢() {
        let mut sink: Vec<u8> = Vec::new();
        {
            let mut w = Redacting {
                inner: &mut sink,
                pending: Vec::new(),
            };
            w.write_all("第一行 token=0123456789abcdef0123456789abcdef\n第二行 ok\n".as_bytes())
                .expect("写得进去");
            w.flush().expect("刷得出去");
        }
        let text = String::from_utf8(sink).expect("是文字");
        assert_eq!(text.lines().count(), 2, "{text}");
        assert!(text.contains("第二行 ok"), "{text}");
        assert!(!text.contains("0123456789abcdef"), "{text}");
    }
}
