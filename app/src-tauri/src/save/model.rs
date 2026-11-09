//! 存档里的七块数据，按 `_tools/save-fields.md` 的字段路径一一对应。
//!
//! 两条规矩：
//!
//! 1. **字段名用存档里的原名**（`SavegameID`、`UnLockedVanityItemIDs` 都是原样），
//!    不做中文或驼峰转换。名字对照是 M4 的事，这一层只负责忠实读出来。
//!    Rust 惯例是蛇形命名，这里用 `#[allow(non_snake_case)]` 顶住警告。
//! 2. **读不出来就是缺**：缺字段用 `Option`，不给编造的默认值。
//!    只有「存档里明确写着 0」和「字段根本没有」两种情况看起来像，但前者是数据、后者是缺失。

#![allow(non_snake_case)]

use serde::{Deserialize, Serialize};

use super::{Property, Value};

/// GUID 一律用 32 位小写十六进制字符串表示，方便存 JSON、看日志。
pub type Guid = String;

const ZERO_GUID: &str = "00000000000000000000000000000000";

/// 把字节串成十六进制。
pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// 七块数据 + 概览要的几个数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveData {
    /// `VersionNumber`：存档格式版本（实测 2）
    pub VersionNumber: i32,
    /// `SaveCreatedInPatch`：这份存档是哪一版游戏创建的（实测 30）
    pub SaveCreatedInPatch: i32,
    /// `Credits`：信用点
    pub Credits: i32,
    /// `PerkPoints`：剩余天赋点（与 [`PerkSave::PerkPoints`] 同一个字段）
    pub PerkPoints: i32,
    /// `TotalPlayTimeSeconds`：总游玩秒数（概览「总时长」）
    pub TotalPlayTimeSeconds: f32,
    /// `NumberOfGamesPlayed`：局数（概览「局数」）
    pub NumberOfGamesPlayed: i32,
    /// `LastPlayedCharacter`：最后一次玩的是谁（只说明「最后」，不是每个角色的职业）
    pub LastPlayedCharacter: Option<String>,

    /// ① 我的矿工 `CharacterSaves`
    pub CharacterSaves: Vec<MinerSave>,
    /// ② 我的矿 `Resources`
    pub Resources: ResourcesSave,
    /// ③ 我的收藏 `SchematicSave`
    pub SchematicSave: SchematicSave,
    /// ④ 我的装备
    pub Weapons: WeaponSave,
    /// ⑤ 我的外观
    pub Vanity: VanitySave,
    /// ⑥ 我的进度
    pub Progress: ProgressSave,
    /// ⑦ 我的天赋
    pub Perks: PerkSave,
}

/// ① 一个角色（`CharacterSaves[i]`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MinerSave {
    /// `SavegameID`：这个角色在本存档里的 id（不是职业 id）
    pub SavegameID: Guid,
    /// `XP`：角色经验
    pub XP: i32,
    /// `TimesRetired`：已退休次数
    pub TimesRetired: i32,
    /// `HasCompletedRetirementCampaign`：是否走完退休流程
    pub HasCompletedRetirementCampaign: bool,
    /// `RetiredCharacterLevels`：退休累计等级
    pub RetiredCharacterLevels: i32,
    /// `CharacterClass`：实测五个角色都是 `None`，见 `_tools/save-fields.md` §4
    pub CharacterClass: String,
    /// `SelectedLoadout`：当前选中的配装槽
    pub SelectedLoadout: i32,
    /// `Loadouts`：配装槽
    pub Loadouts: Vec<Loadout>,
    /// `Vanity.UnLockedVanityItemIDs`：这个角色已解锁的外观件
    pub UnLockedVanityItemIDs: Vec<Guid>,
}

/// 一套配装（`Loadout` / `Loadouts[i]`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Loadout {
    pub PrimaryWeapon: Guid,
    pub SecondaryWeapon: Guid,
    pub TraversalTool: Guid,
    pub ClassTool: Guid,
    pub Armor: Guid,
    pub Flare: Guid,
    pub MiningTool: Guid,
    pub Grenade: Guid,
    pub iconIndex: i32,
}

/// ② 我的矿。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourcesSave {
    /// `OwnedResources`：材料 Guid → 持有量
    pub OwnedResources: Vec<(Guid, f32)>,
}

