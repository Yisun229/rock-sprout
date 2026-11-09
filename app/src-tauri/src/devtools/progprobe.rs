//! 打印一份存档里的赛季挑战与深潜字段，供 M0 核对 StatIndices 下标顺序。
//!
//!   cargo run --release --features devtools --bin progprobe -- <存档.sav>
//!
//! 只读：直接读原档字节，不复制、不写缓存、不碰游戏目录。

use std::fs;
use std::process::ExitCode;

use rock_sprout::save::{gvas, model};

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("用法：progprobe <存档.sav>");
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
    let seasons: Vec<_> = data
        .Progress
        .Seasons
        .iter()
        .map(|s| {
            serde_json::json!({
                "SeasonID": s.SeasonID,
                "XP": s.XP,
                "Tokens": s.Tokens,
                "Rewards": s.Rewards.len(),
                "RewardsClaimedFlags": s.Rewards.iter().filter(|r| r.normal).count(),
                "NodesBought": s.NodesBought,
                "challenges": s.ActiveChallenges.iter().map(|c| serde_json::json!({
                    "ChallengeGuid": c.ChallengeGuid,
                    "StatIndices": c.StatIndices,
                    "CurrentWins": c.CurrentWins,
                    "NumberOfWins": c.NumberOfWins,
                    "XPCompleteGain": c.XPCompleteGain,
                    "TokenGain": c.TokenGain,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    println!(
        "{}",
        serde_json::json!({
            "save": path,
            "seasons": seasons,
            "campaigns": data.Progress.CampaignSave.CompletedCampaigns,
            "zones": data.Progress.UnLockedPlanetZones,
            "missions": data.Progress.UnLockedMissions,
            "diveNormal": format!("{:?}", data.Progress.DeepDiveSave.NormalSave),
            "diveElite": format!("{:?}", data.Progress.DeepDiveSave.EliteSave),
        })
    );
    ExitCode::SUCCESS
}
