//! 从 mint 搬来的「游戏装在哪、pak 叫什么」这一层。
//!
//! 来源：https://github.com/trumank/mint 提交 57aa964e7789f132481813a8f4b5781f924eccd1，
//! 许可证 MIT（见同目录上层 `LICENSE-MINT`）。
//!
//! 这是一个独立的小 crate，因为游戏里的加载器 DLL（`hook`）也要用它。
//! 与原始文件的差别：去掉了 mint 自己的版本信息（`built_info`）和自更新模块（`update`），
//! 日志初始化 `setup_logging` 保留下来（加载器 DLL 里要用）。

pub mod error;
pub mod mod_info;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

#[derive(Debug)]
pub enum DRGInstallationType {
    Steam,
    Xbox,
}

impl DRGInstallationType {
    /// 加载器 DLL 在游戏进程里跑的时候，靠进程名判断是哪个版本。
    pub fn from_exe_path() -> Result<Self> {
        let exe_name = std::env::current_exe()
            .context("could not determine running exe")?
            .file_name()
            .context("failed to get exe path")?
            .to_string_lossy()
            .to_lowercase();
        Ok(match exe_name.as_str() {
            "fsd-win64-shipping.exe" => Self::Steam,
            "fsd-wingdk-shipping.exe" => Self::Xbox,
            _ => bail!("unrecognized exe file name: {exe_name}"),
        })
    }
}

impl DRGInstallationType {
    pub fn from_pak_path<P: AsRef<Path>>(pak: P) -> Result<Self> {
        let pak_name = pak
            .as_ref()
            .file_name()
            .context("failed to get pak file name")?
            .to_string_lossy()
            .to_lowercase();
        Ok(match pak_name.as_str() {
            "fsd-windowsnoeditor.pak" => Self::Steam,
            "fsd-wingdk.pak" => Self::Xbox,
            _ => bail!("unrecognized pak file name: {pak_name}"),
        })
    }
    pub fn binaries_directory_name(&self) -> &'static str {
        match self {
            Self::Steam => "Win64",
            Self::Xbox => "WinGDK",
        }
    }
    pub fn main_pak_name(&self) -> &'static str {
        match self {
            Self::Steam => "FSD-WindowsNoEditor.pak",
            Self::Xbox => "FSD-WinGDK.pak",
        }
    }
    pub fn hook_dll_name(&self) -> &'static str {
        match self {
            Self::Steam => "x3daudio1_7.dll",
            Self::Xbox => "d3d9.dll",
        }
    }
}

#[derive(Debug)]
pub struct DRGInstallation {
    pub root: PathBuf,
    pub installation_type: DRGInstallationType,
}

impl DRGInstallation {
    /// 只支持 Steam 版：从 Steam 的库列表里找 appid 548430。
    pub fn find() -> Option<Self> {
        steamlocate::SteamDir::locate()
            .ok()
            .and_then(|steamdir| {
                steamdir
                    .find_app(548430)
                    .ok()
                    .flatten()
                    .map(|(app, library)| {
                        library
                            .resolve_app_dir(&app)
                            .join("FSD/Content/Paks/FSD-WindowsNoEditor.pak")
                    })
            })
            .and_then(|path| Self::from_pak_path(path).ok())
    }
    pub fn from_pak_path<P: AsRef<Path>>(pak: P) -> Result<Self> {
        let root = pak
            .as_ref()
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .context("failed to get pak parent directory")?
            .to_path_buf();
        Ok(Self {
            root,
            installation_type: DRGInstallationType::from_pak_path(pak)?,
        })
    }
    pub fn binaries_directory(&self) -> PathBuf {
        self.root
            .join("Binaries")
            .join(self.installation_type.binaries_directory_name())
    }
    pub fn paks_path(&self) -> PathBuf {
        self.root.join("Content").join("Paks")
    }
    pub fn main_pak(&self) -> PathBuf {
        self.paks_path()
            .join(self.installation_type.main_pak_name())
    }
    /// 游戏自己的 mod.io 缓存目录（官方整合模组会读这里）。
    pub fn modio_directory(&self) -> Option<PathBuf> {
        match self.installation_type {
            DRGInstallationType::Steam => {
                #[cfg(target_os = "windows")]
                {
                    Some(PathBuf::from("C:\\Users\\Public\\mod.io\\2475"))
                }
                #[cfg(target_os = "linux")]
                {
                    steamlocate::SteamDir::locate()
                        .map(|s| {
                            s.path().join(
                                "steamapps/compatdata/548430/pfx/drive_c/users/Public/mod.io/2475",
                            )
                        })
                        .ok()
                }
                #[cfg(not(any(target_os = "windows", target_os = "linux")))]
                {
                    None
                }
            }
            DRGInstallationType::Xbox => None,
        }
    }
}

/// 让日志同时写文件和控制台。加载器 DLL 里用它把日志写到游戏目录。
///
/// 返回的 guard 必须一直活着，否则文件日志会断。
pub fn setup_logging<P: AsRef<Path>>(
    log_path: P,
    target: &str,
) -> Result<tracing_appender::non_blocking::WorkerGuard> {
    use tracing::metadata::LevelFilter;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{
        EnvFilter,
        field::RecordFields,
        filter,
        fmt::{
            self, FormatFields,
            format::{Pretty, Writer},
        },
    };

    /// 绕开 tracing 的一个已知问题（tokio-rs/tracing#1817）。
    struct NewType(Pretty);

    impl<'writer> FormatFields<'writer> for NewType {
        fn format_fields<R: RecordFields>(
            &self,
            writer: Writer<'writer>,
            fields: R,
        ) -> core::fmt::Result {
            self.0.format_fields(writer, fields)
        }
    }

    let f = fs_err::File::create(log_path.as_ref())?;
    let writer = std::io::BufWriter::new(f);
    let (log_file_appender, guard) = tracing_appender::non_blocking(writer);
    let debug_file_log = fmt::layer()
        .with_writer(log_file_appender)
        .fmt_fields(NewType(Pretty::default()))
        .with_ansi(false)
        .with_filter(filter::Targets::new().with_target(target, tracing::Level::DEBUG));
    let stderr_log = fmt::layer()
        .with_writer(std::io::stderr)
        .event_format(tracing_subscriber::fmt::format().without_time())
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .with_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        );
    let subscriber = tracing_subscriber::registry()
        .with(stderr_log)
        .with(debug_file_log);

    tracing::subscriber::set_global_default(subscriber)?;

    tracing::debug!("tracing subscriber setup");
    tracing::info!("writing logs to {:?}", log_path.as_ref().display());

    Ok(guard)
}
