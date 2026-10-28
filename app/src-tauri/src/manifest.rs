//! 一个 mod 到底改了哪些资源，以及「装它会不会盖掉已经装的」。
//!
//! 清单按内容命名：`<数据目录>\cache\pakmanifest\<sha256>.json`。
//! 内容一样就是同一个文件，所以缓存永远不会过期，也不用写清理逻辑。

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::local;

/// 一个包里的资源清单（路径已归一化：去掉 `../../../`、反斜杠换正斜杠、全小写）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PakManifest {
    pub sha256: String,
    pub paths: BTreeSet<String>,
    pub warnings: Vec<String>,
}

/// 待装的来源：能是本地文件，也能是已经下载过的 mod.io 包
#[derive(Debug, Clone)]
pub struct Incoming {
    pub name: String,
    pub path: PathBuf,
}

/// 一处「你也要改、别人也改过」的资源
#[derive(Debug, Clone, Serialize)]
pub struct Conflict {
    pub path: String,
    pub incoming: Vec<String>,
    pub installed: Vec<String>,
}

/// 预检结果：装之前先问一句「会撞吗」
#[derive(Debug, Clone, Serialize)]
pub struct PrecheckReport {
    pub incoming: Vec<String>,
    pub installed: Vec<String>,
    pub conflicts: Vec<Conflict>,
    pub summary: String,
    pub warnings: Vec<String>,
    pub elapsed_ms: u128,
}

impl PrecheckReport {
    fn is_empty(&self) -> bool {
        self.conflicts.is_empty()
    }
}

/// 把 pak 里的路径归一化成能互相对上的样子
pub fn normalize(raw: &str) -> String {
    let without_prefix = raw
        .strip_prefix("../../../")
        .or_else(|| raw.strip_prefix("..\\..\\..\\"))
        .unwrap_or(raw);
    without_prefix.replace('\\', "/").to_ascii_lowercase()
}

/// 按 64 KB 一块边读边算，不把整个包读进内存（一个 mod 包上百兆很常见）。
fn sha256_of_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("打不开 {}：{e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let read = file
            .read(&mut buf)
            .map_err(|e| format!("读不了 {}：{e}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// 内容指纹：文件按字节算，文件夹按 `local::fingerprint` 的规则算
fn content_id(path: &Path) -> Result<String, String> {
    if path.is_dir() {
        local::fingerprint(path).map(|fp| fp.hash)
    } else {
        sha256_of_file(path)
    }
}

fn cache_file(cache_root: &Path, sha256: &str) -> PathBuf {
    cache_root.join("pakmanifest").join(format!("{sha256}.json"))
}

fn read_cache(path: &Path) -> Option<PakManifest> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_cache(path: &Path, manifest: &PakManifest) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string(manifest) {
        let _ = std::fs::write(path, text);
    }
}

/// 取清单：先看缓存，没有再真去解包
pub fn manifest_for(path: &Path, cache_root: &Path) -> Result<PakManifest, String> {
    if !path.exists() {
        return Err(format!("找不到 {}", path.display()));
    }

    let sha256 = content_id(path)?;
    let cached = cache_file(cache_root, &sha256);
    if let Some(hit) = read_cache(&cached) {
        return Ok(hit);
    }

    let mut paths = BTreeSet::new();
    let mut warnings = Vec::new();

    if path.is_dir() {
        collect_from_dir(path, &mut paths, &mut warnings)?;
    } else {
        let bytes = std::fs::read(path).map_err(|e| format!("读不了 {}：{e}", path.display()))?;
        collect_from_file_bytes(&bytes, &mut paths, &mut warnings);
    }

    if paths.is_empty() && warnings.is_empty() {
        warnings.push("这个包里没找到任何资源文件".to_owned());
    }

    let manifest = PakManifest {
        sha256,
        paths,
        warnings,
    };
    write_cache(&cached, &manifest);
    Ok(manifest)
}

fn collect_from_dir(
    dir: &Path,
    paths: &mut BTreeSet<String>,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let mut paks = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries =
            std::fs::read_dir(&current).map_err(|e| format!("读不了 {}：{e}", current.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("读不了 {}：{e}", current.display()))?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("pak"))
            {
                paks.push(path);
            }
        }
    }

    if paks.is_empty() {
        warnings.push(format!("{} 里没有 .pak 文件", dir.display()));
        return Ok(());
    }

    for pak in paks {
        let bytes = std::fs::read(&pak).map_err(|e| format!("读不了 {}：{e}", pak.display()))?;
        collect_from_pak_bytes(&bytes, &pak.display().to_string(), paths, warnings);
    }
    Ok(())
}

