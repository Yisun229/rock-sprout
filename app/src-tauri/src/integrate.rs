//! 从 mint 搬来的整合逻辑：把若干 mod 合并成游戏能读的一个 pak。
//!
//! 来源：https://github.com/trumank/mint 提交 57aa964e7789f132481813a8f4b5781f924eccd1
//! 许可证 MIT（见 src-tauri/LICENSE-MINT）。
//!
//! 与原始文件的差别：模块路径改成本工程的（crate::providers / crate::mod_lints /
//! mint_lib）；加载器 DLL 现在也会一起写（本工程默认带上 feature `hook`）。

use std::collections::{HashMap, HashSet};
use std::io::{BufReader, BufWriter, Cursor, Read, Seek, Write};
use std::path::{Path, PathBuf};

use fs_err as fs;

use repak::PakWriter;
use snafu::prelude::*;
use tracing::{info, warn};
use uasset_utils::asset_registry::{AssetRegistry, Readable as _, Writable as _};
use uasset_utils::paths::{PakPath, PakPathBuf, PakPathComponentTrait};
use uasset_utils::splice::{
    AssetVersion, TrackedStatement, extract_tracked_statements, inject_tracked_statements, walk,
};
use unreal_asset::AssetBuilder;
use unreal_asset::engine_version::EngineVersion;

use crate::mod_lints::LintError;
use crate::providers::{ModInfo, ProviderError, ReadSeek};
use mint_lib::DRGInstallation;
use mint_lib::mod_info::{ApprovalStatus, Meta, MetaConfig, MetaMod};

/// 我们内嵌的加载器 DLL 的 sha256（算一次就记住）。
///
/// 判「这个 DLL 是不是我们写的」不能再靠比大小了：换个编译器版本大小就会变，
/// 万一同目录里正好有个同样大小的别的文件，就会认错。比内容才认得准。
#[cfg(feature = "hook")]
pub fn hook_dll_sha256() -> Option<[u8; 32]> {
    use sha2::{Digest, Sha256};
    use std::sync::OnceLock;

    static DIGEST: OnceLock<[u8; 32]> = OnceLock::new();
    Some(*DIGEST.get_or_init(|| {
        let mut hasher = Sha256::new();
        hasher.update(include_bytes!(env!("CARGO_CDYLIB_FILE_HOOK_hook")));
        hasher.finalize().into()
    }))
}

#[cfg(not(feature = "hook"))]
pub fn hook_dll_sha256() -> Option<[u8; 32]> {
    None
}


use unreal_asset::{
    Asset,
    exports::ExportBaseTrait,
    flags::EObjectFlags,
    kismet::{
        EExprToken, ExByteConst, ExCallMath, ExLet, ExLetObj, ExLocalVariable, ExRotationConst,
        ExSelf, ExSoftObjectConst, ExStringConst, ExVectorConst, FieldPath, KismetPropertyPointer,
    },
    kismet::{ExFalse, KismetExpression},
    types::PackageIndex,
    types::vector::Vector,
};

