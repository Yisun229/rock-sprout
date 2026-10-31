//! 官方的 UE4SS 运行时：从哪拿、怎么核对、解到哪儿。
//!
//! UE4SS 是「脚本类 mod」跑起来的前提（pak 类 mod 用不上它）。这里只管**把包备齐**：
//! 版本号、下载地址、大小、sha256 全部写死在代码里，不从网络问「最新是哪个」——
//! 拿回来的东西必须跟我们核对过的那一份逐字节相同，否则宁可让用户手动下载。
//!
//! 装进游戏目录是另一回事（那是 M7-2 的事），本模块一个字都不写游戏目录。
//!
//! 目录约定：
//! - 下载下来的压缩包：`<数据目录>\downloads\UE4SS_v3.0.1.zip`
//! - 解出来的四样东西：`<数据目录>\ue4ss\v3.0.1\`（dwmapi.dll / UE4SS.dll / UE4SS-settings.ini / Mods\）

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// 钉死的版本号（不带 v，给界面显示用）。
pub const VERSION: &str = "3.0.1";
/// 压缩包文件名。
pub const ZIP_NAME: &str = "UE4SS_v3.0.1.zip";
/// 官方发布页上的下载地址（固定 tag，永远指向这一份）。
pub const DOWNLOAD_URL: &str =
    "https://github.com/UE4SS-RE/RE-UE4SS/releases/download/v3.0.1/UE4SS_v3.0.1.zip";
/// 这一份压缩包的 sha256（2026-10-31 从上面地址下载后算的，两份独立下载结果一致）。
pub const ZIP_SHA256: &str = "4b47d4bceddd2f561a4e395bfa00924ccfc945af576a2d0c613e6537846c57ec";
/// 压缩包字节数。
pub const ZIP_SIZE: u64 = 5_523_402;
/// 解出来的 `UE4SS.dll` 的 sha256（用来确认解包没出错、文件没被换掉）。
pub const UE4SS_DLL_SHA256: &str = "8ac18fbffc1ef96b0662d4a2d537b3f224c26d65caaba7989a9404c566102b26";
/// 解出来的 `dwmapi.dll` 的 sha256。
pub const DWMAPI_SHA256: &str = "ce596412befa68c30b7f88f65beb77d9bdad55e9b96a276a5a9cf690c63f24bb";

/// 包里必须有的几样东西（少一样就不算备齐）。
pub const REQUIRED_FILES: [&str; 3] = ["dwmapi.dll", "UE4SS.dll", "UE4SS-settings.ini"];

/// 出岔子时给界面看的东西：一个稳定的 code + 一句中文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ue4ssError {
    pub code: &'static str,
    pub message: String,
}

impl Ue4ssError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// 压缩包放哪儿：`<数据目录>\downloads\UE4SS_v3.0.1.zip`。
pub fn zip_path(root: &Path) -> PathBuf {
    root.join("downloads").join(ZIP_NAME)
}

/// 解出来的东西放哪儿：`<数据目录>\ue4ss\v3.0.1\`。
pub fn package_dir(root: &Path) -> PathBuf {
    root.join("ue4ss").join(format!("v{VERSION}"))
}

/// 手动下载时该把 zip 放到哪个目录（给界面显示）。
pub fn manual_hint(root: &Path) -> String {
    root.join("downloads").display().to_string()
}

