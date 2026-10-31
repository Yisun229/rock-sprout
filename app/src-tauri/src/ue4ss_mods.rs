//! 脚本类 mod（计划 §M7-4）。
//!
//! 有些 mod 不是数据包，而是一段跑在游戏里的代码：包里带 `main.dll` / `main.lua` / `main.js`。
//! 这类东西**不能**打进 `mods_P.pak`——游戏只把 pak 当资源包读，代码它不执行。
//! 它们要落到 `FSD\Binaries\Win64\Mods\<名字>\`，由 UE4SS 在启动时按目录名加载。
//!
//! 三条硬规矩（用户定的）：
//! 1. 同名目录已存在**不覆盖**，报 `exists`，调用方给用户「换个名字」或「替换（先备份）」两个选择；
//! 2. 铺之前核对 `UE4SS-settings.ini` 里指向 mods 的那一项确实是我们铺的 `Mods\`，别被别的 UE4SS 配置带偏；
//! 3. 卸载只删我们登记的那个目录，`Mods\` 里别人的东西一概不动。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ue4ss_install;

/// 只要这几个文件名出现在包里，就当脚本类（§0 第 20 条：按文件名判，不按 `Scripts/` 目录判）。
pub const MARKERS: [&str; 3] = ["main.dll", "main.lua", "main.js"];

/// 放在 mod 目录里就表示「让 UE4SS 加载我」的那个空文件（官方文档里的第二种办法）。
pub const ENABLE_FILE: &str = "enabled.txt";

/// 我们登记过的脚本类 mod。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptMod {
    /// 和已装记账里那个 id 对应
    pub id: u64,
    pub name: String,
    /// `Mods\` 下的目录名（不一定是 name，可能被用户改过）
    pub dir: String,
    pub installed_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Registry {
    #[serde(default)]
    mods: Vec<ScriptMod>,
}

fn registry_file(data_root: &Path) -> PathBuf {
    data_root.join("ue4ss-mods.json")
}

fn load_registry(data_root: &Path) -> Registry {
    std::fs::read_to_string(registry_file(data_root))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_registry(data_root: &Path, reg: &Registry) -> Result<(), Ue4ssError> {
    let text = serde_json::to_string_pretty(reg).map_err(|e| Ue4ssError::io(format!("写登记表失败：{e}")))?;
    if let Some(parent) = registry_file(data_root).parent() {
        std::fs::create_dir_all(parent).map_err(|e| Ue4ssError::io(format!("建目录失败：{e}")))?;
    }
    std::fs::write(registry_file(data_root), text).map_err(|e| Ue4ssError::io(format!("写登记表失败：{e}")))
}

/// 铺/撤脚本 mod 时会遇到的几类失败。
#[derive(Debug)]
pub struct Ue4ssError {
    /// `exists` / `settings` / `io` / `not_installed`
    pub code: &'static str,
    pub message: String,
}

impl Ue4ssError {
    pub fn io(message: impl Into<String>) -> Self {
        Self {
            code: "io",
            message: message.into(),
        }
    }
    fn exists(message: impl Into<String>) -> Self {
        Self {
            code: "exists",
            message: message.into(),
        }
    }
    fn settings(message: impl Into<String>) -> Self {
        Self {
            code: "settings",
            message: message.into(),
        }
    }
}

/// 一个文件名（不含路径）是不是脚本标记。
fn is_marker(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    MARKERS.contains(&lower.as_str())
}

/// 把 zip 里的条目名、或者目录里的相对路径，判成「脚本类」。
///
/// 认的是**文件名本身**：`main.dll`、`main.lua`、`main.js`，放在哪一层都算
/// （有的作者放 `Scripts\main.lua`，也有直接丢根目录的）。
pub fn looks_script<S: AsRef<str>>(paths: impl IntoIterator<Item = S>) -> bool {
    paths.into_iter().any(|p| {
        let name = p
            .as_ref()
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        is_marker(&name)
    })
}

/// 目录（最多两层）里有没有脚本标记。
pub fn dir_looks_script(dir: &Path) -> bool {
    let mut names = Vec::new();
    collect_names(dir, 2, &mut names);
    looks_script(names)
}

fn collect_names(dir: &Path, depth: u32, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                collect_names(&path, depth - 1, out);
            }
        } else if let Some(name) = path.file_name() {
            out.push(name.to_string_lossy().to_string());
        }
    }
}

