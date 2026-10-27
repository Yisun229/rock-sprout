//! 把下载好的 mod 真正装进游戏——调从 mint 搬来的整合逻辑，合并成一个 pak。
//!
//! 这一层的职责只有三件事：找到游戏装在哪、把我们的记录翻译成 mint 的数据结构、
//! 在后台线程里跑那个又吃 CPU 又吃内存的合并过程。

use std::path::{Path, PathBuf};

use crate::integrate;
use mint_lib::DRGInstallation;
use mint_lib::mod_info::{
    MetaConfig, ModIdentifier, ModInfo, ModResolution, ModSpecification,
};

/// 一个等待装进游戏的 mod：显示名、来源网址（mod.io 的页面）、本地文件（zip 或 pak）。
#[derive(Debug, Clone)]
pub struct ModToInstall {
    pub name: String,
    pub url: String,
    pub path: PathBuf,
}

/// 加载器 DLL 在游戏目录里的位置（Steam 版是 `Binaries\Win64\x3daudio1_7.dll`）。
fn hook_dll_path(installation: &DRGInstallation) -> PathBuf {
    installation
        .binaries_directory()
        .join(installation.installation_type.hook_dll_name())
}

/// 游戏自带那个同名 DLL 的备份位置（在我们的数据目录里，不放进游戏目录）。
fn stock_dll_backup_path(installation: &DRGInstallation) -> PathBuf {
    crate::paths::data_root()
        .join("game-backup")
        .join(installation.installation_type.hook_dll_name())
}

/// 这个 DLL 是不是我们自己写的？按大小判断就够了。
fn is_our_dll(path: &Path) -> bool {
    let Some(len) = integrate::hook_dll_len() else {
        return false;
    };
    std::fs::metadata(path)
        .map(|m| m.len() as usize == len)
        .unwrap_or(false)
}

/// 覆盖之前先把游戏自带的同名 DLL 存一份——卸载时要原样还回去。
///
/// 那个 `x3daudio1_7.dll` 是游戏自己的音频库，我们的加载器只是借它的名字做转发，
/// 所以「覆盖」可以，「删掉」不行。
fn back_up_stock_dll(installation: &DRGInstallation) -> Result<(), String> {
    let dll = hook_dll_path(installation);
    if !dll.exists() || is_our_dll(&dll) {
        return Ok(());
    }

    let dest = stock_dll_backup_path(installation);
    if dest.exists() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("建备份目录失败：{e}"))?;
    }
    std::fs::copy(&dll, &dest).map_err(|e| format!("备份游戏自带的 DLL 失败：{e}"))?;
    tracing::info!(from = %dll.display(), to = %dest.display(), "已备份游戏自带的同名 DLL");
    Ok(())
}

/// 找游戏装在哪。目前只认 Steam 版；找不到就返回 `None`。
pub fn find_installation() -> Option<DRGInstallation> {
    DRGInstallation::find()
}

/// 合并后那个 pak 的完整路径（`<游戏>\FSD\Content\Paks\mods_P.pak`）。
pub fn installed_pak_path() -> Result<PathBuf, String> {
    let installation = find_installation().ok_or_else(|| {
        "没找到《深岩银河》的安装位置。目前只支持 Steam 版，装在其他盘也能找到".to_owned()
    })?;
    Ok(installation.paks_path().join("mods_P.pak"))
}