/// 算文件的 sha256，返回小写十六进制；读不了就给 `None`。
pub fn sha256_file(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// 压缩包是不是那一份：先看大小（便宜），再算 sha256（贵但只算一次）。
pub fn verify_zip(path: &Path) -> Result<(), Ue4ssError> {
    let meta = std::fs::metadata(path)
        .map_err(|e| Ue4ssError::new("zip_missing", format!("读不到压缩包：{e}")))?;
    if meta.len() != ZIP_SIZE {
        return Err(Ue4ssError::new(
            "zip_size",
            format!(
                "压缩包大小不对：期望 {ZIP_SIZE} 字节，实际 {} 字节。可能没下载完整，删了重下。",
                meta.len()
            ),
        ));
    }
    let got = sha256_file(path)
        .ok_or_else(|| Ue4ssError::new("zip_read", "压缩包读不了，可能被别的程序占着"))?;
    if !got.eq_ignore_ascii_case(ZIP_SHA256) {
        return Err(Ue4ssError::new(
            "zip_sha",
            format!("压缩包内容不对（sha256 {got}）。不是我们核对过的那一份，删了重下。"),
        ));
    }
    Ok(())
}

/// 解出来的目录是不是齐了：四样东西都在，两个 dll 内容对得上。
///
/// 齐了返回 `Ok(())`；不齐返回**缺什么**（给界面说人话用）。
pub fn verify_package(dir: &Path) -> Result<(), Ue4ssError> {
    for name in REQUIRED_FILES {
        if !dir.join(name).is_file() {
            return Err(Ue4ssError::new(
                "pkg_missing",
                format!("解出来的包里少了 {name}，重新解一次。"),
            ));
        }
    }
    if !dir.join("Mods").is_dir() {
        return Err(Ue4ssError::new(
            "pkg_missing",
            "解出来的包里少了 Mods 文件夹，重新解一次。".to_owned(),
        ));
    }
    for (name, want) in [("dwmapi.dll", DWMAPI_SHA256), ("UE4SS.dll", UE4SS_DLL_SHA256)] {
        let path = dir.join(name);
        let got = sha256_file(&path)
            .ok_or_else(|| Ue4ssError::new("pkg_read", format!("{name} 读不了")))?;
        if !got.eq_ignore_ascii_case(want) {
            return Err(Ue4ssError::new(
                "pkg_sha",
                format!("{name} 的内容跟核对过的不一样，别用这一份。"),
            ));
        }
    }
    Ok(())
}

/// 包是否已经备齐（界面一进来就要知道，所以做成纯读、同步）。
pub fn package_ready(root: &Path) -> bool {
    verify_package(&package_dir(root)).is_ok()
}

/// 把压缩包解到 `<数据目录>\ue4ss\v3.0.1\`。
///
/// 只解这四样：`dwmapi.dll`、`UE4SS.dll`、`UE4SS-settings.ini`、`Mods\` 整个目录。
/// 包里带的 README/Changelog 不解，省得白白多出几百 KB。
/// 先解到旁边的临时目录，验过再整体改名——中途失败不会留下半套。
pub fn extract(zip: &Path, dest: &Path) -> Result<PathBuf, Ue4ssError> {
    let staging = dest.with_extension("staging");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)
        .map_err(|e| Ue4ssError::new("extract_io", format!("建临时目录失败：{e}")))?;

    let file = std::fs::File::open(zip)
        .map_err(|e| Ue4ssError::new("zip_missing", format!("读不到压缩包：{e}")))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| Ue4ssError::new("zip_broken", format!("压缩包打不开：{e}")))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| Ue4ssError::new("zip_broken", format!("压缩包里的条目读不了：{e}")))?;
        let Some(name) = entry.enclosed_name() else {
            // 带 .. 或绝对路径的条目一律跳过（正常包里不会有，防一手）
            continue;
        };
        let name_str = name.to_string_lossy().replace('\\', "/");
        let keep = REQUIRED_FILES.iter().any(|f| name_str == *f)
            || name_str == "Mods"
            || name_str.starts_with("Mods/");
        if !keep {
            continue;
        }
        let out_path = staging.join(name);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)
                .map_err(|e| Ue4ssError::new("extract_io", format!("建目录失败：{e}")))?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Ue4ssError::new("extract_io", format!("建目录失败：{e}")))?;
        }
        let mut out = std::fs::File::create(&out_path)
            .map_err(|e| Ue4ssError::new("extract_io", format!("写文件失败：{e}")))?;
        std::io::copy(&mut entry, &mut out)
            .map_err(|e| Ue4ssError::new("extract_io", format!("解包写到一半失败：{e}")))?;
    }

    // 先验，再换名：这样外面永远不会看到一个「一半」的包
    verify_package(&staging)?;
    let _ = std::fs::remove_dir_all(dest);
    std::fs::rename(&staging, dest)
        .map_err(|e| Ue4ssError::new("extract_io", format!("换名失败：{e}")))?;
    Ok(dest.to_path_buf())
}

/// 下载进度（给界面轮询用）。
#[derive(Debug, Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub cancel: AtomicBool,
    pub running: AtomicBool,
}

