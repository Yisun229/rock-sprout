// 战役（assignment）任务清单与顺序：#27。
//
// 数据来源：`_tmp/cp.jsonl`（propdump 对 pak 里全部 `CP_*` 资产的导出）。
// 每个 CP 资产的 CDO 上有一个 `Missions` 数组，顺序就是界面上看到的先后；
// 数组里每项是 Object 引用（原始 index），落到同包的一个 `CampaignMission` 导出上。
// 实测 63 个 CP 资产全部是「原始 index − 2 = 导出下标」，所以这里固定减 2，
// 并逐条核对解出来的一定是 CampaignMission，对不上就报错、不猜。
//
// 两种战役：
//   - 固定任务（如 CP_Intro）：导出上有 PlanetZone + Mission.MissionTemplate；
//   - 运行时生成（如 CP_CoreHunt_1）：只有 Complexity/Duration，没有任务类型与区域，
//     界面上如实写「进游戏才生成」，不按顺序编。
//
// 用法：node _tools/_buildCampaignMissions.mjs [--write]
import fs from "node:fs";
import path from "node:path";

const ROOT = path.resolve(import.meta.dirname, "..");
const SRC = path.join(ROOT, "_tmp/cp.jsonl");
const OUT = path.join(ROOT, "app/src/data/campaign-missions.json");
const WRITE = process.argv.includes("--write");

const missionNames = JSON.parse(fs.readFileSync(path.join(ROOT, "app/src/data/mission-names.json"), "utf8"));
const zoneNames = JSON.parse(fs.readFileSync(path.join(ROOT, "app/src/data/zone-names.json"), "utf8"));

const missionByAsset = new Map();
for (const one of Object.values(missionNames.items)) missionByAsset.set(one.asset, one);
const zoneByAsset = new Map();
for (const one of Object.values(zoneNames.items)) zoneByAsset.set(one.asset, one);

const lines = fs
  .readFileSync(SRC, "utf8")
  .trim()
  .split("\n")
  .map((line) => JSON.parse(line));

const campaigns = {};
let slots = 0;
let fixed = 0;
let random = 0;
const clashes = [];

for (const asset of lines) {
  if (!/^CP_/.test(asset.asset)) continue;
  const cdo = (asset.exports ?? []).find((one) => one.export.startsWith("Default__"));
  if (!cdo) continue;
  const missionsProp = cdo.props.find((one) => one.path === "Missions");
  if (!missionsProp || !missionsProp.count) continue;

  const refs = cdo.props
    .filter((one) => /^Missions\[\d+\]\.\d+$/.test(one.path))
    .sort((a, b) => Number(a.path.match(/\[(\d+)\]/)[1]) - Number(b.path.match(/\[(\d+)\]/)[1]));
  if (refs.length !== missionsProp.count) {
    clashes.push(`${asset.asset}：Missions 说 ${missionsProp.count} 条，实际解出 ${refs.length} 条`);
    continue;
  }

  const list = [];
  for (const ref of refs) {
    const exportIndex = ref.value.index - 2;
    const target = asset.exports[exportIndex];
    if (!target || target.export !== "CampaignMission") {
      clashes.push(`${asset.asset} ${ref.path} → index ${ref.value.index} 解出来是 ${target?.export ?? "空"}`);
      continue;
    }
    const get = (prefix) => target.props.find((one) => one.path.startsWith(prefix));
    const mission = get("Mission.MissionTemplate")?.value?.target ?? null;
    const zone = get("PlanetZone")?.value?.target ?? null;
    const missionZh = mission ? missionByAsset.get(mission)?.zh ?? null : null;
    const zoneZh = zone ? zoneByAsset.get(zone)?.zh ?? null : null;
    const isRandom = mission === null;
    if (isRandom) random += 1;
    else fixed += 1;
    slots += 1;
    list.push({
      mission,
      missionZh,
      zone,
      zoneZh,
      random: isRandom,
      rewards: target.props.filter((one) => /^Rewards\[\d+\]\.\d+$/.test(one.path)).length,
    });
  }

  campaigns[asset.asset] = { missions: list };
}

if (clashes.length) {
  console.error("有解不出来的条目，不写文件：");
  for (const one of clashes) console.error(" -", one);
  process.exit(1);
}

const campaignsWithFixed = Object.values(campaigns).filter((one) => one.missions.some((m) => !m.random)).length;

const table = {
  $source:
    "pak 里全部 CP_* 战役资产（propdump 导出）；任务类型与区域取自 Missions 数组指向的 CampaignMission 导出，顺序即数组顺序（导出下标 = 原始 index − 2，63 个资产逐个核对过）",
  $verify: `${Object.keys(campaigns).length} 个战役、${slots} 个任务位：固定任务 ${fixed} 个、进游戏才生成的 ${random} 个；有固定任务的战役 ${campaignsWithFixed} 个`,
  $notice:
    "只有固定任务的战役才写得出任务类型与区域；其余战役的 Missions 数组里只有复杂度/时长，具体打什么是运行时生成的，表里标 random:true，界面写「进游戏才生成」，不按名字编。",
  campaigns,
};

const text = JSON.stringify(table, null, 1) + "\n";
console.log(table.$verify);
if (!WRITE) {
  console.log("dry-run；加 --write 才写文件。样例：", JSON.stringify(campaigns["CP_Intro"].missions.slice(0, 2)));
} else {
  fs.writeFileSync(OUT, text, "utf8");
  console.log("写出", path.relative(ROOT, OUT), text.length, "字节");
}