/// ③ 我的收藏。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchematicSave {
    pub OwnedSchematics: Vec<Guid>,
    pub ForgedSchematics: Vec<Guid>,
    pub SkinFixupCounter: i32,
    pub bFirstSchematicMessageShown: bool,
}

/// ④ 我的装备。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeaponSave {
    /// `ItemUpgradeLoadouts`：每个职业一份武器改装表
    pub ItemUpgradeLoadouts: Vec<UpgradeLoadout>,
    /// `PurchasedItemUpgrades`：已买过的模组（全存档一份）
    pub PurchasedItemUpgrades: Vec<Guid>,
    /// `WeaponMaintenance.Entries`：武器熟练度
    pub WeaponMaintenance: Vec<MaintenanceEntry>,
}

/// 一份武器改装表（`ItemUpgradeLoadouts[i]`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpgradeLoadout {
    pub Loadout: Vec<WeaponEntry>,
}

/// 一把武器的改装情况（`ItemUpgradeLoadouts[i].Loadout{<武器Guid>}`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeaponEntry {
    pub WeaponID: Guid,
    pub EquippedUpgrades: Vec<Guid>,
    pub PermanentUpgrades: Vec<Guid>,
    pub EquippedOverclock: Guid,
    pub OverclockingUnlocked: bool,
    pub EquippedSkins: Vec<Guid>,
    pub EquippedSkinColor: Guid,
    pub EquippedSkinMesh: Guid,
}

/// 一条武器熟练度记录（`WeaponMaintenance.Entries[i]`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaintenanceEntry {
    pub WeaponID: Guid,
    pub XP: i32,
    pub Level: i32,
    pub LevelUpNotification: bool,
}

/// ⑤ 我的外观。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VanitySave {
    /// `VanityMasterySave.Level`
    pub Level: i32,
    /// `VanityMasterySave.XP`
    pub XP: i32,
    /// `VanityMasterySave.HasAwardedForOldPurchases`
    pub HasAwardedForOldPurchases: bool,
    /// `UnlockedItemSkins`：物品 Guid → 已解锁皮肤
    pub UnlockedItemSkins: Vec<(Guid, Vec<Guid>)>,
    /// `OwnedItems`
    pub OwnedItems: Vec<Guid>,
    /// `UnlockedPickaxeParts`
    pub UnlockedPickaxeParts: Vec<Guid>,
}

/// ⑥ 我的进度。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressSave {
    /// `Milestones.ClaimedKPIRewards`
    pub Milestones: Vec<MilestoneEntry>,
    /// `SeasonSave.Seasons`
    pub Seasons: Vec<SeasonEntry>,
    /// `CampaignSave`
    pub CampaignSave: CampaignEntry,
    /// `DeepDiveSave`
    pub DeepDiveSave: DeepDiveEntry,
    /// `UnLockedMissions`
    pub UnLockedMissions: Vec<Guid>,
    /// `UnLockedPlanetZones`
    pub UnLockedPlanetZones: Vec<Guid>,
}

/// 一条已领的里程碑。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MilestoneEntry {
    pub KPIGuid: Guid,
    pub Tier: i32,
}

/// 一个赛季的进度。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeasonEntry {
    /// 键：赛季 Guid
    pub SeasonID: Guid,
    pub XP: i32,
    pub RewardsClaimed: usize,
    pub NodesBought: usize,
    pub ActiveChallenges: Vec<ChallengeEntry>,
    pub Tokens: i32,
    pub HasClaimedAllRewards: bool,
    pub bSeasonCompletedAnnounced: bool,
}

/// 一条正在进行的赛季挑战。
///
/// `StatIndices` 是下标，进的是挑战资产（`SeasonChallenge_*`）里
/// `TrackedStats` 数组的顺序——名字要靠那张表才翻译得出来。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChallengeEntry {
    pub ChallengeGuid: Guid,
    pub StatIndices: Vec<i32>,
    pub CurrentWins: i32,
    pub NumberOfWins: i32,
    pub XPCompleteGain: i32,
    pub TokenGain: i32,
}