static INTEGRATION_DIR: include_dir::Dir<'_> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/assets/integration");

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub enum IntegrationError {
    #[snafu(display("unable to determine DRG installation at provided path {}", path.display()))]
    DrgInstallationNotFound { path: PathBuf },
    #[snafu(transparent)]
    IoError { source: std::io::Error },
    #[snafu(transparent)]
    RepakError { source: repak::Error },
    #[snafu(transparent)]
    UnrealAssetError { source: unreal_asset::Error },
    #[snafu(display("mod {:?}: I/O error encountered during its processing", mod_info.name))]
    CtxtIoError {
        source: std::io::Error,
        mod_info: Box<ModInfo>,
    },
    #[snafu(display("mod {:?}: repak error encountered during its processing", mod_info.name))]
    CtxtRepakError {
        source: repak::Error,
        mod_info: Box<ModInfo>,
    },
    #[snafu(display(
        "mod {:?}: modfile {} contains unexpected prefix",
        mod_info.name,
        modfile_path
    ))]
    ModfileInvalidPrefix {
        mod_info: Box<ModInfo>,
        modfile_path: String,
    },
    #[snafu(display(
        "mod {:?}: failed to integrate: {source}",
        mod_info.name,
    ))]
    CtxtGenericError {
        source: Box<dyn std::error::Error + Send + Sync>,
        mod_info: Box<ModInfo>,
    },
    #[snafu(transparent)]
    ProviderError { source: ProviderError },
    #[snafu(display("integration error: {msg}"))]
    GenericError { msg: String },
    #[snafu(transparent)]
    JoinError { source: tokio::task::JoinError },
    #[snafu(transparent)]
    LintError { source: LintError },
    #[snafu(display("self update failed: {source:?}"))]
    SelfUpdateFailed {
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl IntegrationError {
    pub fn opt_mod_id(&self) -> Option<u32> {
        match self {
            IntegrationError::CtxtIoError { mod_info, .. }
            | IntegrationError::CtxtRepakError { mod_info, .. }
            | IntegrationError::CtxtGenericError { mod_info, .. }
            | IntegrationError::ModfileInvalidPrefix { mod_info, .. } => mod_info.modio_id,
            IntegrationError::ProviderError { source } => source.opt_mod_id(),
            _ => None,
        }
    }
}

#[tracing::instrument(skip_all)]
pub fn integrate<P: AsRef<Path>>(
    path_pak: P,
    config: MetaConfig,
    mods: Vec<(ModInfo, PathBuf)>,
) -> Result<(), IntegrationError> {
    let Ok(installation) = DRGInstallation::from_pak_path(&path_pak) else {
        return Err(IntegrationError::DrgInstallationNotFound {
            path: path_pak.as_ref().to_path_buf(),
        });
    };
    let path_mod_pak = installation.paks_path().join("mods_P.pak");
    let (_, path_mod_pak_tmp, _) = pak_paths(&installation.paks_path());

    let mut fsd_pak_reader = BufReader::new(fs::File::open(path_pak.as_ref())?);
    let fsd_pak = repak::PakBuilder::new().reader(&mut fsd_pak_reader)?;

    #[derive(Debug, Default)]
    struct RawAsset {
        uasset: Option<Vec<u8>>,
        uexp: Option<Vec<u8>>,
    }

    impl RawAsset {
        fn parse(&self) -> Result<Asset<Cursor<&Vec<u8>>>, IntegrationError> {
            Ok(AssetBuilder::new(
                Cursor::new(self.uasset.as_ref().unwrap()),
                EngineVersion::VER_UE4_27,
            )
            .bulk(Cursor::new(self.uexp.as_ref().unwrap()))
            .build()?)
        }
    }

    let ar_path = "FSD/AssetRegistry.bin";
    let mut asset_registry =
        AssetRegistry::read(&mut Cursor::new(fsd_pak.get(ar_path, &mut fsd_pak_reader)?))
            .map_err(|e| IntegrationError::GenericError { msg: e.to_string() })?;

    let mut other_deferred = vec![];
    let mut deferred = |path| {
        other_deferred.push(path);
        path
    };

    let pcb_path = "FSD/Content/Game/BP_PlayerControllerBase";
    let patch_paths = [
        "FSD/Content/Game/BP_GameInstance",
        "FSD/Content/Game/SpaceRig/BP_PlayerController_SpaceRig",
        "FSD/Content/Game/StartMenu/Bp_StartMenu_PlayerController",
        "FSD/Content/UI/Menu_DeepDives/ITM_DeepDives_Join",
        "FSD/Content/UI/Menu_ServerList/_MENU_ServerList",
        "FSD/Content/UI/Menu_ServerList/WND_JoiningModded",
    ];
    let escape_menu_path = deferred("FSD/Content/UI/Menu_EscapeMenu/MENU_EscapeMenu");
    let modding_tab_path = deferred("FSD/Content/UI/Menu_EscapeMenu/Modding/MENU_Modding");
    let server_list_entry_path = deferred("FSD/Content/UI/Menu_ServerList/ITM_ServerList_Entry");

    let mut deferred_assets: HashMap<&str, RawAsset> = HashMap::from_iter(
        [pcb_path]
            .iter()
            .chain(patch_paths.iter())
            .chain(other_deferred.iter())
            .map(|path| (*path, RawAsset::default())),
    );

    // collect assets from game pak file
    for (path, asset) in &mut deferred_assets {
        // TODO repak should return an option...
        asset.uasset = match fsd_pak.get(&format!("{path}.uasset"), &mut fsd_pak_reader) {
            Ok(file) => Ok(Some(file)),
            Err(repak::Error::MissingEntry(_)) => Ok(None),
            Err(e) => Err(e),
        }?;
        asset.uexp = match fsd_pak.get(&format!("{path}.uexp"), &mut fsd_pak_reader) {
            Ok(file) => Ok(Some(file)),
            Err(repak::Error::MissingEntry(_)) => Ok(None),
            Err(e) => Err(e),
        }?;
    }

    // 先写到 mods_P.pak.tmp，等全部写完、校验通过之后再换上去。
    // 这样中途失败的话，游戏目录里那份旧的合并包一直是完整的。
    let mut bundle = ModBundleWriter::new(
        BufWriter::new(
            fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&path_mod_pak_tmp)?,
        ),
        &fsd_pak.files(),
    )?;

    #[cfg(feature = "hook")]
    {
        let path_hook_dll = installation
            .binaries_directory()
            .join(installation.installation_type.hook_dll_name());
        let hook_dll = include_bytes!(env!("CARGO_CDYLIB_FILE_HOOK_hook"));
        if path_hook_dll
            .metadata()
            .map(|m| m.len() != hook_dll.len() as u64)
            .unwrap_or(true)
        {
            fs::write(&path_hook_dll, hook_dll)?;
        }
    }

    let mut init_spacerig_assets = HashSet::new();
    let mut init_cave_assets = HashSet::new();

    let mut added_paths = HashSet::new();

    for (mod_info, path) in &mods {
        let raw_mod_file = fs::File::open(path).with_context(|_| CtxtIoSnafu {
            mod_info: mod_info.clone(),
        })?;
        let mut buf = get_pak_from_data(Box::new(BufReader::new(raw_mod_file))).map_err(|e| {
            if let IntegrationError::IoError { source } = e {
                IntegrationError::CtxtIoError {
                    source,
                    mod_info: mod_info.clone().into(),
                }
            } else {
                e
            }
        })?;
        let pak = repak::PakBuilder::new()
            .reader(&mut buf)
            .with_context(|_| CtxtRepakSnafu {
                mod_info: mod_info.clone(),
            })?;

        let mount = PakPath::new(pak.mount_point());

        let pak_files = pak
            .files()
            .into_iter()
            .map(|p| -> Result<_, IntegrationError> {
                let j = mount.join(&p);
                Ok((
                    j.strip_prefix("../../../")
                        .map_err(|_| IntegrationError::ModfileInvalidPrefix {
                            mod_info: mod_info.clone().into(),
                            modfile_path: j.to_string(),
                        })?
                        .to_path_buf(),
                    p,
                ))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;

        for (normalized, pak_path) in &pak_files {
            match normalized.extension() {
                Some("uasset" | "umap")
                    if pak_files.contains_key(&normalized.with_extension("uexp")) =>
                {
                    let uasset = pak
                        .get(pak_path, &mut buf)
                        .with_context(|_| CtxtRepakSnafu {
                            mod_info: mod_info.clone(),
                        })?;

                    let uexp = pak
                        .get(
                            PakPath::new(pak_path).with_extension("uexp").as_str(),
                            &mut buf,
                        )
                        .with_context(|_| CtxtRepakSnafu {
                            mod_info: mod_info.clone(),
                        })?;

                    let asset = AssetBuilder::new(Cursor::new(uasset), EngineVersion::VER_UE4_27)
                        .bulk(Cursor::new(uexp))
                        .skip_data(true)
                        .build();

                    let asset = match asset {
                        Ok(asset) => asset,
                        Err(err) => {
                            warn!("failed to parse asset {normalized}: {err}");
                            continue;
                        }
                    };

                    asset_registry
                        .populate(normalized.with_extension("").as_str(), &asset)
                        .map_err(|e| IntegrationError::CtxtGenericError {
                            source: e.into(),
                            mod_info: mod_info.clone().into(),
                        })?;
                }
                _ => {}
            }
        }

        for (normalized, pak_path) in pak_files {
            let lowercase = normalized.as_str().to_ascii_lowercase();
            if added_paths.contains(&lowercase) {
                continue;
            }

            if let Some(filename) = normalized.file_name() {
                if filename == "AssetRegistry.bin" {
                    continue;
                }
                if normalized.extension() == Some("ushaderbytecode") {
                    continue;
                }
                let lower = filename.to_lowercase();
                if lower == "initspacerig.uasset" {
                    init_spacerig_assets.insert(format_soft_class(&normalized));
                }
                if lower == "initcave.uasset" {
                    init_cave_assets.insert(format_soft_class(&normalized));
                }
            }

            let file_data = pak
                .get(&pak_path, &mut buf)
                .with_context(|_| CtxtRepakSnafu {
                    mod_info: mod_info.clone(),
                })?;
            if let Some(raw) = normalized
                .as_str()
                .strip_suffix(".uasset")
                .and_then(|path| deferred_assets.get_mut(path))
            {
                raw.uasset = Some(file_data);
            } else if let Some(raw) = normalized
                .as_str()
                .strip_suffix(".uexp")
                .and_then(|path| deferred_assets.get_mut(path))
            {
                raw.uexp = Some(file_data);
            } else {
                bundle.write_file(&file_data, normalized.as_str())?;
                added_paths.insert(lowercase);
            }
        }
    }

    {
        let mut pcb_asset = deferred_assets[&pcb_path].parse()?;
        hook_pcb(&mut pcb_asset);
        bundle.write_asset(pcb_asset, pcb_path)?;
    }

    let mut patch_deferred = |path_str: &str,
                              f: fn(&mut _) -> Result<(), IntegrationError>|
     -> Result<(), IntegrationError> {
        let mut asset = deferred_assets[path_str].parse()?;
        f(&mut asset)?;
        bundle.write_asset(asset, path_str)
    };

    // apply patches to base assets
    for patch_path in patch_paths {
        patch_deferred(patch_path, patch)?;
    }
    patch_deferred(escape_menu_path, patch_modding_tab)?;
    patch_deferred(modding_tab_path, patch_modding_tab_item)?;
    patch_deferred(server_list_entry_path, patch_server_list_entry)?;

    let mut int_files = HashMap::new();
    collect_dir_files(&INTEGRATION_DIR, &mut int_files);

    for (path, data) in &int_files {
        bundle.write_file(data, path)?;
    }

    bundle.write_meta(config, &mods)?;

    let mut buf = vec![];
    asset_registry
        .write(&mut buf)
        .map_err(|e| IntegrationError::GenericError { msg: e.to_string() })?;
    bundle.write_file(&buf, ar_path)?;

    let written = bundle.finish()?;

    // 写完先自己读一遍，条目数对不上就当这次没成功，旧包别动
    let paks_dir = installation.paks_path();
    if let Err(e) = verify_pak(&path_mod_pak_tmp, written) {
        let _ = fs::remove_file(&path_mod_pak_tmp);
        return Err(e);
    }
    swap_in(&paks_dir)?;

    info!(
        "{} mods installed to {}",
        mods.len(),
        path_mod_pak.display()
    );

    Ok(())
}

/// 合并包的三条路径：正式名、正在写的临时名、刚被换下来的旧包名。
pub(crate) fn pak_paths(paks_dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    (
        paks_dir.join("mods_P.pak"),
        paks_dir.join("mods_P.pak.tmp"),
        paks_dir.join("mods_P.pak.prev"),
    )
}

/// 重新读一遍刚写完的临时包，条目数对不上就认为这次没写成功。
fn verify_pak(path: &Path, expected: usize) -> Result<(), IntegrationError> {
    let mut reader = BufReader::new(fs::File::open(path)?);
    let pak = repak::PakBuilder::new().reader(&mut reader)?;
    let actual = pak.files().len();
    if actual != expected {
        return Err(IntegrationError::GenericError {
            msg: format!("合并包校验没过：写了 {expected} 条，重新读出来是 {actual} 条"),
        });
    }
    Ok(())
}

/// 把写好的临时包换成正式包。
///
/// 顺序是「旧包先改名成 `.prev` → 临时包改名成正式名 → 删掉 `.prev`」，
/// 不用「先删旧包再改名」——那样中间有一段时间游戏目录里根本没有合并包。
pub(crate) fn swap_in(paks_dir: &Path) -> Result<(), IntegrationError> {
    let (main, tmp, prev) = pak_paths(paks_dir);
    if prev.exists() {
        fs::remove_file(&prev)?;
    }
    let had_old = main.exists();
    if had_old {
        fs::rename(&main, &prev)?;
    }
    if let Err(e) = fs::rename(&tmp, &main) {
        if had_old {
            // 换上去失败：把旧包改回原名，别让用户手上什么都没有
            let _ = fs::rename(&prev, &main);
        }
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    if had_old {
        let _ = fs::remove_file(&prev);
    }
    Ok(())
}

/// 清理上次没写完留下的东西，返回清掉了几个文件。
///
/// - 只剩 `.prev`、没有正式包：说明上次断在「旧包改名完、新包还没换上」，把旧包改回原名；
/// - 正式包和 `.prev` 都在：`swap_in` 没走完，`.prev` 是多余的，删掉；
/// - `.tmp` 一律删掉（它是没写完的半成品）。
pub fn clean_stale(paks_dir: &Path) -> Result<usize, IntegrationError> {
    let (main, tmp, prev) = pak_paths(paks_dir);
    let mut cleaned = 0;
    if prev.exists() {
        if main.exists() {
            fs::remove_file(&prev)?;
            info!("清掉多余的 {}（正式包已经在）", prev.display());
        } else {
            fs::rename(&prev, &main)?;
            info!("上次没换完，把旧包改回原名：{}", main.display());
        }
        cleaned += 1;
    }
    if tmp.exists() {
        fs::remove_file(&tmp)?;
        info!("清掉没写完的 {}", tmp.display());
        cleaned += 1;
    }
    Ok(cleaned)
}

fn collect_dir_files(dir: &'static include_dir::Dir, collect: &mut HashMap<String, &[u8]>) {
    for entry in dir.entries() {
        match entry {
            include_dir::DirEntry::Dir(dir) => {
                collect_dir_files(dir, collect);
            }
            include_dir::DirEntry::File(file) => {
                collect.insert(
                    file.path().to_str().unwrap().replace('\\', "/"),
                    file.contents(),
                );
            }
        }
    }
}

fn format_soft_class<P: AsRef<PakPath>>(path: P) -> String {
    let path = path.as_ref();
    let name = path.file_stem().unwrap();
    format!(
        "/Game/{}{}_C",
        path.strip_prefix("FSD/Content")
            .unwrap()
            .as_str()
            .strip_suffix("uasset")
            .unwrap(),
        name
    )
}

struct ModBundleWriter<W: Write + Seek> {
    pak_writer: PakWriter<W>,
    directories: HashMap<String, Dir>,
    /// 已经写进去多少条。写完要用它跟重新读出来的条目数对一下，确认文件是完整的。
    written: usize,
}

impl<W: Write + Seek> ModBundleWriter<W> {
    fn new(writer: W, fsd_paths: &[String]) -> Result<Self, IntegrationError> {
        let mut directories: HashMap<String, Dir> = HashMap::new();
        for f in fsd_paths {
            let mut dir = &mut directories;
            for c in PakPath::new(f).components() {
                dir = &mut dir
                    .entry(c.as_str().to_ascii_lowercase())
                    .or_insert(Dir {
                        name: c.to_string(),
                        children: Default::default(),
                    })
                    .children;
            }
        }

        Ok(Self {
            pak_writer: repak::PakBuilder::new()
                .compression([repak::Compression::Zlib])
                .writer(writer, repak::Version::V11, "../../../".to_string(), None),
            directories,
            written: 0,
        })
    }
    /// Used to normalize match path case to existing files in the DRG pak.
    fn normalize_path(&self, path_str: &str) -> PakPathBuf {
        let mut dir = Some(&self.directories);
        let path = PakPath::new(path_str);
        let mut normalized_path = PakPathBuf::new();
        for c in path.components() {
            if let Some(entry) = dir.and_then(|d| d.get(&c.as_str().to_ascii_lowercase())) {
                normalized_path.push(&entry.name);
                dir = Some(&entry.children);
            } else {
                normalized_path.push(c);
            }
        }
        normalized_path
    }

    fn write_file(&mut self, data: &[u8], path: &str) -> Result<(), IntegrationError> {
        self.pak_writer
            .write_file(self.normalize_path(path).as_str(), data)?;
        self.written += 1;
        Ok(())
    }

    fn write_asset<C: Read + Seek>(
        &mut self,
        asset: Asset<C>,
        path: &str,
    ) -> Result<(), IntegrationError> {
        let mut data_out = (Cursor::new(vec![]), Cursor::new(vec![]));

        asset.write_data(&mut data_out.0, Some(&mut data_out.1))?;
        data_out.0.rewind()?;
        data_out.1.rewind()?;

        self.write_file(&data_out.0.into_inner(), &format!("{path}.uasset"))?;
        self.write_file(&data_out.1.into_inner(), &format!("{path}.uexp"))?;

        Ok(())
    }

    fn write_meta(
        &mut self,
        config: MetaConfig,
        mods: &[(ModInfo, PathBuf)],
    ) -> Result<(), IntegrationError> {
        let meta = Meta {
            version: env!("CARGO_PKG_VERSION").into(),
            config,
            mods: mods
                .iter()
                .map(|(info, _)| MetaMod {
                    name: info.name.clone(),
                    version: "TODO".into(), // TODO
                    author: "TODO".into(),  // TODO
                    required: info.suggested_require,
                    url: info.resolution.get_resolvable_url_or_name().to_string(),
                    approval: info
                        .modio_tags
                        .as_ref()
                        .map(|t| t.approval_status)
                        .unwrap_or(ApprovalStatus::Sandbox),
                })
                .collect(),
        };
        self.write_file(&postcard::to_allocvec(&meta).unwrap(), "meta")?;
        Ok(())
    }

    fn finish(self) -> Result<usize, IntegrationError> {
        self.pak_writer.write_index()?;
        Ok(self.written)
    }
}

#[derive(Debug, Default)]
struct Dir {
    name: String,
    children: HashMap<String, Dir>,
}

pub(crate) fn get_pak_from_data(
    mut data: Box<dyn ReadSeek>,
) -> Result<Box<dyn ReadSeek>, IntegrationError> {
    if let Ok(mut archive) = zip::ZipArchive::new(&mut data) {
        (0..archive.len())
            .map(|i| -> Result<Option<Box<dyn ReadSeek>>, IntegrationError> {
                let mut file = archive
                    .by_index(i)
                    .map_err(|_| IntegrationError::GenericError {
                        msg: "failed to extract file in zip archive".to_string(),
                    })?;
                match file.enclosed_name() {
                    Some(p) => {
                        if file.is_file() && p.extension() == Some(std::ffi::OsStr::new("pak")) {
                            let mut buf = vec![];
                            file.read_to_end(&mut buf)?;
                            Ok(Some(Box::new(Cursor::new(buf))))
                        } else {
                            Ok(None)
                        }
                    }
                    None => Ok(None),
                }
            })
            .find_map(Result::transpose)
            .context(GenericSnafu {
                msg: "zip archive does not contain pak",
            })?
    } else {
        data.rewind()?;
        Ok(data)
    }
}

type ImportChain<'a> = Vec<Import<'a>>;

struct Import<'a> {
    class_package: &'a str,
    class_name: &'a str,
    object_name: &'a str,
}
impl<'a> Import<'a> {
    fn new(class_package: &'a str, class_name: &'a str, object_name: &'a str) -> Import<'a> {
        Import {
            class_package,
            class_name,
            object_name,
        }
    }
}

fn get_import<R: Read + Seek>(asset: &mut Asset<R>, import: ImportChain) -> PackageIndex {
    let mut pi = PackageIndex::new(0);
    for i in import {
        let ai = &asset
            .imports
            .iter()
            .enumerate()
            .find(|(_, ai)| {
                ai.class_package.get_content(|n| n == i.class_package)
                    && ai.class_name.get_content(|n| n == i.class_name)
                    && ai.object_name.get_content(|n| n == i.object_name)
                    && ai.outer_index == pi
            })
            .map(|(index, _)| PackageIndex::from_import(index as i32).unwrap());
        pi = ai.unwrap_or_else(|| {
            let new_import = unreal_asset::Import::new(
                asset.add_fname(i.class_package),
                asset.add_fname(i.class_name),
                pi,
                asset.add_fname(i.object_name),
                false,
            );
            asset.add_import(new_import)
        });
    }
    pi
}

/// "it's only 3 instructions"
/// "how much boilerplate could there possibly be"
fn hook_pcb<R: Read + Seek>(asset: &mut Asset<R>) {
    let transform = get_import(
        asset,
        vec![
            Import::new("/Script/CoreUObject", "Package", "/Script/CoreUObject"),
            Import::new("/Script/CoreUObject", "ScriptStruct", "Transform"),
        ],
    );
    let actor = get_import(
        asset,
        vec![
            Import::new("/Script/CoreUObject", "Package", "/Script/Engine"),
            Import::new("/Script/CoreUObject", "Class", "Actor"),
        ],
    );
    let load_class = get_import(
        asset,
        vec![
            Import::new("/Script/CoreUObject", "Package", "/Script/Engine"),
            Import::new("/Script/CoreUObject", "Class", "KismetSystemLibrary"),
            Import::new("/Script/CoreUObject", "Function", "LoadClassAsset_Blocking"),
        ],
    );
    let make_transform = get_import(
        asset,
        vec![
            Import::new("/Script/CoreUObject", "Package", "/Script/Engine"),
            Import::new("/Script/CoreUObject", "Class", "KismetMathLibrary"),
            Import::new("/Script/CoreUObject", "Function", "MakeTransform"),
        ],
    );
    let begin_spawning = get_import(
        asset,
        vec![
            Import::new("/Script/CoreUObject", "Package", "/Script/Engine"),
            Import::new("/Script/CoreUObject", "Class", "GameplayStatics"),
            Import::new(
                "/Script/CoreUObject",
                "Function",
                "BeginDeferredActorSpawnFromClass",
            ),
        ],
    );
    let finish_spawning = get_import(
        asset,
        vec![
            Import::new("/Script/CoreUObject", "Package", "/Script/Engine"),
            Import::new("/Script/CoreUObject", "Class", "GameplayStatics"),
            Import::new("/Script/CoreUObject", "Function", "FinishSpawningActor"),
        ],
    );
    let ex_transform = ExCallMath {
        token: EExprToken::ExCallMath,
        stack_node: make_transform,
        parameters: vec![
            ExVectorConst {
                token: EExprToken::ExVectorConst,
                value: unreal_asset::types::vector::Vector::new(
                    0f64.into(),
                    0f64.into(),
                    0f64.into(),
                ),
            }
            .into(),
            ExRotationConst {
                token: EExprToken::ExVectorConst,
                rotator: Vector::new(0f64.into(), 0f64.into(), 0f64.into()),
            }
            .into(),
            ExVectorConst {
                token: EExprToken::ExVectorConst,
                value: unreal_asset::types::vector::Vector::new(
                    1f64.into(),
                    1f64.into(),
                    1f64.into(),
                ),
            }
            .into(),
        ],
    };
    let prop_class_name = asset.add_fname("begin_spawn");
    let prop_class = unreal_asset::fproperty::FObjectProperty {
        generic_property: unreal_asset::fproperty::FGenericProperty {
            name: prop_class_name.clone(),
            flags: EObjectFlags::RF_PUBLIC,
            array_dim: unreal_asset::enums::EArrayDim::TArray,
            element_size: 8,
            property_flags: unreal_asset::flags::EPropertyFlags::CPF_NONE,
            rep_index: 0,
            rep_notify_func: asset.add_fname("None"),
            blueprint_replication_condition: unreal_asset::enums::ELifetimeCondition::CondNone,
            serialized_type: Some(asset.add_fname("ClassProperty")),
        },
        property_class: actor,
    };
    let prop_transform_name = asset.add_fname("transform");
    let prop_transform = unreal_asset::fproperty::FStructProperty {
        generic_property: unreal_asset::fproperty::FGenericProperty {
            name: prop_transform_name.clone(),
            flags: EObjectFlags::RF_PUBLIC,
            array_dim: unreal_asset::enums::EArrayDim::TArray,
            element_size: 48,
            property_flags: unreal_asset::flags::EPropertyFlags::CPF_NONE,
            rep_index: 0,
            rep_notify_func: asset.add_fname("None"),
            blueprint_replication_condition: unreal_asset::enums::ELifetimeCondition::CondNone,
            serialized_type: Some(asset.add_fname("StructProperty")),
        },
        struct_value: transform,
    };
    let prop_begin_spawn_name = asset.add_fname("begin_spawn");
    let prop_begin_spawn = unreal_asset::fproperty::FObjectProperty {
        generic_property: unreal_asset::fproperty::FGenericProperty {
            name: prop_begin_spawn_name.clone(),
            flags: EObjectFlags::RF_PUBLIC,
            array_dim: unreal_asset::enums::EArrayDim::TArray,
            element_size: 8,
            property_flags: unreal_asset::flags::EPropertyFlags::CPF_NONE,
            rep_index: 0,
            rep_notify_func: asset.add_fname("None"),
            blueprint_replication_condition: unreal_asset::enums::ELifetimeCondition::CondNone,
            serialized_type: Some(asset.add_fname("ObjectProperty")),
        },
        property_class: actor,
    };

    let (fi, func) = asset
        .asset_data
        .exports
        .iter_mut()
        .enumerate()
        .find_map(|(i, e)| {
            if let unreal_asset::exports::Export::FunctionExport(func) = e
                && func
                    .get_base_export()
                    .object_name
                    .get_content(|n| n == "ReceiveBeginPlay")
            {
                return Some((PackageIndex::from_export(i as i32).unwrap(), func));
            }
            None
        })
        .unwrap();

    func.struct_export.loaded_properties.push(prop_class.into());
    func.struct_export
        .loaded_properties
        .push(prop_transform.into());
    func.struct_export
        .loaded_properties
        .push(prop_begin_spawn.into());
    let inst = func.struct_export.script_bytecode.as_mut().unwrap();
    inst.insert(
        0,
        ExLetObj {
            token: EExprToken::ExLetObj,
            variable_expression: Box::new(
                ExLocalVariable {
                    token: EExprToken::ExLocalVariable,
                    variable: KismetPropertyPointer {
                        old: None,
                        new: Some(FieldPath {
                            path: vec![prop_class_name.clone()],
                            resolved_owner: fi,
                        }),
                    },
                }
                .into(),
            ),
            assignment_expression: Box::new(
                ExCallMath {
                    token: EExprToken::ExCallMath,
                    stack_node: load_class,
                    parameters: vec![
                        ExSoftObjectConst {
                            token: EExprToken::ExSoftObjectConst,
                            value: Box::new(
                                ExStringConst {
                                    token: EExprToken::ExStringConst,
                                    value: "/Game/_AssemblyStorm/ModIntegration/MI_SpawnMods.MI_SpawnMods_C".to_string()
                                }.into()
                            )
                        }
                        .into()
                    ]
                }
                .into(),
            ),
        }
        .into(),
    );
    inst.insert(
        1,
        ExLet {
            token: EExprToken::ExLet,
            value: KismetPropertyPointer {
                old: None,
                new: Some(FieldPath {
                    path: vec![prop_transform_name.clone()],
                    resolved_owner: fi,
                }),
            },
            variable: Box::new(
                ExLocalVariable {
                    token: EExprToken::ExLocalVariable,
                    variable: KismetPropertyPointer {
                        old: None,
                        new: Some(FieldPath {
                            path: vec![prop_transform_name.clone()],
                            resolved_owner: fi,
                        }),
                    },
                }
                .into(),
            ),
            expression: Box::new(ex_transform.into()),
        }
        .into(),
    );

    inst.insert(
        2,
        ExLetObj {
            token: EExprToken::ExLetObj,
            variable_expression: Box::new(
                ExLocalVariable {
                    token: EExprToken::ExLocalVariable,
                    variable: KismetPropertyPointer {
                        old: None,
                        new: Some(FieldPath {
                            path: vec![prop_begin_spawn_name.clone()],
                            resolved_owner: fi,
                        }),
                    },
                }
                .into(),
            ),
            assignment_expression: Box::new(
                ExCallMath {
                    token: EExprToken::ExCallMath,
                    stack_node: begin_spawning,
                    parameters: vec![
                        ExSelf {
                            token: EExprToken::ExSelf,
                        }
                        .into(),
                        ExLocalVariable {
                            token: EExprToken::ExLocalVariable,
                            variable: KismetPropertyPointer {
                                old: None,
                                new: Some(FieldPath {
                                    path: vec![prop_class_name],
                                    resolved_owner: fi,
                                }),
                            },
                        }
                        .into(),
                        ExLocalVariable {
                            token: EExprToken::ExLocalVariable,
                            variable: KismetPropertyPointer {
                                old: None,
                                new: Some(FieldPath {
                                    path: vec![prop_transform_name.clone()],
                                    resolved_owner: fi,
                                }),
                            },
                        }
                        .into(),
                        ExByteConst {
                            token: EExprToken::ExByteConst,
                            value: 1,
                        }
                        .into(),
                        ExSelf {
                            token: EExprToken::ExSelf,
                        }
                        .into(),
                    ],
                }
                .into(),
            ),
        }
        .into(),
    );

    inst.insert(
        3,
        ExCallMath {
            token: EExprToken::ExCallMath,
            stack_node: finish_spawning,
            parameters: vec![
                ExLocalVariable {
                    token: EExprToken::ExLocalVariable,
                    variable: KismetPropertyPointer {
                        old: None,
                        new: Some(FieldPath {
                            path: vec![prop_begin_spawn_name],
                            resolved_owner: fi,
                        }),
                    },
                }
                .into(),
                ExLocalVariable {
                    token: EExprToken::ExLocalVariable,
                    variable: KismetPropertyPointer {
                        old: None,
                        new: Some(FieldPath {
                            path: vec![prop_transform_name],
                            resolved_owner: fi,
                        }),
                    },
                }
                .into(),
            ],
        }
        .into(),
    );
}

fn patch<C: Seek + Read>(asset: &mut Asset<C>) -> Result<(), IntegrationError> {
    let ver = AssetVersion::new_from(asset);
    let mut statements = extract_tracked_statements(asset, ver, &None);

    let find_function = |name| {
        asset
            .imports
            .iter()
            .enumerate()
            .find(|(_, i)| {
                i.class_package.get_content(|s| s == "/Script/CoreUObject")
                    && i.class_name.get_content(|s| s == "Function")
                    && i.object_name.get_content(|s| s == name)
            })
            .map(|(pi, _)| PackageIndex::from_import(pi as i32).unwrap())
    };

    fn patch_ismodded(
        is_modded: Option<PackageIndex>,
        is_modded_sandbox: Option<PackageIndex>,
        mut statement: TrackedStatement,
    ) -> Option<TrackedStatement> {
        walk(&mut statement.ex, &|ex| {
            if let KismetExpression::ExCallMath(f) = ex
                && (Some(f.stack_node) == is_modded || Some(f.stack_node) == is_modded_sandbox)
            {
                *ex = ExFalse::default().into()
            }
        });
        Some(statement)
    }

    let is_modded = find_function("FSDIsModdedServer");
    let is_modded_sandbox = find_function("FSDIsModdedSandboxServer");

    for (_pi, statements) in statements.iter_mut() {
        *statements = std::mem::take(statements)
            .into_iter()
            .filter_map(|s| patch_ismodded(is_modded, is_modded_sandbox, s))
            .collect();
    }
    inject_tracked_statements(asset, ver, statements);
    Ok(())
}

fn patch_modding_tab<C: Seek + Read>(asset: &mut Asset<C>) -> Result<(), IntegrationError> {
    let ver = AssetVersion::new_from(asset);
    let mut statements = extract_tracked_statements(asset, ver, &None);

    for (_pi, statements) in statements.iter_mut() {
        for statement in statements {
            walk(&mut statement.ex, &|ex| {
                if let KismetExpression::ExSetArray(arr) = ex
                    && arr.elements.len() == 2
                {
                    arr.elements.retain(|e| !matches!(e, KismetExpression::ExInstanceVariable(v) if v.variable.new.as_ref().unwrap().path.last().unwrap().get_content(|c| c == "BTN_Modding")));
                    if arr.elements.len() != 2 {
                        info!("patched modding tab visibility");
                    }
                }
            });
        }
    }
    inject_tracked_statements(asset, ver, statements);
    Ok(())
}

fn patch_modding_tab_item<C: Seek + Read>(asset: &mut Asset<C>) -> Result<(), IntegrationError> {
    let itm_tab_modding = get_import(
        asset,
        vec![
            Import::new(
                "/Script/CoreUObject",
                "Package",
                "/Game/UI/Menu_EscapeMenu/Modding/ITM_Tab_Modding",
            ),
            Import::new(
                "/Script/UMG",
                "WidgetBlueprintGeneratedClass",
                "ITM_Tab_Modding_C",
            ),
        ],
    );
    let itm_tab_modding_cdo = get_import(
        asset,
        vec![
            Import::new(
                "/Script/CoreUObject",
                "Package",
                "/Game/UI/Menu_EscapeMenu/Modding/ITM_Tab_Modding",
            ),
            Import::new(
                "/Game/UI/Menu_EscapeMenu/Modding/ITM_Tab_Modding",
                "ITM_Tab_Modding_C",
                "Default__ITM_Tab_Modding_C",
            ),
        ],
    );

    let new_class = asset.add_fname("MI_UI_C");
    let new_cdo = asset.add_fname("Default__MI_UI_C");
    let new_package = asset.add_fname("/Game/_AssemblyStorm/ModIntegration/MI_UI");

    // TODO add get_import_mut or something so indexes don't have to be handled manually

    asset.imports[(-itm_tab_modding_cdo.index - 1) as usize].object_name = new_cdo;
    asset.imports[(-itm_tab_modding_cdo.index - 1) as usize].class_package = new_package.clone();
    asset.imports[(-itm_tab_modding_cdo.index - 1) as usize].class_name = new_class.clone();

    let package_index = {
        let obj = &mut asset.imports[(-itm_tab_modding.index - 1) as usize];
        obj.object_name = new_class;
        obj.outer_index
    };

    asset.imports[(-package_index.index - 1) as usize].object_name = new_package;

    Ok(())
}

fn patch_server_list_entry<C: Seek + Read>(asset: &mut Asset<C>) -> Result<(), IntegrationError> {
    let get_mods_installed = asset
        .imports
        .iter()
        .enumerate()
        .find(|(_, i)| {
            i.class_package.get_content(|s| s == "/Script/CoreUObject")
                && i.class_name.get_content(|s| s == "Function")
                && i.object_name.get_content(|s| s == "FSDGetModsInstalled")
        })
        .map(|(pi, _)| PackageIndex::from_import(pi as i32).unwrap());

    let fsd_target_platform = asset
        .imports
        .iter()
        .enumerate()
        .find(|(_, i)| {
            i.class_package.get_content(|s| s == "/Script/CoreUObject")
                && i.class_name.get_content(|s| s == "Function")
                && i.object_name.get_content(|s| s == "FSDTargetPlatform")
        })
        .map(|(pi, _)| PackageIndex::from_import(pi as i32).unwrap());

    let ver = AssetVersion::new_from(asset);
    let mut statements = extract_tracked_statements(asset, ver, &None);

    for (pi, statements) in statements.iter_mut() {
        let name = &asset
            .asset_data
            .get_export(*pi)
            .unwrap()
            .get_base_export()
            .object_name;

        let swap_platform = name.get_content(|c| ["GetMissionToolTip", "SetSession"].contains(&c));

        for statement in statements {
            walk(&mut statement.ex, &|ex| {
                if let KismetExpression::ExCallMath(cm) = ex {
                    if Some(cm.stack_node) == get_mods_installed && cm.parameters.len() == 2 {
                        cm.parameters[1] = ExFalse {
                            token: EExprToken::ExFalse,
                        }
                        .into();
                        info!("patched server list entry");
                    }
                    if swap_platform && Some(cm.stack_node) == fsd_target_platform {
                        *ex = ExByteConst {
                            token: EExprToken::ExByteConst,
                            value: 0,
                        }
                        .into();
                    }
                }
            });
        }
    }
    inject_tracked_statements(asset, ver, statements);

    {
        // swap out tooltip with rebuilt version
        let itm_tab_modding = get_import(
            asset,
            vec![
                Import::new(
                    "/Script/CoreUObject",
                    "Package",
                    "/Game/UI/Menu_ServerList/TOOLTIP_ServerEntry_Mods",
                ),
                Import::new(
                    "/Script/UMG",
                    "WidgetBlueprintGeneratedClass",
                    "TOOLTIP_ServerEntry_Mods_C",
                ),
            ],
        );
        let itm_tab_modding_cdo = get_import(
            asset,
            vec![
                Import::new(
                    "/Script/CoreUObject",
                    "Package",
                    "/Game/UI/Menu_ServerList/TOOLTIP_ServerEntry_Mods",
                ),
                Import::new(
                    "/Game/UI/Menu_ServerList/TOOLTIP_ServerEntry_Mods",
                    "TOOLTIP_ServerEntry_Mods_C",
                    "Default__TOOLTIP_ServerEntry_Mods_C",
                ),
            ],
        );
        let new_package = asset.add_fname(
            "/Game/_AssemblyStorm/ModIntegration/RebuiltAssets/TOOLTIP_ServerEntry_Mods",
        );
        asset.imports[(-itm_tab_modding_cdo.index - 1) as usize].class_package =
            new_package.clone();
        let package_index = {
            let obj = &mut asset.imports[(-itm_tab_modding.index - 1) as usize];
            obj.outer_index
        };
        asset.imports[(-package_index.index - 1) as usize].object_name = new_package;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paks(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rs-paks-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建测试目录");
        dir
    }

    #[test]
    fn 第一次装的时候没有旧包也能换上() {
        let dir = temp_paks("swap-first");
        let (main, tmp, prev) = pak_paths(&dir);
        fs::write(&tmp, b"new").expect("写临时包");
        swap_in(&dir).expect("应该能换上");
        assert!(main.exists(), "正式包应该出现");
        assert!(!tmp.exists(), "临时包应该没了");
        assert!(!prev.exists(), "不该留下 .prev");
        assert_eq!(fs::read(&main).expect("读正式包"), b"new");
    }

    #[test]
    fn 有旧包时换成新的并且不留下_prev() {
        let dir = temp_paks("swap-old");
        let (main, tmp, prev) = pak_paths(&dir);
        fs::write(&main, b"old").expect("写旧包");
        fs::write(&tmp, b"new").expect("写临时包");
        swap_in(&dir).expect("应该能换上");
        assert_eq!(fs::read(&main).expect("读正式包"), b"new", "正式包应该是新内容");
        assert!(!prev.exists(), "换完之后 .prev 应该删掉");
        assert!(!tmp.exists(), "临时包应该没了");
    }

    #[test]
    fn 上次断在改名之后会把旧包改回原名() {
        let dir = temp_paks("clean-prev");
        let (main, _, prev) = pak_paths(&dir);
        fs::write(&prev, b"old").expect("写 .prev");
        let cleaned = clean_stale(&dir).expect("应该能清理");
        assert_eq!(cleaned, 1, "应该只处理了一个文件");
        assert_eq!(fs::read(&main).expect("读正式包"), b"old", "旧包应该回到正式名");
        assert!(!prev.exists(), ".prev 应该没了");
    }

    #[test]
    fn 正式包和_prev_同时存在时只删_prev() {
        let dir = temp_paks("clean-both");
        let (main, _, prev) = pak_paths(&dir);
        fs::write(&main, b"new").expect("写正式包");
        fs::write(&prev, b"old").expect("写 .prev");
        clean_stale(&dir).expect("应该能清理");
        assert_eq!(fs::read(&main).expect("读正式包"), b"new", "正式包不该被动");
        assert!(!prev.exists(), "多余的 .prev 应该删掉");
    }

    #[test]
    fn 没写完的半成品会被清掉() {
        let dir = temp_paks("clean-tmp");
        let (main, tmp, _) = pak_paths(&dir);
        fs::write(&tmp, b"half").expect("写临时包");
        clean_stale(&dir).expect("应该能清理");
        assert!(!tmp.exists(), ".tmp 应该删掉");
        assert!(!main.exists(), "没有旧包就不该凭空变出一个");
    }

    #[test]
    fn 校验读不出包时会报错而不是放过去() {
        let dir = temp_paks("verify-bad");
        let (_, tmp, _) = pak_paths(&dir);
        fs::write(&tmp, b"not a pak").expect("写坏文件");
        assert!(verify_pak(&tmp, 3).is_err(), "坏文件应该校验不过");
    }
}