/// zip 里有没有脚本标记（顺便把条目名都拿出来，给报错信息用）。
pub fn zip_entries(zip: &Path) -> Result<Vec<String>, Ue4ssError> {
    let file = std::fs::File::open(zip).map_err(|e| Ue4ssError::io(format!("打不开这个包：{e}")))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| Ue4ssError::io(format!("这个包解不开：{e}")))?;
    let mut names = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        if let Ok(entry) = archive.by_index(i) {
            names.push(entry.name().to_owned());
        }
    }
    Ok(names)
}

/// 把 zip 解到指定目录。条目里的目录会自动建。
pub fn extract_zip(zip: &Path, dest: &Path) -> Result<(), Ue4ssError> {
    let file = std::fs::File::open(zip).map_err(|e| Ue4ssError::io(format!("打不开这个包：{e}")))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| Ue4ssError::io(format!("这个包解不开：{e}")))?;
    std::fs::create_dir_all(dest).map_err(|e| Ue4ssError::io(format!("建目录失败：{e}")))?;
    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(e) => return Err(Ue4ssError::io(format!("读包里的第 {i} 个条目失败：{e}"))),
        };
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| Ue4ssError::io(format!("建目录失败：{e}")))?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Ue4ssError::io(format!("建目录失败：{e}")))?;
        }
        let mut sink =
            std::fs::File::create(&out).map_err(|e| Ue4ssError::io(format!("写文件失败：{e}")))?;
        std::io::copy(&mut entry, &mut sink).map_err(|e| Ue4ssError::io(format!("解包失败：{e}")))?;
    }
    Ok(())
}

/// `Mods\` 目录（游戏侧的）。
pub fn mods_dir(win64: &Path) -> PathBuf {
    win64.join("Mods")
}

/// 把 zip 解出来的东西里那一层「真正的 mod 目录」找出来。
///
/// 常见两种情况：解出来直接是 `main.lua` 这一堆文件；或者外面还套了一层文件夹
/// （`EnemyWaveTimer\main.dll`）。两种都认，返回应该是「这个 mod 的内容目录」。
pub fn content_root(staged: &Path) -> PathBuf {
    let Ok(entries) = std::fs::read_dir(staged) else {
        return staged.to_path_buf();
    };
    let items: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    let has_marker = items.iter().any(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .map(is_marker)
            .unwrap_or(false)
    });
    if has_marker {
        return staged.to_path_buf();
    }
    // 只有一层目录、里面才有标记 → 用那一层
    let dirs: Vec<&PathBuf> = items.iter().filter(|p| p.is_dir()).collect();
    if items.len() == 1 && dirs.len() == 1 {
        return dirs[0].clone();
    }
    staged.to_path_buf()
}

/// 核对 `UE4SS-settings.ini` 里指向 mods 的那一项确实是我们铺的 `Mods\`。
///
/// 3.0.1 的 ini 里这一项叫 `ModsDirectory`（也见过 `ModsDir`），值是相对游戏目录的路径，
/// 常见写法是 `Mods` 或 `..\Mods`。这里只做一件事：读出来、归一化、跟我们的目标比。
/// 读不到这一项就当通过（官方默认就是 `Mods`），读到但指向别处就报 `settings`。
pub fn check_settings(win64: &Path) -> Result<(), Ue4ssError> {
    let ini = win64.join("UE4SS-settings.ini");
    let Ok(text) = std::fs::read_to_string(&ini) else {
        return Ok(());
    };
    let mut value = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        let Some((key, rest)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        if key == "modsdirectory" || key == "modsdir" {
            value = Some(rest.trim().to_owned());
            break;
        }
    }
    let Some(raw) = value else {
        return Ok(());
    };
    // 只看「最后一段是不是 Mods」不够：`D:\别的工具\Mods` 也是 Mods。
    // 所以把这一项按 ini 的语义解析成真实路径：绝对路径原样，相对路径接在游戏目录后面。
    let cleaned = raw.trim_matches('"').replace('/', "\\");
    let absolute = cleaned.len() >= 2 && cleaned.as_bytes()[1] == b':';
    let resolved = if absolute {
        PathBuf::from(&cleaned)
    } else {
        let mut out = win64.to_path_buf();
        for part in cleaned.split('\\') {
            match part {
                "" | "." => {}
                ".." => {
                    out.pop();
                }
                other => out.push(other),
            }
        }
        out
    };
    let wanted = win64.join("Mods");
    let same = resolved
        .to_string_lossy()
        .trim_end_matches('\\')
        .eq_ignore_ascii_case(wanted.to_string_lossy().trim_end_matches('\\'));
    if same {
        return Ok(());
    }
    Err(Ue4ssError::settings(format!(
        "游戏里那份 UE4SS-settings.ini 把 mods 目录指到了「{raw}」（算下来是 {}），不是我们铺的 {}。\
         先把这份配置换回官方的（设置页 → UE4SS → 装到游戏里），再来装脚本 mod。",
        resolved.display(),
        wanted.display()
    )))
}

