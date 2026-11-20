// 从 DT_SchematicPricingTable 生成 app/src/data/forge-costs.json。
//
//   node _tools/_buildForgeCosts.mjs [--write]
//
// 数据来源：dtdump 读 DataTable 行（行是带属性名的结构，不需要猜偏移）。
// 价目是区间：CreditCost / Material1-3 各是 IRandRange{Min,Max}。

import { execFileSync } from "node:child_process";
import fs from "node:fs";

const ASSET = "_tools/_dt/out/FSD/Content/GameElements/Schematics/DT_SchematicPricingTable.uasset";
const OUT = "app/src/data/forge-costs.json";

const text = execFileSync("_tools/target/release/dtdump.exe", [ASSET], { encoding: "utf8" });

/** 把 "Struct(Some(\"IRandRange\")) [\n Min = Int 7000\n Max = Int 9000]" 取成 [min,max] */
function range(block) {
  const min = Number(/Min = Int (-?\d+)/.exec(block)?.[1]);
  const max = Number(/Max = Int (-?\d+)/.exec(block)?.[1]);
  if (!Number.isFinite(min) || !Number.isFinite(max)) throw new Error(`区间解析失败: ${block}`);
  return [min, max];
}

const rows = [];
for (const chunk of text.split(/\n {4}row /).slice(1)) {
  const name = /^(\S+)#(\d+) <(\w+)>/.exec(chunk);
  if (!name) continue;
  const props = {};
  for (const key of ["CreditCost", "Material1", "Material2", "Material3"]) {
    const at = chunk.indexOf(`${key} = Struct`);
    if (at < 0) throw new Error(`${name[1]} 缺 ${key}`);
    props[key] = range(chunk.slice(at));
  }
  rows.push({ name: name[1], instance: Number(name[2]), props });
}

if (rows.length !== 5) throw new Error(`应有 5 行，实际 ${rows.length}`);
const by = Object.fromEntries(rows.map((r) => [`${r.name}#${r.instance}`, r]));
const oc = by["Overclocks#0"];
if (!oc) throw new Error("缺 Overclocks#0");

const pick = (r) => ({
  credits: r.props.CreditCost,
  materials: [r.props.Material1, r.props.Material2, r.props.Material3],
});

const data = {
  $source:
    "FSD/Content/GameElements/Schematics/DT_SchematicPricingTable.uasset（DataTable，行结构 PricingTierEntry / IRandRange；用 _tools/target/release/dtdump.exe 读，生成脚本 _tools/_buildForgeCosts.mjs）",
  $verify:
    "5 行：Overclocks#0、Vanity#2/#3/#4、Resource#0；每行四个区间（CreditCost / Material1-3）。行是带属性名的结构，未按字节偏移猜读。",
  $notice:
    "价目是区间（每次在 Min–Max 之间取值）。材料数量是「三种随机矿物」，表里只给数量区间、不指定是哪三种。外观行（Vanity#2/#3/#4）对应哪个稀有度，资产里没有映射字段，本版不下结论。",
  overclock: pick(oc),
  vanity: ["Vanity#2", "Vanity#3", "Vanity#4"].filter((k) => by[k]).map((k) => ({ key: k, ...pick(by[k]) })),
  resource: pick(by["Resource#0"]),
};

const json = JSON.stringify(data, null, 2) + "\n";
if (process.argv.includes("--write")) {
  fs.writeFileSync(OUT, json);
  console.log(`wrote ${OUT} (${json.length} B)`);
} else {
  console.log(json);
}
