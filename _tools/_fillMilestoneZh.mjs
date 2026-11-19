// 给 app/src/data/milestone-names.json 的 57 条里程碑补中文名。
//
// 链路（全离线，不连游戏）：
//   1. 从 _tools/.pak-list.txt 取出 FSD/Content/GameElements/KPI/Milestones/*.uasset|.uexp，
//      pakdump extract-list 抽到 _tools/_mil/
//   2. propdump 读每份资产的 FormattedTitle（Text），得到 namespace 与键
//   3. 在 FSD/Content/Localization/Game/{en,zh-CN}/Game.locres 里按键取值
//   4. 拿 en 与表里原有的 en 对一遍（只差行尾空格也算一致），占位符 {value}/{character} 必须一一对应
//
// 用法：node _tools/_fillMilestoneZh.mjs          只检查、打印统计
//       node _tools/_fillMilestoneZh.mjs --write  写回 milestone-names.json
import fs from "node:fs";
import { execFileSync } from "node:child_process";
import { parseLocres } from "./_locres2.mjs";

const PAK =
  "D:/steam/steamapps/common/Deep Rock Galactic/FSD/Content/Paks/FSD-WindowsNoEditor.pak";
const BIN = "_tools/target/release";
const WORK = "_tools/_mil";
const MANIFEST = "_tmp/v031mil/manifest.txt";
const LOC = "_tools/_pak/loc/FSD/Content/Localization/Game";
const JSON_PATH = "app/src/data/milestone-names.json";
const WRITE = process.argv.includes("--write");