/// 铺一个脚本 mod。
///
/// `replace` 为假时，同名目录已存在就报 `exists`，一个字节都不动；
/// 为真时先把旧目录整个挪到 `<数据目录>\backup\ue4ss-mods\<目录名>-<毫秒>\`，再铺新的。
pub fn install(
    game_win64: &Path,
    data_root: &Path,
    id: u64,
    dir_name: &str,
    content: &Path,
    replace: bool,
) -> Result<ScriptMod, Ue4ssError> {
    check_settings(game_win64)?;

    let target = mods_dir(game_win64).join(dir_name);
    if target.exists() {
        if !replace {
            return Err(Ue4ssError::exists(format!(
                "Mods 里已经有一个叫「{dir_name}」的目录了。可以换个名字装，或者替换掉（替换前会先备份）。"
            )));
        }
        let stamp = now_ms();
        let backup = data_root
            .join("backup")
            .join("ue4ss-mods")
            .join(format!("{dir_name}-{stamp}"));
        move_tree(&target, &backup)?;
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Ue4ssError::io(format!("建目录失败：{e}")))?;
    }
    copy_tree(content, &target)?;
    arrange_for_ue4ss(&target)?;

    let entry = ScriptMod {
        id,
        name: dir_name.to_owned(),
        dir: dir_name.to_owned(),
        installed_at: now_ms(),
    };
    let mut reg = load_registry(data_root);
    reg.mods.retain(|m| m.id != id && m.dir != dir_name);
    reg.mods.push(entry.clone());
    save_registry(data_root, &reg)?;
    Ok(entry)
}

/// 撤掉一个脚本 mod：只删登记过的那一个目录。
pub fn uninstall(data_root: &Path, id: u64) -> Result<Option<ScriptMod>, Ue4ssError> {
    let mut reg = load_registry(data_root);
    let Some(entry) = reg.mods.iter().find(|m| m.id == id).cloned() else {
        return Ok(None);
    };
    // 只删登记表里的目录名，不接受外面传进来的路径
    let Some(game) = crate::game::find_installation() else {
        return Err(Ue4ssError::io("没找到游戏目录，没法撤".to_owned()));
    };
    let target = mods_dir(&game.binaries_directory()).join(&entry.dir);
    if target.exists() {
        std::fs::remove_dir_all(&target).map_err(|e| Ue4ssError::io(format!("删目录失败：{e}")))?;
    }
    reg.mods.retain(|m| m.id != id);
    save_registry(data_root, &reg)?;
    Ok(Some(entry))
}

/// 按 UE4SS 的约定把解出来的东西摆一遍。
///
/// 官方文档（`docs/guides/installing-a-c++-mod.md`）写得很死：C++ mod 的 dll 要放在
/// `Mods\<名字>\dlls\main.dll`，并且要在 `Mods\mods.txt` 里登记；另一个官方办法是在 mod
/// 目录里放一个空的 `enabled.txt`（文档说不推荐、但没有顺序要求的问题，好处是不用去改
/// 官方那份 `mods.txt`，卸载时删目录就干净了）。这里走 `enabled.txt`。
///
/// 从 mod.io 下下来的脚本 mod 往往是平铺的 `main.dll`（那是别的启动器的约定），
/// 平铺的 UE4SS 3.0.1 不认，所以这里统一搬进 `dlls\`。
fn arrange_for_ue4ss(target: &Path) -> Result<(), Ue4ssError> {
    let flat = target.join("main.dll");
    if flat.is_file() {
        let dlls = target.join("dlls");
        std::fs::create_dir_all(&dlls).map_err(|e| Ue4ssError::io(format!("建 dlls 目录失败：{e}")))?;
        std::fs::rename(&flat, dlls.join("main.dll"))
            .map_err(|e| Ue4ssError::io(format!("把 main.dll 挪进 dlls 失败：{e}")))?;
    }
    let has_dll = target.join("dlls").join("main.dll").is_file();
    let has_lua =
        target.join("Scripts").join("main.lua").is_file() || target.join("Scripts").join("main.js").is_file();
    if (has_dll || has_lua) && !target.join(ENABLE_FILE).exists() {
        std::fs::write(target.join(ENABLE_FILE), "")
            .map_err(|e| Ue4ssError::io(format!("写 enabled.txt 失败：{e}")))?;
    }
    Ok(())
}

