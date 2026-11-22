// 从 pak 抽出的改装资产里取「效果说明」，中文来自游戏本地化。
//
//   node _tools/_buildUpgradeTexts.mjs [--write]
//
// 输入：_tmp/v030/upg-names.jsonl（asset → guid + Name/Description 的 FText 键与英文）
//       _tools/_pak/loc/.../zh-CN/Game.locres
// 输出：app/src/data/upgrade-texts.json

import fs from "node:fs";
import { parseLocres } from "./_locres2.mjs";

const WRITE = process.argv.includes("--write");
const OUT = "app/src/data/upgrade-texts.json";
const ZH_LOCRES = "_tools/_pak/loc/FSD/Content/Localization/Game/zh-CN/Game.locres";

const loc = await parseLocres(ZH_LOCRES);
/** 按 namespace + key 取中文；namespace 为空时用 "" */
function zhOf(table, key) {
  const ns = (table || "").split("/").pop() || "";
  for (const one of loc.namespaces) {
    if (one.name !== ns) continue;
    const hit = one.keys.get(key);
    if (hit) return hit;
  }
  return null;
}

const texts = {};
let total = 0;
let withZh = 0;
for (const line of fs.readFileSync("_tmp/v030/upg-names.jsonl", "utf8").split("\n")) {
  if (!line.trim()) continue;
  const row = JSON.parse(line);
  if (!row.guid) continue;
  const desc = (row.fields || []).find((f) => f.prop === "Description");
  const name = (row.fields || []).find((f) => f.prop === "Name");
  if (!desc && !name) continue;
  total += 1;
  const zh = desc ? zhOf(desc.table, desc.key) : null;
  if (zh) withZh += 1;
  texts[row.guid] = {
    asset: row.asset,
    name: name ? { key: name.key, en: name.en, zh: zhOf(name.table, name.key) } : null,
    desc: desc
      ? { key: desc.key, table: desc.table || "", en: desc.en, zh }
      : null,
  };
}

if (total === 0) throw new Error("一条都没读到");
if (total !== Object.keys(texts).length) throw new Error("条数对不上");

const data = {
  $source:
    "游戏 pak 的改装资产 WeaponsNTools/**/UPG_*.uasset（propdump/namedump 读 Name 与 Description 的 FText），中文到 FSD/Content/Localization/Game/zh-CN/Game.locres 按 FText 键取；生成脚本 _tools/_buildUpgradeTexts.mjs",
  $verify: `${total} 条改装：有说明 ${total} 条、其中中文 ${withZh} 条。没有说明的条目不写进表，界面按「没有说明」处理。`,
  texts,
};

const json = JSON.stringify(data, null, 2) + "\n";
if (WRITE) {
  fs.writeFileSync(OUT, json);
  console.log(`wrote ${OUT} (${json.length} B) total=${total} zh=${withZh}`);
} else {
  console.log(`total=${total} zh=${withZh}`);
}