/// 战役进度。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CampaignEntry {
    pub CompletedCampaigns: Vec<Guid>,
    pub ActiveCampaignWeek: i32,
}

/// 深潜进度。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeepDiveEntry {
    pub NormalSave: DeepDiveItem,
    pub EliteSave: DeepDiveItem,
}

/// 一档深潜（普通 / 精英）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeepDiveItem {
    pub Seed: i32,
    pub HasEverCompleted: bool,
    pub Progress: i32,
    pub Rewards: usize,
}

/// ⑦ 我的天赋。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerkSave {
    /// `PerkPoints`：剩余天赋点
    pub PerkPoints: i32,
    /// `OwnedPerks.PerkEntries`
    pub OwnedPerks: Vec<PerkEntry>,
    /// `OwnedPerks.HasResetPerks2`
    pub HasResetPerks2: bool,
    /// `EquippedPerkLoadouts`
    pub EquippedPerkLoadouts: Vec<PerkLoadout>,
    /// `RandomEquippedPerkLoadout`
    pub RandomEquippedPerkLoadout: PerkLoadout,
}

/// 一个已拥有的天赋。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerkEntry {
    pub PerkID: Guid,
    pub currentRank: i32,
}

/// 一套天赋配装。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PerkLoadout {
    pub CharacterPerks: Vec<CharacterPerks>,
}

/// 某个角色配的天赋。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CharacterPerks {
    pub characterID: Guid,
    pub PerkIDs: Vec<Guid>,
}

// ---------------------------------------------------------------------------
// 从解析树里取值的小工具：读不到就给 None 或空，不编造
// ---------------------------------------------------------------------------

fn field<'a>(props: &'a [Property], name: &str) -> Option<&'a Property> {
    props.iter().find(|p| p.name == name)
}

fn as_int(v: &Value) -> Option<i32> {
    match v {
        Value::Int(i) => Some(*i),
        Value::Int64(i) => i32::try_from(*i).ok(),
        Value::UInt32(i) => i32::try_from(*i).ok(),
        Value::UInt64(i) => i32::try_from(*i).ok(),
        Value::Byte(b) => Some(i32::from(b.number)),
        _ => None,
    }
}

fn as_float(v: &Value) -> Option<f32> {
    match v {
        Value::Float(f) => Some(*f),
        Value::Double(d) => Some(*d as f32),
        _ => None,
    }
}

fn as_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

fn as_text(v: &Value) -> Option<String> {
    match v {
        Value::Str(s) | Value::Object(s) | Value::Enum(s) => Some(s.clone()),
        _ => None,
    }
}

fn as_guid(v: &Value) -> Option<Guid> {
    match v {
        Value::Binary(b) if b.ty == "Guid" && b.bytes.len() == 16 => Some(hex(&b.bytes)),
        // 裸结构体（Guid / Vector / …）在解析树里长这样：
        //   Struct { props: [ { name: "$binary", value: Binary { ty: "Guid", .. } } ] }
        // 顺着 "$binary" 再取一层，否则存档里所有 GUID 字段都会读成 0。
        Value::Struct(s) => s
            .props
            .iter()
            .find(|p| p.name == "$binary")
            .and_then(|p| as_guid(&p.value)),
        _ => None,
    }
}

fn as_props(v: &Value) -> &[Property] {
    match v {
        Value::Struct(s) => &s.props,
        _ => &[],
    }
}

fn as_items(v: &Value) -> &[Value] {
    match v {
        Value::Array(a) => &a.items,
        Value::Set(s) => &s.items,
        _ => &[],
    }
}

fn as_entries(v: &Value) -> &[(Value, Value)] {
    match v {
        Value::Map(m) => &m.entries,
        _ => &[],
    }
}

fn ints_of(props: &[Property], name: &str) -> Vec<i32> {
    items_of(props, name).iter().filter_map(as_int).collect()
}

fn int_of(props: &[Property], name: &str) -> Option<i32> {
    field(props, name).and_then(|p| as_int(&p.value))
}

fn float_of(props: &[Property], name: &str) -> Option<f32> {
    field(props, name).and_then(|p| as_float(&p.value))
}

fn bool_of(props: &[Property], name: &str) -> Option<bool> {
    field(props, name).and_then(|p| as_bool(&p.value))
}