/// 某个 id 登记过没有。
pub fn registered(data_root: &Path, id: u64) -> Option<ScriptMod> {
    load_registry(data_root).mods.into_iter().find(|m| m.id == id)
}

/// 把目录名变成系统能接受的样子（去掉路径分隔符和 Windows 不认的字符）。
pub fn safe_dir_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.').to_owned();
    if trimmed.is_empty() {
        "script-mod".to_owned()
    } else {
        trimmed
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn copy_tree(src: &Path, dst: &Path) -> Result<(), Ue4ssError> {
    ue4ss_install::copy_into(src, dst).map_err(|e| Ue4ssError::io(e.message))
}

fn move_tree(src: &Path, dst: &Path) -> Result<(), Ue4ssError> {
    copy_tree(src, dst)?;
    std::fs::remove_dir_all(src).map_err(|e| Ue4ssError::io(format!("挪走旧目录失败：{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let base = std::env::var("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("target"));
        let dir = base.join("rock-sprout-tests").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建测试目录");
        dir
    }

    #[test]
    fn 有main_lua就算脚本类() {
        assert!(looks_script(["Scripts/main.lua"]));
        assert!(looks_script(["main.dll"]));
        assert!(looks_script(["deep/er/main.js"]));
    }

    #[test]
    fn 只有pak就不算脚本类() {
        assert!(!looks_script(["EnemyWaveTimer.pak"]));
        assert!(!looks_script(["readme.txt", "Mods/x/main.lua.bak"]));
    }

    #[test]
    fn 大小写都认() {
        assert!(looks_script(["MAIN.DLL"]));
        assert!(looks_script(["Main.Lua"]));
    }

    #[test]
    fn 目录里那一层套着也能认出来() {
        let dir = test_dir("script-detect");
        let inner = dir.join("EnemyWaveTimer");
        std::fs::create_dir_all(&inner).expect("建目录");
        std::fs::write(inner.join("main.dll"), b"x").expect("写文件");
        assert!(dir_looks_script(&dir));
    }

    #[test]
    fn 解出来的内容目录会剥掉外面那层() {
        let dir = test_dir("script-root");
        let inner = dir.join("EnemyWaveTimer");
        std::fs::create_dir_all(&inner).expect("建目录");
        std::fs::write(inner.join("main.dll"), b"x").expect("写文件");
        assert_eq!(content_root(&dir), inner);
    }

    #[test]
    fn 直接解在根上就不剥() {
        let dir = test_dir("script-root-flat");
        std::fs::write(dir.join("main.lua"), b"x").expect("写文件");
        assert_eq!(content_root(&dir), dir);
    }

    #[test]
    fn 目录名里的非法字符会被换掉() {
        assert_eq!(safe_dir_name("a/b:c*d"), "a_b_c_d");
        assert_eq!(safe_dir_name("   "), "script-mod");
    }

    #[test]
    fn 配置里指向别处就报错() {
        let dir = test_dir("script-settings-bad");
        std::fs::write(
            dir.join("UE4SS-settings.ini"),
            "[Overrides]\nModsDirectory = D:\\Other\\Mods\n",
        )
        .expect("写 ini");
        let err = check_settings(&dir).expect_err("应该报错");
        assert_eq!(err.code, "settings");
    }

    #[test]
    fn 配置里写Mods或干脆没这项都放过() {
        let dir = test_dir("script-settings-ok");
        std::fs::write(dir.join("UE4SS-settings.ini"), "[Overrides]\nModsDirectory = Mods\n")
            .expect("写 ini");
        assert!(check_settings(&dir).is_ok());

        let dir2 = test_dir("script-settings-none");
        std::fs::write(dir2.join("UE4SS-settings.ini"), "[General]\nFoo = 1\n").expect("写 ini");
        assert!(check_settings(&dir2).is_ok());
    }

    #[test]
    fn 同名目录存在时没确认就一个字节都不动() {
        let data = test_dir("script-install-exists-data");
        let game = test_dir("script-install-exists-game");
        let staged = test_dir("script-install-exists-staged");
        std::fs::write(staged.join("main.lua"), b"new").expect("写文件");
        std::fs::create_dir_all(mods_dir(&game).join("Same")).expect("建目录");
        std::fs::write(mods_dir(&game).join("Same").join("main.lua"), b"old").expect("写旧文件");

        let err = install(&game, &data, 7, "Same", &staged, false).expect_err("应该拒绝");
        assert_eq!(err.code, "exists");
        let kept = std::fs::read_to_string(mods_dir(&game).join("Same").join("main.lua")).expect("读");
        assert_eq!(kept, "old", "原来那个不能被改");
    }

    #[test]
    fn 确认替换会先备份再铺新的() {
        let data = test_dir("script-install-replace-data");
        let game = test_dir("script-install-replace-game");
        let staged = test_dir("script-install-replace-staged");
        std::fs::write(staged.join("main.lua"), b"new").expect("写文件");
        std::fs::create_dir_all(mods_dir(&game).join("Same")).expect("建目录");
        std::fs::write(mods_dir(&game).join("Same").join("main.lua"), b"old").expect("写旧文件");

        let entry = install(&game, &data, 7, "Same", &staged, true).expect("应该成功");
        assert_eq!(entry.dir, "Same");
        let now = std::fs::read_to_string(mods_dir(&game).join("Same").join("main.lua")).expect("读");
        assert_eq!(now, "new");
        let backups: Vec<_> = std::fs::read_dir(data.join("backup/ue4ss-mods"))
            .expect("读备份目录")
            .flatten()
            .collect();
        assert_eq!(backups.len(), 1, "旧的那份应该在备份里");
    }

    #[test]
    fn 平铺的main_dll会被摆进dlls并写enabled_txt() {
        let data = test_dir("script-cpp-layout-data");
        let game = test_dir("script-cpp-layout-game");
        let staged = test_dir("script-cpp-layout-staged");
        std::fs::write(staged.join("main.dll"), b"dll").expect("写 dll");
        std::fs::write(staged.join("EnemyWaveTimer.pak"), b"pak").expect("写 pak");

        install(&game, &data, 11, "EnemyWaveTimer", &staged, false).expect("装");

        let dir = mods_dir(&game).join("EnemyWaveTimer");
        assert!(dir.join("dlls").join("main.dll").is_file(), "dll 要在 dlls 下面");
        assert!(!dir.join("main.dll").exists(), "根上不该再留一个");
        assert!(dir.join(ENABLE_FILE).is_file(), "要放 enabled.txt，不然 UE4SS 不加载");
        assert!(dir.join("EnemyWaveTimer.pak").is_file(), "pak 留在 mod 目录根上");
    }

    #[test]
    fn lua类也算它一份enabled_txt() {
        let data = test_dir("script-lua-layout-data");
        let game = test_dir("script-lua-layout-game");
        let staged = test_dir("script-lua-layout-staged");
        std::fs::create_dir_all(staged.join("Scripts")).expect("建目录");
        std::fs::write(staged.join("Scripts").join("main.lua"), b"--").expect("写");

        install(&game, &data, 12, "SomeLua", &staged, false).expect("装");
        let dir = mods_dir(&game).join("SomeLua");
        assert!(dir.join("Scripts").join("main.lua").is_file());
        assert!(dir.join(ENABLE_FILE).is_file());
        assert!(!dir.join("dlls").exists(), "lua 类不需要 dlls 目录");
    }

    #[test]
    fn 撤掉时只删登记过的那个目录() {
        let data = test_dir("script-uninstall-data");
        let game = test_dir("script-uninstall-game");
        let staged = test_dir("script-uninstall-staged");
        std::fs::write(staged.join("main.lua"), b"x").expect("写文件");
        install(&game, &data, 9, "Mine", &staged, false).expect("装");
        std::fs::create_dir_all(mods_dir(&game).join("Other")).expect("建别人的目录");
        std::fs::write(mods_dir(&game).join("Other").join("main.lua"), b"y").expect("写");

        // uninstall 走的是真实游戏目录，这里只验登记表与目录名的对应关系
        let entry = registered(&data, 9).expect("登记过");
        assert_eq!(entry.dir, "Mine");
        let mut reg = load_registry(&data);
        reg.mods.retain(|m| m.id != 9);
        save_registry(&data, &reg).expect("存");
        assert!(registered(&data, 9).is_none());
        assert!(mods_dir(&game).join("Other").join("main.lua").exists(), "别人的东西不能动");
    }
}
