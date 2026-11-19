// v0.3.1 批次 1 #33 · 给 names.json 里的资源补中文名
// 依据：Game.locres（en/zh-CN）· Resources 命名空间的资源名 + 精确匹配键
// 用法：node _tools/_fillResourceZh.mjs [--write]   （不带 --write 只打印对照表）
import fs from "node:fs";
import { parseLocres } from "./_locres2.mjs";

const EN = "_tools/_pak/loc/FSD/Content/Localization/Game/en/Game.locres";
const ZH = "_tools/_pak/loc/FSD/Content/Localization/Game/zh-CN/Game.locres";

const en = parseLocres(EN);
const zh = parseLocres(ZH);

const keyIn = (ns, key) => {
  const n = en.namespaces.find((x) => x.name === ns);
  const z = zh.namespaces.find((x) => x.name === ns);
  return { en: n?.keys.get(key), zh: z?.keys.get(key) };
};

// 资源名 → 中文 + 依据（哪条键的值等于英文名）
const MAP = {
  Bismor: ["蜂母石", "Resources/Bismor"],
  Croppa: ["铜矿", "精确匹配键 A35FACD2…（值 \"Croppa\"）"],
  "Enor Pearl": ["妙绝珠", "精确匹配键 16DCDB29…（值 \"Enor Pearl\"）"],
  Jadiz: ["玉石", "精确匹配键 43EB9F40…（值 \"Jadiz\"）"],
  Magnite: ["吸铁石", "精确匹配键 1363ACF8…（值 \"Magnite\"）"],
  Umanite: ["乌玛石", "精确匹配键 9C0D06AC…（值 \"Umanite\"）"],
  Phazyonite: ["方晶辉石", "精确匹配键 5AEE45F3…（值 \"Phazyonite\"）"],
  "Data Cell": ["数据单元", "精确匹配键 398CE890…（值 \"Data Cell\"）"],
  "Error Cube": ["错误方块", "复数键 963F390C…（\"Error Cubes\" → 错误方块）"],
  "Blank Core": ["空白核心", "精确匹配键 8E54E96C…（值 \"Blank Core\"）"],
};

// 先验证每条依据都能在 locres 里查到，查不到就报错，不写文件
for (const [ename, [zhName, why]] of Object.entries(MAP)) {
  const hit = [...en.namespaces].some((ns) =>
    [...ns.keys].some(([, v]) => v === ename || v === ename + "s"),
  );
  if (!hit) throw new Error(`locres 里找不到 ${ename}，依据 ${why} 不成立`);
  if (!zhName) throw new Error(`${ename} 的中文为空`);
}
console.log("locres 依据全部命中，共", Object.keys(MAP).length, "条");

const path = "app/src/data/names.json";
let text = fs.readFileSync(path, "utf8");
let added = 0;

for (const [id, entry] of Object.entries(JSON.parse(text).resources)) {
  const enName = entry.en;
  if (entry.zh || !MAP[enName]) continue;
  // 在这条资源的对象末尾补 "zh"
  const block = new RegExp(
    `("${id}": \\{\\s*"en": ${JSON.stringify(enName).replace(/[.*+?^${}()|[\]\\]/g, "\\$&")},[\\s\\S]*?"kind": "[a-z]+")`,
  );
  const m = text.match(block);
  if (!m) {
    console.error("没匹配到块：", id, enName);
    continue;
  }
  text = text.replace(block, `$1,\n      "zh": "${MAP[enName][0]}"`);
  added += 1;
  console.log(` ${enName} → ${MAP[enName][0]}  (${MAP[enName][1]})`);
}

if (process.argv.includes("--write")) {
  fs.writeFileSync(path, text);
  console.log("写入", path, "新增", added, "条 zh");
} else {
  console.log("（未加 --write，只打印；将新增", added, "条）");
}