fn collect_from_file_bytes(bytes: &[u8], paths: &mut BTreeSet<String>, warnings: &mut Vec<String>) {
    // 可能是 zip（里面装 pak），也可能本身就是 pak
    if let Ok(mut archive) = zip::ZipArchive::new(Cursor::new(bytes)) {
        let mut found_pak = false;
        for i in 0..archive.len() {
            let mut file = match archive.by_index(i) {
                Ok(file) => file,
                Err(_) => continue,
            };
            if !file.is_file() {
                continue;
            }
            let name = file.name().to_owned();
            if !name.to_ascii_lowercase().ends_with(".pak") {
                continue;
            }
            found_pak = true;
            let mut inner = Vec::new();
            if file.read_to_end(&mut inner).is_err() {
                warnings.push(format!("{name} 解不出来，已跳过"));
                continue;
            }
            collect_from_pak_bytes(&inner, &name, paths, warnings);
        }
        if !found_pak {
            warnings.push("这个压缩包里没有 .pak 文件".to_owned());
        }
        return;
    }

    collect_from_pak_bytes(bytes, "包", paths, warnings);
}

fn collect_from_pak_bytes(
    bytes: &[u8],
    label: &str,
    paths: &mut BTreeSet<String>,
    warnings: &mut Vec<String>,
) {
    let mut cursor = Cursor::new(bytes);
    let pak = match repak::PakBuilder::new().reader(&mut cursor) {
        Ok(pak) => pak,
        Err(e) => {
            warnings.push(format!("{label} 打不开（{e}），已跳过"));
            return;
        }
    };
    let mount = pak.mount_point().to_owned();
    for file in pak.files() {
        let joined = if mount.is_empty() {
            file
        } else {
            format!("{mount}{file}")
        };
        paths.insert(normalize(&joined));
    }
}

/// 打包/合并时会特殊处理、不能算「打架」的路径。
///
/// - `FSD/AssetRegistry.bin`：每个带资源的 mod 都会带一份，合并时会把它们重新拼起来（见 `integrate.rs`）；
/// - `FSD/Content/_Interop`：mod 之间约定共用的中转目录。
fn is_shared_bookkeeping(path: &str) -> bool {
    path == "fsd/assetregistry.bin" || path.starts_with("fsd/content/_interop")
}

/// 算指纹时一次读多少字节。64 KB 是「不占内存、也不至于读太碎」的折中。
const CHUNK: usize = 64 * 1024;

/// 预检：这些待装的包，会不会盖掉已经装的
pub fn precheck(
    incoming: &[Incoming],
    installed: &[Incoming],
    cache_root: &Path,
) -> Result<PrecheckReport, String> {
    let started = std::time::Instant::now();
    let mut warnings = Vec::new();

    let mut incoming_paths: Vec<(String, BTreeSet<String>)> = Vec::new();
    for item in incoming {
        let manifest = manifest_for(&item.path, cache_root)?;
        warnings.extend(manifest.warnings);
        incoming_paths.push((item.name.clone(), manifest.paths));
    }

    let mut installed_paths: Vec<(String, BTreeSet<String>)> = Vec::new();
    for item in installed {
        if !item.path.exists() {
            warnings.push(format!("{} 的包已经不在本机，跳过对比", item.name));
            continue;
        }
        match manifest_for(&item.path, cache_root) {
            Ok(manifest) => {
                warnings.extend(manifest.warnings);
                installed_paths.push((item.name.clone(), manifest.paths));
            }
            Err(e) => warnings.push(e),
        }
    }

    // 每个路径：谁（待装）要改、谁（已装）已经改过
    let mut per_path: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)> = BTreeMap::new();
    for (name, paths) in &incoming_paths {
        for path in paths {
            per_path
                .entry(path.clone())
                .or_default()
                .0
                .insert(name.clone());
        }
    }
    for (name, paths) in &installed_paths {
        for path in paths {
            per_path
                .entry(path.clone())
                .or_default()
                .1
                .insert(name.clone());
        }
    }

    let mut conflicts = Vec::new();
    let mut skipped_shared = 0usize;
    for (path, (incoming_names, installed_names)) in per_path {
        if is_shared_bookkeeping(&path) {
            skipped_shared += 1;
            continue;
        }
        // 只有「待装」和「已装」都碰过的才算打架；待装之间互相重叠也单独算一类
        let touches_installed = !installed_names.is_empty() && !incoming_names.is_empty();
        let incoming_overlap = incoming_names.len() > 1;
        if touches_installed || incoming_overlap {
            conflicts.push(Conflict {
                path,
                incoming: incoming_names.into_iter().collect(),
                installed: installed_names.into_iter().collect(),
            });
        }
    }
    if skipped_shared > 0 {
        warnings.push(format!(
            "{skipped_shared} 处是每个 mod 都会带的公共文件（AssetRegistry.bin / _Interop），合并时会重新拼，不算撞"
        ));
    }

    let mut report = PrecheckReport {
        incoming: incoming_paths.iter().map(|(name, _)| name.clone()).collect(),
        installed: installed_paths.iter().map(|(name, _)| name.clone()).collect(),
        conflicts,
        summary: String::new(),
        warnings,
        elapsed_ms: 0,
    };
    report.summary = summarize(&report);
    report.elapsed_ms = started.elapsed().as_millis();
    Ok(report)
}