impl Progress {
    pub fn done(&self) -> u64 {
        self.done.load(Ordering::Relaxed)
    }
    pub fn total(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }
    pub fn running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    fn reset(&self) {
        self.done.store(0, Ordering::Relaxed);
        self.total.store(ZIP_SIZE, Ordering::Relaxed);
        self.cancel.store(false, Ordering::SeqCst);
        self.running.store(true, Ordering::SeqCst);
    }
    fn finish(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

/// 下载并核对压缩包，落到 `<数据目录>\downloads\`。返回字节数。
///
/// 边下边写 `.part`，下完核对大小与 sha256 再改名成正式名字；
/// 中途取消或者失败都把 `.part` 删掉，不留半截文件。
pub async fn download(root: &Path, progress: &Progress) -> Result<u64, Ue4ssError> {
    let dir = root.join("downloads");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| Ue4ssError::new("io", format!("建下载目录失败：{e}")))?;
    let dest = zip_path(root);
    let part = dest.with_extension("zip.part");

    progress.reset();
    let result = stream_to(&part, progress).await;
    progress.finish();

    match result {
        Ok(written) => {
            if let Err(err) = verify_zip(&part) {
                let _ = tokio::fs::remove_file(&part).await;
                return Err(err);
            }
            tokio::fs::rename(&part, &dest)
                .await
                .map_err(|e| Ue4ssError::new("io", format!("改名失败：{e}")))?;
            let _ = written;
            Ok(ZIP_SIZE)
        }
        Err(err) => {
            let _ = tokio::fs::remove_file(&part).await;
            Err(err)
        }
    }
}

async fn stream_to(part: &Path, progress: &Progress) -> Result<u64, Ue4ssError> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("rock-sprout/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| Ue4ssError::new("network", format!("创建网络客户端失败：{e}")))?;

    let resp = client
        .get(DOWNLOAD_URL)
        .send()
        .await
        .map_err(|e| {
            // 这类错误一律说人话：用户多半是断网或者开着不合适的代理
            let _ = e;
            Ue4ssError::new(
                "network",
                "连不上下载地址。检查一下网络，或者照着下面的地址用浏览器下载。",
            )
        })?;
    if !resp.status().is_success() {
        return Err(Ue4ssError::new(
            "http",
            format!("下载地址返回了 HTTP {}，过一会儿再试。", resp.status().as_u16()),
        ));
    }

    let mut out = tokio::fs::File::create(part)
        .await
        .map_err(|e| Ue4ssError::new("io", format!("建文件失败：{e}")))?;
    let mut resp = resp;
    let mut written: u64 = 0;
    while let Some(chunk) = resp.chunk().await.map_err(|_| {
        Ue4ssError::new("network", "下载中途断了。再试一次，或者用浏览器下载。")
    })? {
        if progress.cancel.load(Ordering::SeqCst) {
            return Err(Ue4ssError::new("cancelled", "已取消下载。"));
        }
        tokio::io::AsyncWriteExt::write_all(&mut out, &chunk)
            .await
            .map_err(|e| Ue4ssError::new("io", format!("写文件失败：{e}")))?;
        written += chunk.len() as u64;
        progress.done.store(written, Ordering::Relaxed);
    }
    tokio::io::AsyncWriteExt::flush(&mut out)
        .await
        .map_err(|e| Ue4ssError::new("io", format!("写文件失败：{e}")))?;
    Ok(written)
}

/// 备齐的完整流程：有包就直接解，没包就下载。
pub async fn ensure_package(root: &Path, progress: &Progress) -> Result<PathBuf, Ue4ssError> {
    let zip = zip_path(root);
    if zip.is_file() {
        verify_zip(&zip)?;
    } else {
        download(root, progress).await?;
    }
    extract(&zip, &package_dir(root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 手动下载提示指向下载目录() {
        let root = PathBuf::from(r"D:\data");
        let hint = manual_hint(&root);
        assert!(hint.ends_with("downloads"), "{hint}");
        assert!(zip_path(&root).ends_with(ZIP_NAME));
    }

    #[test]
    fn 包目录带版本号() {
        let dir = package_dir(Path::new(r"D:\data"));
        assert!(dir.ends_with("v3.0.1"), "{}", dir.display());
    }

    #[test]
    fn 不存在的压缩包说清楚缺什么() {
        let err = verify_zip(Path::new(r"D:\nope\UE4SS_v3.0.1.zip")).unwrap_err();
        assert_eq!(err.code, "zip_missing");
        assert!(err.message.contains("压缩包"));
    }

    #[test]
    fn 大小不对的压缩包直接拒绝() {
        let dir = std::env::temp_dir().join("rock-sprout-ue4ss-size");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let zip = dir.join(ZIP_NAME);
        std::fs::write(&zip, b"not the real thing").unwrap();
        let err = verify_zip(&zip).unwrap_err();
        assert_eq!(err.code, "zip_size");
        assert!(err.message.contains("期望"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 目录不齐时指出缺哪一样() {
        let dir = std::env::temp_dir().join("rock-sprout-ue4ss-partial");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("dwmapi.dll"), b"x").unwrap();
        let err = verify_package(&dir).unwrap_err();
        assert_eq!(err.code, "pkg_missing");
        assert!(err.message.contains("UE4SS.dll"), "{}", err.message);
        assert!(!package_ready(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 下载地址与版本号对得上() {
        assert!(DOWNLOAD_URL.contains(&format!("v{VERSION}")));
        assert!(DOWNLOAD_URL.ends_with(ZIP_NAME));
        assert_eq!(ZIP_NAME, format!("UE4SS_v{VERSION}.zip"));
    }

    #[test]
    fn 取消标记能被看见() {
        let p = Progress::default();
        assert!(!p.running());
        p.request_cancel();
        assert!(p.cancel.load(Ordering::SeqCst));
    }
}