/// 把这一批 mod 合并进游戏的 `mods_P.pak`。传空列表就是「一个都不装」。
///
/// 注意：这是**同步**的重活（要读一个 2.5 GB 的包、解压改资源、再写出去），
/// 界面上要放到后台线程里调，别卡住窗口。
pub fn apply(mods: &[ModToInstall]) -> Result<PathBuf, String> {
    let installation = find_installation().ok_or_else(|| {
        "没找到《深岩银河》的安装位置。目前只支持 Steam 版，装在其他盘也能找到".to_owned()
    })?;
    let main_pak = installation.main_pak();
    if !main_pak.exists() {
        return Err(format!(
            "游戏的主 pak 不在预期位置：{}",
            main_pak.display()
        ));
    }

    let list: Vec<(ModInfo, PathBuf)> = mods
        .iter()
        .map(|m| {
            let spec = ModSpecification::new(m.url.clone());
            (
                ModInfo {
                    provider: "modio",
                    name: m.name.clone(),
                    spec: spec.clone(),
                    versions: vec![spec.clone()],
                    resolution: ModResolution::resolvable(ModIdentifier::new(m.url.clone())),
                    suggested_require: false,
                    suggested_dependencies: Vec::new(),
                    modio_tags: None,
                    modio_id: None,
                },
                m.path.clone(),
            )
        })
        .collect();

    tracing::info!(count = list.len(), "开始合并 mod");
    // 覆盖加载器 DLL 之前，先留一份游戏自带的同名 DLL
    back_up_stock_dll(&installation)?;
    integrate::integrate(&main_pak, MetaConfig {}, list)
        .map_err(|e| format!("装进游戏时出错：{e}"))?;
    let out = installation.paks_path().join("mods_P.pak");
    tracing::info!(path = %out.display(), "合并完成");
    Ok(out)
}

/// 给「诊断信息」用的几句话：游戏在哪、合并包在不在、加载器是哪一个。
///
/// 不看网络、不读令牌内容，断网时也能用。
pub fn status_lines() -> Vec<String> {
    let Some(installation) = find_installation() else {
        return vec!["游戏：没找到（目前只支持 Steam 版）".to_owned()];
    };

    let mut out = Vec::new();
    out.push(format!("游戏目录：{}", installation.root.display()));

    let main_pak = installation.main_pak();
    out.push(format!(
        "游戏主 pak：{}（{}）",
        main_pak.display(),
        if main_pak.exists() { "在" } else { "不在" }
    ));

    let pak = installation.paks_path().join("mods_P.pak");
    out.push(match std::fs::metadata(&pak) {
        Ok(m) => format!("合并包：有，{} 字节", m.len()),
        Err(_) => "合并包：没有".to_owned(),
    });

    let dll = hook_dll_path(&installation);
    out.push(if !dll.exists() {
        format!("加载器 DLL：不在（{}）", dll.display())
    } else if is_our_dll(&dll) {
        format!("加载器 DLL：是岩笋写的（{}）", dll.display())
    } else {
        format!("加载器 DLL：游戏原版（{}）", dll.display())
    });

    out
}

/// 一个都不装：删掉我们写的合并包，把加载器 DLL 换回游戏自带的那个。
///
/// 只动我们自己写的东西：合并包直接删；DLL 优先还原备份。
/// **备份不在时不动那个 DLL**——宁可把我们的加载器留着（游戏照常能启动），
/// 也不要凭空删掉游戏目录里的文件：那是游戏自己的音频库。
pub fn clear() -> Result<(), String> {
    let Some(installation) = find_installation() else {
        // 找不到游戏就没什么可清的
        return Ok(());
    };

    let pak = installation.paks_path().join("mods_P.pak");
    if pak.exists() {
        std::fs::remove_file(&pak).map_err(|e| format!("删合并包失败：{e}"))?;
        tracing::info!(path = %pak.display(), "已删除合并包");
    }

    let dll = hook_dll_path(&installation);
    let backup = stock_dll_backup_path(&installation);
    if backup.exists() {
        std::fs::copy(&backup, &dll).map_err(|e| format!("还原游戏自带的 DLL 失败：{e}"))?;
        tracing::info!(path = %dll.display(), "已把加载器换回游戏自带的 DLL");
    } else if dll.exists() {
        tracing::warn!(
            path = %dll.display(),
            "没有游戏自带 DLL 的备份，这次不动它（下次装 mod 时会自动备份）"
        );
    }

    Ok(())
}