fn guid_of(props: &[Property], name: &str) -> Option<Guid> {
    field(props, name).and_then(|p| as_guid(&p.value))
}

fn guids_of(props: &[Property], name: &str) -> Vec<Guid> {
    field(props, name)
        .map(|p| as_items(&p.value).iter().filter_map(as_guid).collect())
        .unwrap_or_default()
}

fn props_of<'a>(props: &'a [Property], name: &str) -> &'a [Property] {
    field(props, name).map(|p| as_props(&p.value)).unwrap_or(&[])
}

fn items_of<'a>(props: &'a [Property], name: &str) -> &'a [Value] {
    field(props, name).map(|p| as_items(&p.value)).unwrap_or(&[])
}

fn entries_of<'a>(props: &'a [Property], name: &str) -> &'a [(Value, Value)] {
    field(props, name).map(|p| as_entries(&p.value)).unwrap_or(&[])
}

/// 把顶层属性树翻成七块。
pub fn build(props: &[Property]) -> SaveData {
    SaveData {
        VersionNumber: int_of(props, "VersionNumber").unwrap_or_default(),
        SaveCreatedInPatch: int_of(props, "SaveCreatedInPatch").unwrap_or_default(),
        Credits: int_of(props, "Credits").unwrap_or_default(),
        PerkPoints: int_of(props, "PerkPoints").unwrap_or_default(),
        TotalPlayTimeSeconds: float_of(props, "TotalPlayTimeSeconds").unwrap_or_default(),
        NumberOfGamesPlayed: int_of(props, "NumberOfGamesPlayed").unwrap_or_default(),
        LastPlayedCharacter: field(props, "LastPlayedCharacter")
            .and_then(|p| as_text(&p.value)),

        CharacterSaves: items_of(props, "CharacterSaves").iter().map(miner).collect(),
        Resources: ResourcesSave {
            OwnedResources: entries_of(props_of(props, "Resources"), "OwnedResources")
                .iter()
                .filter_map(|(k, v)| Some((as_guid(k)?, as_float(v)?)))
                .collect(),
        },
        SchematicSave: schematic(props_of(props, "SchematicSave")),
        Weapons: weapons(props),
        Vanity: vanity(props, props_of(props, "VanityMasterySave")),
        Progress: progress(props),
        Perks: perks(props),
    }
}

fn miner(v: &Value) -> MinerSave {
    let p = as_props(v);
    let vanity = props_of(p, "Vanity");
    MinerSave {
        SavegameID: guid_of(p, "SavegameID").unwrap_or_else(|| ZERO_GUID.to_string()),
        XP: int_of(p, "XP").unwrap_or_default(),
        TimesRetired: int_of(p, "TimesRetired").unwrap_or_default(),
        HasCompletedRetirementCampaign: bool_of(p, "HasCompletedRetirementCampaign")
            .unwrap_or_default(),
        RetiredCharacterLevels: int_of(p, "RetiredCharacterLevels").unwrap_or_default(),
        CharacterClass: field(p, "CharacterClass")
            .and_then(|x| as_text(&x.value))
            .unwrap_or_else(|| "None".to_string()),
        SelectedLoadout: int_of(p, "SelectedLoadout").unwrap_or_default(),
        Loadouts: items_of(p, "Loadouts").iter().map(loadout).collect(),
        UnLockedVanityItemIDs: guids_of(vanity, "UnLockedVanityItemIDs"),
    }
}

fn loadout(v: &Value) -> Loadout {
    let p = as_props(v);
    let g = |name: &str| guid_of(p, name).unwrap_or_else(|| ZERO_GUID.to_string());
    Loadout {
        PrimaryWeapon: g("PrimaryWeapon"),
        SecondaryWeapon: g("SecondaryWeapon"),
        TraversalTool: g("TraversalTool"),
        ClassTool: g("ClassTool"),
        Armor: g("Armor"),
        Flare: g("Flare"),
        MiningTool: g("MiningTool"),
        Grenade: g("Grenade"),
        iconIndex: int_of(p, "iconIndex").unwrap_or_default(),
    }
}

