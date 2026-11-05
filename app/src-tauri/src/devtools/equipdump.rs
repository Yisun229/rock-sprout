//! 把「装备」页的数据原样打出来（只读，不改任何东西）。
//!
//! 用法：
//! ```text
//! ROCK_SPROUT_SAVE_DIR=<某份存档所在目录> cargo run --release --features devtools --bin equipdump
//! ```

use std::process::ExitCode;

use rock_sprout::ipc::equipment::equipment_view;

fn main() -> ExitCode {
    let api = equipment_view(None);
    if !api.ok {
        let err = api
            .error
            .map(|e| format!("{} {}", e.code, e.message))
            .unwrap_or_default();
        println!("失败：{err}");
        return ExitCode::FAILURE;
    }
    let data = api.data.expect("ok 时一定有 data");
    println!(
        "state={} file={:?} cache={} 角色块={}",
        data.state,
        data.file_name,
        data.from_cache,
        data.characters.len()
    );

    let mut known = 0usize;
    let mut unknown = 0usize;
    let mut empty = 0usize;
    for c in &data.characters {
        println!(
            "  槽 {}: 职业={:?} 配装{}套",
            c.slot,
            c.class,
            c.loadouts.len()
        );
        for set in &c.loadouts {
            let line: Vec<String> = set
                .items
                .iter()
                .map(|i| {
                    if i.id.is_empty() {
                        empty += 1;
                        format!("{}:空", i.key)
                    } else if i.known {
                        known += 1;
                        format!("{}={}", i.key, i.name)
                    } else {
                        unknown += 1;
                        format!("{}=#{}", i.key, &i.id[..i.id.len().min(8)])
                    }
                })
                .collect();
            println!("    配装 {}: {}", set.index, line.join(" | "));
        }
    }
    println!("合计：有名 {known}、没名字 {unknown}、空格 {empty}");
    ExitCode::SUCCESS
}
