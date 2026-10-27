//! 命令行自测工具：不开界面，直接跟 mod.io 打交道。
//!
//! 用法（在 `app\src-tauri` 目录下跑）：
//!
//! ```text
//! cargo run --bin peek -- me              看当前令牌是谁
//! cargo run --bin peek -- search 钻石      搜 mod
//! cargo run --bin peek -- files 1234      列某个 mod 的文件
//! cargo run --bin peek -- download 1234   下载最新文件
//! cargo run --bin peek -- game            看有没有找到游戏、合并后的 pak 会在哪
//! cargo run --bin peek -- install <文件>   把本地 zip/pak 装进游戏（会真的改游戏目录）
//! ```
//!
//! 令牌从 `<数据目录>\modio-token.txt` 读；用 `ROCK_SPROUT_DATA` 可以把数据目录指到别处。

use rock_sprout::modio::Modio;
use rock_sprout::paths;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("用法：peek <me|search 关键词|files <mod_id>|download <mod_id>|game|install <文件>>");
        std::process::exit(2);
    }

    // 这几个子命令不需要令牌，先处理掉
    match args[0].as_str() {
        "ls" => {
            let Some(path) = args.get(1) else {
                eprintln!("要给一个 pak 路径");
                std::process::exit(2);
            };
            match rock_sprout::paklist::list(std::path::Path::new(path)) {
                Ok(entries) => {
                    for e in &entries {
                        println!("{}", e);
                    }
                }
                Err(e) => {
                    eprintln!("读不了这个 pak：{e}");
                    std::process::exit(1);
                }
            }
            return;
        }
        "game" => {
            match rock_sprout::game::find_installation() {
                Some(inst) => {
                    println!("游戏目录：{}", inst.root.display());
                    println!("主 pak：{}", inst.main_pak().display());
                    println!(
                        "装完会写：{}",
                        inst.paks_path().join("mods_P.pak").display()
                    );
                }
                None => {
                    println!("没找到《深岩银河》的安装位置（只支持 Steam 版）");
                    std::process::exit(1);
                }
            }
            return;
        }
        "install" => {
            let Some(path) = args.get(1) else {
                eprintln!("要给一个本地文件路径（zip 或 pak）");
                std::process::exit(2);
            };
            let item = rock_sprout::game::ModToInstall {
                name: "命令行装的测试 mod".to_owned(),
                url: "https://mod.io/g/drg".to_owned(),
                path: std::path::PathBuf::from(path),
            };
            match rock_sprout::game::apply(&[item]) {
                Ok(p) => println!("装好了：{}", p.display()),
                Err(e) => {
                    eprintln!("失败：{e}");
                    std::process::exit(1);
                }
            }
            return;
        }
        _ => {}
    }

    let client = match Modio::from_token_file(&paths::token_file()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("读令牌失败（{}）：{}", paths::token_file().display(), e.message());
            std::process::exit(1);
        }
    };

    let result = match args[0].as_str() {
        "me" => match client.me().await {
            Ok(me) => {
                println!("已登录：{}（uid = {}）", me.username, me.id);
                Ok(())
            }
            Err(e) => Err(e.message()),
        },
        "search" => {
            let keyword = args.get(1).cloned().unwrap_or_default();
            match client.search(&keyword, 1).await {
                Ok(page) => {
                    println!("关键词「{}」：共 {} 条，本页 {} 条", page.query, page.total, page.items.len());
                    for m in &page.items {
                        println!("  {:>8}  {:<40} 下载 {}", m.id, m.name, m.downloads);
                    }
                    Ok(())
                }
                Err(e) => Err(e.message()),
            }
        }
        "files" | "download" => {
            let Some(id) = args.get(1).and_then(|s| s.parse::<u64>().ok()) else {
                eprintln!("要给一个数字 mod id");
                std::process::exit(2);
            };
            match client.files(id).await {
                Ok(files) => {
                    for f in &files {
                        println!("  {:>8}  {:<40} {} 字节", f.id, f.filename, f.size);
                    }
                    if args[0] == "download" {
                        let Some(latest) = files.first() else {
                            eprintln!("这个 mod 没有文件");
                            std::process::exit(1);
                        };
                        match client.download(latest, &paths::mods_cache_dir()).await {
                            Ok(out) => {
                                println!("已下载：{}（{} 字节）", out.path, out.size);
                                Ok(())
                            }
                            Err(e) => Err(e.message()),
                        }
                    } else {
                        Ok(())
                    }
                }
                Err(e) => Err(e.message()),
            }
        }
        other => {
            eprintln!("不认识的子命令：{other}");
            std::process::exit(2);
        }
    };

    if let Err(msg) = result {
        eprintln!("失败：{msg}");
        std::process::exit(1);
    }
}