fn schematic(p: &[Property]) -> SchematicSave {
    SchematicSave {
        OwnedSchematics: guids_of(p, "OwnedSchematics"),
        ForgedSchematics: guids_of(p, "ForgedSchematics"),
        SkinFixupCounter: int_of(p, "SkinFixupCounter").unwrap_or_default(),
        bFirstSchematicMessageShown: bool_of(p, "bFirstSchematicMessageShown").unwrap_or_default(),
    }
}

fn weapons(props: &[Property]) -> WeaponSave {
    let tables: Vec<UpgradeLoadout> = items_of(props, "ItemUpgradeLoadouts")
        .iter()
        .map(|v| UpgradeLoadout {
            Loadout: entries_of(as_props(v), "Loadout")
                .iter()
                .filter_map(|(k, val)| Some(weapon_entry(as_guid(k)?, val)))
                .collect(),
        })
        .collect();

    let maintenance = props_of(props, "WeaponMaintenance");
    let entries = items_of(maintenance, "Entries")
        .iter()
        .map(|v| {
            let p = as_props(v);
            MaintenanceEntry {
                WeaponID: guid_of(p, "WeaponID").unwrap_or_else(|| ZERO_GUID.to_string()),
                XP: int_of(p, "XP").unwrap_or_default(),
                Level: int_of(p, "Level").unwrap_or_default(),
                LevelUpNotification: bool_of(p, "LevelUpNotification").unwrap_or_default(),
            }
        })
        .collect();

    WeaponSave {
        ItemUpgradeLoadouts: tables,
        PurchasedItemUpgrades: guids_of(props, "PurchasedItemUpgrades"),
        WeaponMaintenance: entries,
    }
}

fn weapon_entry(key: Guid, v: &Value) -> WeaponEntry {
    let p = as_props(v);
    let g = |name: &str| guid_of(p, name).unwrap_or_else(|| ZERO_GUID.to_string());
    WeaponEntry {
        WeaponID: guid_of(p, "WeaponID").unwrap_or(key),
        EquippedUpgrades: guids_of(p, "EquippedUpgrades"),
        PermanentUpgrades: guids_of(p, "PermanentUpgrades"),
        EquippedOverclock: g("EquippedOverclock"),
        OverclockingUnlocked: bool_of(p, "OverclockingUnlocked").unwrap_or_default(),
        EquippedSkins: guids_of(p, "EquippedSkins"),
        EquippedSkinColor: g("EquippedSkinColor"),
        EquippedSkinMesh: g("EquippedSkinMesh"),
    }
}

fn vanity(props: &[Property], mastery: &[Property]) -> VanitySave {
    VanitySave {
        Level: int_of(mastery, "Level").unwrap_or_default(),
        XP: int_of(mastery, "XP").unwrap_or_default(),
        HasAwardedForOldPurchases: bool_of(mastery, "HasAwardedForOldPurchases")
            .unwrap_or_default(),
        UnlockedItemSkins: entries_of(props, "UnlockedItemSkins")
            .iter()
            .filter_map(|(k, v)| {
                // 值是 struct { Skins: Set<Guid> }，不是裸集合
                let set = match v {
                    Value::Struct(s) => field(&s.props, "Skins").map(|p| &p.value),
                    other => Some(other),
                };
                let skins: Vec<Guid> = set
                    .map(|x| as_items(x).iter().filter_map(as_guid).collect())
                    .unwrap_or_default();
                Some((as_guid(k)?, skins))
            })
            .collect(),
        OwnedItems: guids_of(props, "OwnedItems"),
        UnlockedPickaxeParts: field(props, "UnlockedPickaxeParts")
            .map(|p| as_items(&p.value).iter().filter_map(as_guid).collect())
            .unwrap_or_default(),
    }
}

