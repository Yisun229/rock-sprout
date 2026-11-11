//! Steam Web API key 的存放：和 mod.io 令牌同一套做法，先用 DPAPI 包一层再落盘。
//!
//! 磁盘上只有一个文件 `<数据目录>\steam-key.sealed`，内容是一行 `dpapi1:...`。
//! **不写进 `settings.json`**——那是明文 JSON，谁都能看，而 key 能替人读好友列表。
//!
//! 解不开（换电脑、换账号、文件被改坏）一律当作「没配」，让用户重新粘一次。

use std::path::{Path, PathBuf};

use crate::secret;

/// 封好的 key 文件。
pub fn sealed_path(data_root: &Path) -> PathBuf {
    data_root.join("steam-key.sealed")
}

/// 只认 32 位十六进制（Steam 发的就是这个形状）。
///
/// 不在这里校验「这把 key 有没有效」——那要打一次接口，设置页明确不做校验请求。
pub fn normalize(raw: &str) -> Option<String> {
    let text = raw.trim();
    if text.len() == 32 && text.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(text.to_ascii_uppercase())
    } else {
        None
    }
}

/// 读出 key。没有、或解不开，都返回 None。
pub fn load(data_root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(sealed_path(data_root)).ok()?;
    let plain = secret::open(&text).ok()?;
    normalize(&plain)
}

/// 本机有没有能用的 key（解不开也算没有）。
pub fn has(data_root: &Path) -> bool {
    load(data_root).is_some()
}

/// 保存 key：能封就封，写不进封过的文件时返回错误（不偷偷写明文）。
pub fn save(data_root: &Path, key: &str) -> Result<(), String> {
    let Some(key) = normalize(key) else {
        return Err("Steam key 是 32 位十六进制，粘的时候别带空格".to_owned());
    };
    let sealed = secret::seal(&key)?;
    std::fs::create_dir_all(data_root).map_err(|e| format!("建目录失败：{e}"))?;

    let path = sealed_path(data_root);
    let tmp = path.with_extension("sealed.tmp");
    std::fs::write(&tmp, sealed.as_bytes()).map_err(|e| format!("写文件失败：{e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("替换文件失败：{e}"))?;
    Ok(())
}

/// 清掉 key。
pub fn clear(data_root: &Path) {
    match std::fs::remove_file(sealed_path(data_root)) {
        Ok(()) => tracing::info!("Steam key 已删除"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!("删 Steam key 失败：{e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 只收_32_位十六进制() {
        assert_eq!(normalize(" 0123456789abcdef0123456789abcdef "), Some("0123456789ABCDEF0123456789ABCDEF".to_owned()));
        assert_eq!(normalize("0123456789abcdef0123456789abcde"), None);
        assert_eq!(normalize("0123456789abcdef0123456789abcdeg"), None);
        assert_eq!(normalize(""), None);
    }

    #[test]
    fn 封起来再解开还是原来那串() {
        let dir = std::env::temp_dir().join("rs-steamkey-test");
        let _ = std::fs::remove_dir_all(&dir);
        save(&dir, "0123456789abcdef0123456789abcdef").unwrap();
        assert!(has(&dir));
        assert_eq!(load(&dir).as_deref(), Some("0123456789ABCDEF0123456789ABCDEF"));
        // 磁盘上不能出现明文
        let raw = std::fs::read_to_string(sealed_path(&dir)).unwrap();
        assert!(!raw.contains("0123456789ABCDEF0123456789ABCDEF"));
        clear(&dir);
        assert!(!has(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
