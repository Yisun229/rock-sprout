//! 把「我的矿工」命令的结果原样打出来，用来对数字（不改任何东西）。
//!
//! 用法：
//! ```text
//! ROCK_SPROUT_SAVE_DIR=<某份存档所在目录> cargo run --release --features devtools --bin dwarfdump
//! ```
//!
//! 也可以把 `ROCK_SPROUT_SAVE_DIR` 指向 `_tools/save-samples` 跑全部样本所在目录里的最新一份。

use std::process::ExitCode;

use rock_sprout::ipc::dwarf::my_dwarf_view;

fn main() -> ExitCode {
    match my_dwarf_view(None) {
        api if api.ok => {
            let data = api.data.expect("ok 时一定有 data");
            println!("state={} file={:?} cache={}", data.state, data.file_name, data.from_cache);
            println!("curve_ok={} 里程碑 已领 {}/{}", data.curve_ok, data.milestones_claimed, data.milestones_total);
            for c in &data.characters {
                println!(
                    "  槽 {}: XP={} 等级={:?} 退休={} 退休等级={} 有进度={} 职业={:?} 依据={:?}",
                    c.slot, c.xp, c.level, c.times_retired, c.retired_levels, c.has_progress, c.class, c.basis
                );
            }
            for m in data.milestones.iter().take(6) {
                println!("  里程碑 {} tier={} 上限={} 名字={:?}", &m.id[..8], m.tier, m.max_tier, m.name);
            }
            let named = data.milestones.iter().filter(|m| m.name.is_some()).count();
            println!("  里程碑有名字 {}/{}", named, data.milestones.len());
            ExitCode::SUCCESS
        }
        api => {
            let err = api.error.map(|e| format!("{} {}", e.code, e.message)).unwrap_or_default();
            println!("失败：{err}");
            ExitCode::FAILURE
        }
    }
}