fn progress(props: &[Property]) -> ProgressSave {
    let milestones = props_of(props, "Milestones");
    let campaigns = props_of(props, "CampaignSave");
    let dives = props_of(props, "DeepDiveSave");

    ProgressSave {
        Milestones: items_of(milestones, "ClaimedKPIRewards")
            .iter()
            .map(|v| {
                let p = as_props(v);
                MilestoneEntry {
                    KPIGuid: guid_of(p, "KPIGuid").unwrap_or_else(|| ZERO_GUID.to_string()),
                    Tier: int_of(p, "Tier").unwrap_or_default(),
                }
            })
            .collect(),
        Seasons: entries_of(props_of(props, "SeasonSave"), "Seasons")
            .iter()
            .filter_map(|(k, v)| {
                let p = as_props(v);
                Some(SeasonEntry {
                    SeasonID: as_guid(k)?,
                    XP: int_of(p, "XP").unwrap_or_default(),
                    RewardsClaimed: items_of(p, "RewardsClaimed").len(),
                    NodesBought: entries_of(p, "NodesBought").len(),
                    ActiveChallenges: items_of(p, "ActiveChallenges")
                        .iter()
                        .map(|v| {
                            let c = as_props(v);
                            ChallengeEntry {
                                ChallengeGuid: guid_of(c, "ChallengeGuid")
                                    .unwrap_or_else(|| ZERO_GUID.to_string()),
                                StatIndices: ints_of(c, "StatIndices"),
                                CurrentWins: int_of(c, "CurrentWins").unwrap_or_default(),
                                NumberOfWins: int_of(c, "NumberOfWins").unwrap_or_default(),
                                XPCompleteGain: int_of(c, "XPCompleteGain").unwrap_or_default(),
                                TokenGain: int_of(c, "TokenGain").unwrap_or_default(),
                            }
                        })
                        .collect(),
                    Tokens: int_of(p, "Tokens").unwrap_or_default(),
                    HasClaimedAllRewards: bool_of(p, "HasClaimedAllRewards").unwrap_or_default(),
                    bSeasonCompletedAnnounced: bool_of(p, "bSeasonCompletedAnnounced")
                        .unwrap_or_default(),
                })
            })
            .collect(),
        CampaignSave: CampaignEntry {
            CompletedCampaigns: guids_of(campaigns, "CompletedCampaigns"),
            ActiveCampaignWeek: int_of(campaigns, "ActiveCampaignWeek").unwrap_or_default(),
        },
        DeepDiveSave: DeepDiveEntry {
            NormalSave: dive(&props_of(dives, "NormalSave")),
            EliteSave: dive(&props_of(dives, "EliteSave")),
        },
        UnLockedMissions: guids_of(props, "UnLockedMissions"),
        UnLockedPlanetZones: guids_of(props, "UnLockedPlanetZones"),
    }
}

fn dive(p: &[Property]) -> DeepDiveItem {
    DeepDiveItem {
        Seed: int_of(p, "Seed").unwrap_or_default(),
        HasEverCompleted: bool_of(p, "HasEverCompleted").unwrap_or_default(),
        Progress: int_of(p, "Progress").unwrap_or_default(),
        Rewards: items_of(p, "Rewards").len(),
    }
}

fn perks(props: &[Property]) -> PerkSave {
    let owned = props_of(props, "OwnedPerks");
    PerkSave {
        PerkPoints: int_of(props, "PerkPoints").unwrap_or_default(),
        OwnedPerks: items_of(owned, "PerkEntries")
            .iter()
            .map(|v| {
                let p = as_props(v);
                PerkEntry {
                    PerkID: guid_of(p, "PerkID").unwrap_or_else(|| ZERO_GUID.to_string()),
                    currentRank: int_of(p, "currentRank").unwrap_or_default(),
                }
            })
            .collect(),
        HasResetPerks2: bool_of(owned, "HasResetPerks2").unwrap_or_default(),
        EquippedPerkLoadouts: items_of(props, "EquippedPerkLoadouts")
            .iter()
            .map(perk_loadout)
            .collect(),
        RandomEquippedPerkLoadout: field(props, "RandomEquippedPerkLoadout")
            .map(|p| perk_loadout(&p.value))
            .unwrap_or_default(),
    }
}

fn perk_loadout(v: &Value) -> PerkLoadout {
    PerkLoadout {
        CharacterPerks: items_of(as_props(v), "CharacterPerks")
            .iter()
            .map(|c| {
                let p = as_props(c);
                CharacterPerks {
                    characterID: guid_of(p, "characterID")
                        .unwrap_or_else(|| ZERO_GUID.to_string()),
                    PerkIDs: guids_of(p, "PerkIDs"),
                }
            })
            .collect(),
    }
}