fn summarize(report: &PrecheckReport) -> String {
    if report.is_empty() {
        return "跟已经装的不撞：装上去不会盖掉任何东西。".to_owned();
    }

    // 每个待装 mod：它会盖掉哪些已装 mod 的各多少处资源
    let mut per_incoming: BTreeMap<&str, BTreeMap<&str, usize>> = BTreeMap::new();
    for conflict in &report.conflicts {
        for incoming in &conflict.incoming {
            for installed in &conflict.installed {
                *per_incoming
                    .entry(incoming.as_str())
                    .or_default()
                    .entry(installed.as_str())
                    .or_default() += 1;
            }
        }
    }

    let mut parts = Vec::new();
    for (incoming, victims) in per_incoming {
        let described = victims
            .iter()
            .map(|(name, count)| format!("{name}（{count} 处）"))
            .collect::<Vec<_>>()
            .join("、");
        parts.push(format!(
            "装 {incoming} 会盖掉 {described}，两处都改过同一批文件时只有排在后面的会生效"
        ));
    }

    if parts.is_empty() {
        let names = report
            .conflicts
            .first()
            .map(|c| c.incoming.join("、"))
            .unwrap_or_default();
        return format!("{names} 这几个之间自己就撞（改的是同一批文件）");
    }

    parts.join("；")
}

