//! 令牌的存取：封好了才落盘，读的时候解出来。
//!
//! 磁盘上只有一个文件 `<数据目录>\token.sealed`，内容是一行 `dpapi1:...`。
//! v0.1.5 及以前是明文 `<数据目录>\modio-token.txt`；老用户第一次读到时会被
//! 自动封一份新的、然后把老文件删掉，不留两份（一份明文等于没封）。
//!
//! 解不开（换电脑、换账号、文件被改坏）一律当作「没有令牌」，
//! 让用户重新粘一次，而不是弹一个他看不懂的错误。

use std::path::{Path, PathBuf};

use crate::modio::{Modio, ModioError};
use crate::secret;

/// 封好的令牌文件。
pub fn sealed_path(data_root: &Path) -> PathBuf {
    data_root.join("token.sealed")
}

/// 老的明文令牌文件（只读不改，读到就迁移）。
pub fn legacy_path(data_root: &Path) -> PathBuf {
    data_root.join("modio-token.txt")
}

/// 读出令牌。没有、或解不开，都返回 None。
pub fn load(data_root: &Path) -> Option<String> {
    if let Ok(text) = std::fs::read_to_string(sealed_path(data_root)) {
        if let Ok(plain) = secret::open(&text) {
            let plain = plain.trim().to_owned();
            if !plain.is_empty() {
                return Some(plain);
            }
        }
    }

    // 老文件：读到就顺手迁移过去（迁移失败也不影响这次使用）
    let legacy = legacy_path(data_root);
    let text = std::fs::read_to_string(&legacy).ok()?;
    let plain = text.trim().to_owned();
    if plain.is_empty() {
        return None;
    }
    if save(data_root, &plain).is_ok() {
        tracing::info!("老令牌已改成加密存放，明文文件已删掉");
    }
    Some(plain)
}

/// 本机有没有能用的令牌（解不开也算没有）。
pub fn has_token(data_root: &Path) -> bool {
    load(data_root).is_some()
}

/// 按本机令牌建 mod.io 客户端；没有令牌就返回「没登录」。
pub fn client(data_root: &Path) -> Result<Modio, ModioError> {
    match load(data_root) {
        Some(token) => Ok(Modio::new(token)),
        None => Err(ModioError::NoToken),
    }
}

/// 保存令牌：能封就封，写不进封过的文件时返回错误（不偷偷写明文）。
pub fn save(data_root: &Path, token: &str) -> Result<(), String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("令牌是空的".to_owned());
    }
    let sealed = secret::seal(token)?;

    if let Err(e) = std::fs::create_dir_all(data_root) {
        return Err(format!("建目录失败：{e}"));
    }
    let path = sealed_path(data_root);
    let tmp = path.with_extension("sealed.tmp");
    std::fs::write(&tmp, sealed.as_bytes()).map_err(|e| format!("写文件失败：{e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("替换文件失败：{e}"))?;

    // 明文留着就等于没加密
    match std::fs::remove_file(legacy_path(data_root)) {
        Ok(()) => tracing::info!("已删掉老的明文令牌文件"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!(error = %e, "老的明文令牌文件删不掉"),
    }
    Ok(())
}

/// 忘掉令牌：两个文件都删，返回原本是否有令牌。
pub fn clear(data_root: &Path) -> Result<bool, String> {
    let mut had = false;
    for path in [sealed_path(data_root), legacy_path(data_root)] {
        match std::fs::remove_file(&path) {
            Ok(()) => had = true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("删文件失败：{e}")),
        }
    }
    Ok(had)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rock-sprout-token-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建临时目录");
        dir
    }

    #[test]
    fn 存了能读回来且磁盘上没有明文() {
        let root = temp_root("roundtrip");
        save(&root, "0123456789abcdef0123456789abcdef").expect("存得进去");

        let text = std::fs::read_to_string(sealed_path(&root)).expect("文件在");
        assert!(text.starts_with("dpapi1:"), "要封过：{text}");
        assert!(
            !text.contains("0123456789abcdef"),
            "磁盘上不能有明文：{text}"
        );
        assert_eq!(
            load(&root).as_deref(),
            Some("0123456789abcdef0123456789abcdef")
        );
    }

    #[test]
    fn 老明文文件读到就迁移走() {
        let root = temp_root("legacy");
        std::fs::write(legacy_path(&root), "old-plain-token\n").expect("写老文件");

        assert_eq!(load(&root).as_deref(), Some("old-plain-token"));
        assert!(!legacy_path(&root).exists(), "迁移完明文要删掉");
        assert!(sealed_path(&root).exists(), "要有封好的文件");
        assert_eq!(load(&root).as_deref(), Some("old-plain-token"));
    }

    #[test]
    fn 封过的文件坏了就当没有令牌() {
        let root = temp_root("broken");
        std::fs::write(sealed_path(&root), "dpapi1:ffff").expect("写坏文件");
        assert!(load(&root).is_none());
        assert!(!has_token(&root));
    }

    #[test]
    fn 清除把两个文件都删掉() {
        let root = temp_root("clear");
        save(&root, "abc").expect("存得进去");
        std::fs::write(legacy_path(&root), "leftover").expect("写老文件");

        assert_eq!(clear(&root), Ok(true));
        assert!(!sealed_path(&root).exists());
        assert!(!legacy_path(&root).exists());
        assert_eq!(clear(&root), Ok(false), "再清一次就是本来没有");
    }

    #[test]
    fn 空令牌不给存() {
        let root = temp_root("empty");
        assert!(save(&root, "  ").is_err());
        assert!(!sealed_path(&root).exists());
    }
}