function extractAssets() {
  const list = fs.readFileSync("_tools/.pak-list.txt", "utf8");
  const assets = list
    .split(/\r?\n/)
    .map((s) => s.trim())
    .filter((s) => /^FSD\/Content\/GameElements\/KPI\/Milestones\/MIL_.*\.uasset$/.test(s));
  if (!assets.length) throw new Error("pak 清单里没找到 MIL_ 资产");
  const pairs = assets.flatMap((a) => [a, a.replace(/\.uasset$/, ".uexp")]);
  fs.mkdirSync("_tmp/v031mil", { recursive: true });
  fs.writeFileSync(MANIFEST, pairs.join("\n") + "\n");
  execFileSync(`${BIN}/pakdump.exe`, ["extract-list", PAK, MANIFEST, WORK], {
    stdio: "ignore",
  });
  return assets.map((a) => a.replace(/^FSD\/Content\//, "").replace(/\.uasset$/, ""));
}

function titlesOf(assets) {
  const out = new Map();
  for (const rel of assets) {
    const file = `${WORK}/FSD/Content/${rel}.uasset`;
    const raw = execFileSync(`${BIN}/propdump.exe`, [file], {
      encoding: "utf8",
      maxBuffer: 1 << 26,
    });
    const j = JSON.parse(raw);
    const title = j.exports?.[0]?.props?.find((p) => p.path === "FormattedTitle");
    if (!title?.value?.key) throw new Error(`${rel} 没有 FormattedTitle`);
    out.set(j.asset, {
      key: title.value.key,
      table: title.value.table ?? "",
      en: title.value.en ?? "",
    });
  }
  return out;
}

function locresValue(loc, table, key) {
  const ns =
    loc.namespaces.find((n) => n.name === table) ??
    loc.namespaces.find((n) => n.name === "") ??
    null;
  if (!ns?.keys?.has(key)) return null;
  return ns.keys.get(key);
}

function placeholders(s) {
  return [...String(s).matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(",");
}

const assets = extractAssets();
const titles = titlesOf(assets);
const en = parseLocres(`${LOC}/en/Game.locres`);
const zh = parseLocres(`${LOC}/zh-CN/Game.locres`);

const data = JSON.parse(fs.readFileSync(JSON_PATH, "utf8"));
const rows = Object.entries(data.milestones);
const missing = [];
const mismatch = [];
const mapped = new Map();
for (const [, m] of rows) {
  const t = titles.get(m.asset);
  if (!t) {
    missing.push(`${m.asset}（资产没抽到）`);
    continue;
  }
  const zhText = locresValue(zh, t.table, t.key);
  if (!zhText) {
    missing.push(`${m.asset}（zh-CN locres 里没有这个键 ${t.key}）`);
    continue;
  }
  const enLoc = locresValue(en, t.table, t.key);
  const sameEn = enLoc == null ? "en 缺键" : enLoc.trim() === m.en.trim();
  if (sameEn !== true) mismatch.push(`${m.asset}: 表里「${m.en}」vs 资产「${t.en}」vs en locres「${enLoc}」`);
  if (placeholders(m.en) !== placeholders(zhText))
    mismatch.push(`${m.asset}: 占位符不一致「${m.en}」/「${zhText}」`);
  mapped.set(m.asset, { zh: zhText, key: t.key, table: t.table });
}
if (missing.length) throw new Error(`有 ${missing.length} 条取不到：\n${missing.join("\n")}`);
if (mismatch.length) throw new Error(`有 ${mismatch.length} 条对不上：\n${mismatch.join("\n")}`);

const meta = {
  $zh:
    "中文名是里程碑在游戏里的标题原文，取自 zh-CN 本地化，未改写措辞；{value} 由每级完成量填、{character} 由职业名填，两处占位符都原样保留。",
  $zhSource:
    "资产 GameElements/KPI/Milestones/MIL_*.uasset 的 FormattedTitle（Text，namespace 空）；中文取 FSD/Content/Localization/Game/zh-CN/Game.locres 同键；抽取脚本 _tools/_fillMilestoneZh.mjs（pakdump + propdump + _locres2）。",
};
const verify = `表内 ${rows.length} 条：zh ${mapped.size} 条、缺 ${rows.length - mapped.size} 条；en 与资产/locres 逐条比对一致（行尾空格忽略）；{value}/{character} 占位符逐条一致。`;

let text = fs.readFileSync(JSON_PATH, "utf8");
const crlf = text.includes("\r\n");
if (crlf) throw new Error("文件是 CRLF，先归一化成 LF 再跑");

for (const [asset, v] of mapped) {
  const needle = `"asset": ${JSON.stringify(asset)},`;
  const at = text.indexOf(needle);
  if (at < 0) throw new Error(`找不到 ${asset} 的 asset 行`);
  if (text.indexOf(needle, at + 1) >= 0) throw new Error(`${asset} 的 asset 行出现多次`);
  const enLineStart = text.lastIndexOf('\n   "en": ', at);
  if (enLineStart < 0) throw new Error(`${asset} 上方没有 en 行`);
  const enLineEnd = text.indexOf("\n", enLineStart + 1);
  const zhLine = `\n   "zh": ${JSON.stringify(v.zh)},\n   "zhKey": ${JSON.stringify(v.key)},`;
  if (text.slice(enLineStart, enLineEnd).includes('"zh"')) continue;
  text = text.slice(0, enLineEnd) + zhLine + text.slice(enLineEnd);
}

text = text.replace(
  /^(\s*)"\$notice": "([^"]*)",/m,
  (_m, sp, notice) =>
    `${sp}"$notice": "${notice}",\n${sp}"${Object.keys(meta)[0]}": ${JSON.stringify(meta.$zh)},\n${sp}"${Object.keys(meta)[1]}": ${JSON.stringify(meta.$zhSource)},`
);
text = text.replace(/^(\s*)"\$verify": "[^"]*",/m, (_m, sp) => `${sp}"$verify": ${JSON.stringify(verify)},`);

const check = JSON.parse(text);
const withZh = Object.values(check.milestones).filter((m) => m.zh).length;
console.log(
  `assets ${assets.length} | titles ${titles.size} | rows ${rows.length} | zh ${withZh} | missing ${missing.length} | mismatch ${mismatch.length}`
);
console.log("sample:", JSON.stringify(Object.entries(check.milestones)[0]));
if (WRITE) {
  fs.writeFileSync(JSON_PATH, text);
  console.log("written", JSON_PATH);
} else {
  console.log("dry run（加 --write 才写回）");
}