/// 已经装着的这些 mod 之间，谁盖着谁
#[derive(Debug, Clone, Serialize)]
pub struct ScanMod {
    pub id: u64,
    pub name: String,
    /// 有几个文件被别人盖住了
    pub overridden: usize,
    /// 有几个文件盖住了别人
    pub covers: usize,
    /// 盖住它的那几个 mod 的名字（去重）
    pub by: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanReport {
    pub mods: Vec<ScanMod>,
    /// 有争议的文件总数
    pub total_paths: usize,
    pub warnings: Vec<String>,
    pub elapsed_ms: u64,
}

/// 扫一遍已经装的 mod：按合并顺序（先装的在前），同一个文件后面的盖前面的。
pub fn scan(installed: &[(u64, String, PathBuf)], cache_root: &Path) -> Result<ScanReport, String> {
    let started = std::time::Instant::now();
    let mut warnings = Vec::new();
    let mut order: Vec<(u64, String)> = Vec::new();
    // 文件路径 → 改过它的 mod 下标（按合并顺序）
    let mut owners: BTreeMap<String, Vec<usize>> = BTreeMap::new();

    for (index, (id, name, path)) in installed.iter().enumerate() {
        order.push((*id, name.clone()));
        if !path.exists() {
            warnings.push(format!("{name}：原文件已经不在本机，这一条跳过了"));
            continue;
        }
        match manifest_for(path, cache_root) {
            Ok(manifest) => {
                warnings.extend(manifest.warnings.iter().cloned());
                for p in &manifest.paths {
                    if is_shared_bookkeeping(p) {
                        continue;
                    }
                    owners.entry(p.clone()).or_default().push(index);
                }
            }
            Err(e) => warnings.push(format!("{name}：{e}")),
        }
    }

    let mut mods = order
        .iter()
        .map(|(id, name)| ScanMod {
            id: *id,
            name: name.clone(),
            overridden: 0,
            covers: 0,
            by: Vec::new(),
        })
        .collect::<Vec<_>>();

    let mut total_paths = 0;
    for indices in owners.values() {
        if indices.len() < 2 {
            continue;
        }
        total_paths += 1;
        let winner = *indices.last().expect("至少有俩");
        mods[winner].covers += 1;
        for loser in &indices[..indices.len() - 1] {
            mods[*loser].overridden += 1;
            let name = order[winner].1.clone();
            if !mods[*loser].by.contains(&name) {
                mods[*loser].by.push(name);
            }
        }
    }

    Ok(ScanReport {
        mods,
        total_paths,
        warnings,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../_tools/fixtures/small-mod.zip")
    }

    fn temp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("rs-manifest-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("建测试目录");
        root
    }

    #[test]
    fn 路径会被整理成同一套写法() {
        assert_eq!(normalize("../../../FSD/Content/A.uasset"), "fsd/content/a.uasset");
        assert_eq!(normalize("..\\..\\..\\FSD\\Content\\B.uasset"), "fsd/content/b.uasset");
        assert_eq!(normalize("FSD/Content/C.uasset"), "fsd/content/c.uasset");
    }

    #[test]
    fn 能读出固定包里的资源清单() {
        let root = temp_root("read");
        let manifest = manifest_for(&fixture(), &root).expect("应该能读");
        assert!(!manifest.paths.is_empty(), "清单不该是空的");
        assert!(
            manifest.paths.iter().all(|p| !p.contains("..") && !p.contains('\\')),
            "路径都该归一化过：{:?}",
            manifest.paths
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn 扫已装的包能说清谁盖了谁() {
        let root = temp_root("scan");
        // 同一个包装了两遍（排在后面的那份生效），再加一个不撞的
        let list = vec![
            (1u64, "先装的".to_owned(), fixture()),
            (2u64, "后装的".to_owned(), fixture()),
        ];
        let report = scan(&list, &root).expect("能扫");
        assert_eq!(report.mods.len(), 2);
        assert!(report.total_paths > 0, "同一个包的两份应该撞上");
        assert!(report.mods[0].overridden > 0, "先装的那份被盖住了");
        assert_eq!(report.mods[0].covers, 0);
        assert!(report.mods[0].by.contains(&"后装的".to_owned()));
        assert!(report.mods[1].covers > 0, "后装的那份盖住了前一份");
        assert_eq!(report.mods[1].overridden, 0);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn 第二次读同一个包会命中缓存() {
        let root = temp_root("cache");
        let first = manifest_for(&fixture(), &root).expect("第一次");
        let cached = cache_file(&root, &first.sha256);
        assert!(cached.exists(), "缓存文件应该写出来了");

        // 缓存文件存在时，即使源文件被换掉也直接返回缓存内容（按内容命名，不会串）
        let second = manifest_for(&fixture(), &root).expect("第二次");
        assert_eq!(first.sha256, second.sha256);
        assert_eq!(first.paths, second.paths);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn 没有pak的压缩包会留下一条警告() {
        let root = temp_root("nopak");
        let zip_path = root.join("只有文本.zip");
        {
            let file = std::fs::File::create(&zip_path).expect("建 zip");
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file("readme.txt", zip::write::SimpleFileOptions::default())
                .expect("写条目");
            std::io::Write::write_all(&mut writer, b"nothing here").expect("写内容");
            writer.finish().expect("收尾");
        }

        let manifest = manifest_for(&zip_path, &root).expect("应该能读");
        assert!(manifest.paths.is_empty(), "不该有资源路径");
        assert!(
            manifest.warnings.iter().any(|w| w.contains("没有 .pak")),
            "应该有警告：{:?}",
            manifest.warnings
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn 装同一个包会跟已装的撞上() {
        let root = temp_root("clash");
        let incoming = vec![Incoming {
            name: "新下的".to_owned(),
            path: fixture(),
        }];
        let installed = vec![Incoming {
            name: "装着的".to_owned(),
            path: fixture(),
        }];

        let report = precheck(&incoming, &installed, &root).expect("预检应该成功");
        assert!(!report.conflicts.is_empty(), "同一个包必然撞");
        assert!(report.summary.contains("装着的"), "说明里要点出被盖的人：{}", report.summary);
        assert!(
            report.summary.contains("只有排在后面的会生效"),
            "要说人话：{}",
            report.summary
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn 已装的包不在本机时只留警告不报错() {
        let root = temp_root("gone");
        let incoming = vec![Incoming {
            name: "新下的".to_owned(),
            path: fixture(),
        }];
        let installed = vec![Incoming {
            name: "已经删掉的".to_owned(),
            path: root.join("不存在.zip"),
        }];

        let report = precheck(&incoming, &installed, &root).expect("不该报错");
        assert!(report.conflicts.is_empty());
        assert!(
            report.warnings.iter().any(|w| w.contains("已经不在本机")),
            "应该有警告：{:?}",
            report.warnings
        );
        assert!(report.summary.contains("不撞"));
        std::fs::remove_dir_all(&root).ok();
    }
}
