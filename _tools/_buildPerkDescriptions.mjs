// 生成 app/src/data/perk-descriptions.json（天赋说明 + 逐级填好的数字）。
//
// 前置：把天赋目录的资产抽出来（只需一次）：
//   grep "GameElements/KPI/Perks/" _tools/.pak-list.txt | tr -d '\r' | grep "\.uasset$" \
//     | while read -r p; do printf '%s\n%s\n' "$p" "${p%.uasset}.uexp"; done > _tmp/perks.txt
//   _tools/target/release/pakdump.exe extract-list \
//     "D:/steam/steamapps/common/Deep Rock Galactic/FSD/Content/Paks/FSD-WindowsNoEditor.pak" \
//     _tmp/perks.txt _tools/_perksall
//
// 运行：node _tools/_buildPerkDescriptions.mjs
// 说明：占位符按同一条效果里的 RankValues 逐级填；填不了的原样保留并在 unclean 里写原因。

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { parseLocres } = require("./_locres2.mjs");

const ROOT = "_tools/_perksall/FSD/Content/GameElements/KPI/Perks";
const OUT = "app/src/data/perk-descriptions.json";
const NAMES = require("../app/src/data/perk-names.json").perks;

const enLoc = parseLocres("_tools/_pak/loc/FSD/Content/Localization/Game/en/Game.locres");
const zhLoc = parseLocres("_tools/_pak/loc/FSD/Content/Localization/Game/zh-CN/Game.locres");

function lookup(loc, key) {
  if (!key) return undefined;
  for (const name of ["", "PerkDescriptions"]) {
    const ns = loc.namespaces.find((n) => n.name === name);
    if (ns && ns.keys.has(key)) return ns.keys.get(key);
  }
  return undefined;
}

const fmt = (n) => String(Math.round(n * 1000) / 1000);
const SUPPORTED = new Set(["value", "float_value", "percent", "percent*100", "minutes"]);

function fill(text, v) {
  const bad = [];
  const out = text.replace(/\{([^}]+)\}/g, (all, name) => {
    if (name === "value" || name === "float_value" || name === "percent") return fmt(v);
    if (name === "percent*100") return fmt(v * 100);
    if (name === "minutes") return fmt(v / 60);
    bad.push(name);
    return all;
  });
  return { text: out, bad };
}

function walk(dir) {
  const out = [];
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) out.push(...walk(p));
    else if (e.name.endsWith(".uasset")) out.push(p);
  }
  return out;
}

const files = walk(ROOT).sort();
const assetToGuid = new Map(Object.values(NAMES).map((n) => [n.asset, n.guid]));
const perks = new Map(Object.values(NAMES).map((n) => [n.guid, { asset: n.asset, title: { en: n.en, zh: n.zh }, effects: [] }]));

let total = 0, cleaned = 0, partial = 0, missingZh = 0, missingEn = 0, noGuid = 0, debug = 0;
const unclean = [];

for (const f of files) {
  const j = JSON.parse(execFileSync("_tools/target/release/propdump.exe", [f], { maxBuffer: 1 << 28 }).toString());
  const guid = assetToGuid.get(path.basename(f, ".uasset"));
  if (!guid) { noGuid++; continue; }
  for (const e of j.exports) {
    const map = new Map();
    for (const p of e.props || []) {
      const m = /^Effects\[(\d+)\]\.Effects\.(.+)$/.exec(p.path);
      if (!m) continue;
      const i = +m[1];
      if (!map.has(i)) map.set(i, { vals: [] });
      const rec = map.get(i);
      if (m[2] === "FormattedDescription") rec.desc = p.value;
      const rv = /^RankValues\[(\d+)\]\.RankValues\.(RankIndex|Value)$/.exec(m[2]);
      if (rv) {
        const idx = +rv[1];
        rec.vals[idx] = rec.vals[idx] || {};
        rec.vals[idx][rv[2]] = p.value;
      }
    }
    for (const [, rec] of [...map.entries()].sort((a, b) => a[0] - b[0])) {
      const d = rec.desc || {};
      const key = d.key || "";
      const isTable = (d.table || "").includes("PerkDescriptions");
      const en = (isTable ? lookup(enLoc, key) : d.en) || d.en || lookup(enLoc, key) || "";
      if (!en) { missingEn++; continue; }
      if (en.includes("Show as {value}")) { debug++; continue; }
      const zh = key ? lookup(zhLoc, key) || "" : "";
      if (!zh) missingZh++;

      const values = [...rec.vals.filter(Boolean)]
        .sort((a, b) => (a.RankIndex ?? 0) - (b.RankIndex ?? 0))
        .map((v) => v.Value);
      const distinct = [];
      for (const v of values) if (!distinct.includes(v)) distinct.push(v);

      const ph = [...new Set((en.match(/\{[^}]+\}/g) || []).map((x) => x.slice(1, -1)))];
      const bad = ph.filter((p) => !SUPPORTED.has(p));
      const list = perks.get(guid).effects;
      if (list.some((x) => x.text.en === en && x.text.zh === zh)) continue;

      let cleanedRows = [];
      let reason = null;
      if (ph.length === 0) {
        cleanedRows = [{ en, zh }];
        cleaned++;
      } else if (bad.length === 0 && distinct.length > 0) {
        cleanedRows = distinct.map((v) => ({ value: v, en: fill(en, v).text, zh: fill(zh, v).text }));
        cleaned++;
      } else {
        reason = bad.length
          ? `占位符 ${bad.map((p) => "{" + p + "}").join("、")} 不知道按什么换算`
          : "资产里没有 RankValues，填不出数字";
        partial++;
        unclean.push({ asset: path.basename(f, ".uasset"), key: key || null, reason, values: distinct });
      }
      total++;
      list.push({ key: key || null, text: { en, zh }, values, cleaned: cleanedRows, unclean: reason });
    }
  }
}

const ordered = Object.fromEntries([...perks.entries()].sort((a, b) => a[1].asset.localeCompare(b[1].asset)));
const flat = Object.values(ordered).flatMap((p) => p.effects);
const out = {
  $comment:
    "天赋说明文字。en/zh 直接取自游戏本地化（不是人工翻译）；{value} 这类占位符按同一条效果的 RankValues 逐级填成可读文案，填不了的原样保留并在 unclean 里写原因——没有猜。",
  $source:
    "游戏 pak 里 Character/Vanity2 之外的天赋资产 GameElements/KPI/Perks/ 下 106 个（propdump 读 Title/Effects[].FormattedDescription/RankValues），文字到 FSD/Content/Localization/Game/{en,zh-CN}/Game.locres 的 PerkDescriptions 命名空间与空命名空间按 FText 键取值；脚本 _tools/_buildPerkDescriptions.mjs",
  $verify:
    `天赋 ${Object.keys(ordered).length} 个、说明 ${total} 条；zh ${flat.filter((e) => e.text.zh).length} 条、缺 zh ${missingZh} 条；可清洗 ${cleaned} 条、清洗不了 ${partial} 条（见 unclean）；丢弃调试字符串 ${debug} 条；没有天赋条目的资产 ${noGuid} 个`,
  perks: ordered,
};

fs.writeFileSync(OUT, JSON.stringify(out, null, 2) + "\n", "utf8");
console.log(out.$verify);
for (const u of unclean) console.log("unclean:", u.asset, u.key, "|", u.reason, JSON.stringify(u.values));
