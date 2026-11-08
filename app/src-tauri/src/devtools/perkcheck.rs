//! 拿一份存档验算天赋点数三式：已赚 − 已花 == 还剩。
//!
//!   cargo run --release --features devtools --bin perkcheck -- <存档.sav>
//!
//! 只读：直接读原档字节，不复制、不写缓存、不碰游戏目录。

use std::fs;
use std::process::ExitCode;

use rock_sprout::ipc::perks::points_check;
use rock_sprout::save::{gvas, model};

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("用法：perkcheck <存档.sav>");
        return ExitCode::from(2);
    };
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("读不了 {path}：{e}");
            return ExitCode::from(2);
        }
    };
    let parsed = match gvas::parse(&bytes) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("解析失败：{e:?}");
            return ExitCode::from(1);
        }
    };
    let data = model::build(&parsed.props);
    let claimed = &data.Progress.Milestones;
    let owned = &data.Perks.OwnedPerks;
    let check = points_check(data.Perks.PerkPoints, claimed, owned);
    println!(
        "{}",
        serde_json::json!({
            "save": path,
            "claimed": claimed.len(),
            "owned": owned.len(),
            "earned": check.earned,
            "spent": check.spent,
            "left": check.left,
            "verified": check.verified,
        })
    );
    if check.verified {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
