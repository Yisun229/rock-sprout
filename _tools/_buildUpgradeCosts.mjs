// 未解锁改装的消耗：信用点（Cost）+ 资源与数量（UpgradeCraftingCost）。
//
//   node _tools/_buildUpgradeCosts.mjs [--write]
//
// 输入：_tmp/v030/upg.jsonl（全属性）、_tmp/v030/upg-names.jsonl（asset → guid）、
//       app/src/data/names.json（矿物中文名）
// 输出：app/src/data/upgrade-costs.json

import fs from "node:fs";

const WRITE = process.argv.includes("--write");
const OUT = "app/src/data/upgrade-costs.json";

const names = JSON.parse(fs.readFileSync("app/src/data/names.json", "utf8"));
const zhByEn = new Map();
for (const one of Object.values(names.resources || {})) {
  if (one?.en && one?.zh) zhByEn.set(one.en, one.zh);
}
function zhOfResource(asset) {
  const bare = asset.replace(/^RES_(CARVED|VEIN|EMBED)_/, "");
  if (zhByEn.has(bare)) return zhByEn.get(bare);
  for (const [en, zh] of zhByEn) {
    if (en.startsWith(bare)) return zh;
  }
  return null;
}

const guidByAsset = new Map();
for (const line of fs.readFileSync("_tmp/v030/upg-names.jsonl", "utf8").split("\n")) {
  if (!line.trim()) continue;
  const row = JSON.parse(line);
  if (row.guid) guidByAsset.set(row.asset, row.guid);
}

const costs = {};
let total = 0;
let withCredits = 0;
let withResources = 0;
for (const line of fs.readFileSync("_tmp/v030/upg.jsonl", "utf8").split("\n")) {
  if (!line.trim()) continue;
  const row = JSON.parse(line);
  const guid = guidByAsset.get(row.asset);
  if (!guid) continue;
  const props = (row.exports?.[0]?.props ?? []).map((p) => p);
  const byPath = new Map(props.map((p) => [p.path, p]));

  // 武器改装资产里的 Cost 是 1 或 0 的占位，真正的信用点由 GD_UpgradeSettings
  // 按「改装类别 + 等级」查表；本表只保留表里明写的数量，占位不写。
  const rawCredits = Number(byPath.get("Cost")?.value ?? 0) || 0;
  const credits = rawCredits > 1 ? rawCredits : 0;
  const resources = [];
  for (const p of props) {
    const m = /^UpgradeCraftingCost\[\d+:(\{.*?\})\]\.UpgradeCraftingCost$/.exec(p.path);
    if (!m) continue;
    let key = null;
    try {
      key = JSON.parse(m[1]).target ?? null;
    } catch {
      key = null;
    }
    if (!key) continue;
    const amount = Number(p.value);
    if (!Number.isFinite(amount)) continue;
    resources.push({ asset: key, zh: zhOfResource(key), amount });
  }
  // 没有 UpgradeCraftingCost 的，退一步用 ResourceCost 的引用（没有数量，标出来）
  if (resources.length === 0) {
    for (const p of props) {
      const m = /^ResourceCost\[\d+\]\.\d+$/.exec(p.path);
      if (!m) continue;
      const key = p.value?.target;
      if (!key) continue;
      resources.push({ asset: key, zh: zhOfResource(key), amount: null });
    }
  }

  total += 1;
  if (credits > 0) withCredits += 1;
  if (resources.length > 0) withResources += 1;
  costs[guid] = { asset: row.asset, credits, resources };
}

if (total === 0) throw new Error("一条都没读到");

const data = {
  $source:
    "游戏 pak 的改装资产 WeaponsNTools/**/UPG_*.uasset 的 CDO 属性：Cost（信用点）、UpgradeCraftingCost（资源→数量）；资源中文名取自 app/src/data/names.json 的 resources。生成脚本 _tools/_buildUpgradeCosts.mjs",
  $verify: `${total} 条改装：有信用点 ${withCredits} 条、有资源 ${withResources} 条。信用点只收资产明写的数（Cost=1/0 是占位，按 0 处理，不算「免费」）；资源只有引用没有数量时 amount 写 null。`,
  costs,
};

const json = JSON.stringify(data, null, 2) + "\n";
if (WRITE) {
  fs.writeFileSync(OUT, json);
  console.log(`wrote ${OUT} (${json.length} B) total=${total} credits=${withCredits} resources=${withResources}`);
} else {
  console.log(`total=${total} credits=${withCredits} resources=${withResources}`);
}
