//! 列出 pak 里都有什么（调试用）。
//!
//! 装 mod 时如果产出的大小和我们预期不一样，就靠它把两个包的内容对出来。

use std::io::{BufReader, Seek, SeekFrom};
use std::path::Path;

/// 每个条目一行：路径 + 原始大小 + 是否压缩。
pub fn list(path: &Path) -> Result<Vec<String>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("打开失败：{e}"))?;
    let mut reader = BufReader::new(file);
    let pak = repak::PakBuilder::new()
        .reader(&mut reader)
        .map_err(|e| format!("不是能识别的 pak：{e}"))?;

    let mut out = Vec::new();
    for name in pak.files() {
        // 读一次才能知道大小；条目多的包会慢，仅用于调试
        reader
            .seek(SeekFrom::Start(0))
            .map_err(|e| format!("回绕文件失败：{e}"))?;
        let size = pak
            .get(&name, &mut reader)
            .map(|data| data.len())
            .unwrap_or(0);
        out.push(format!("{size:>10}  {name}"));
    }
    Ok(out)
}
