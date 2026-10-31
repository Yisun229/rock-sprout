// M8-3：把 13 处写死颜色换成 token 引用。值一个字都不改，只换成变量名。
//   node _tools/_swap-colors.mjs
// 每一步都断言「改之前长这样、且只出现一次」，防止风格漂移时改错地方。
import { readFileSync, writeFileSync } from "node:fs";

const FILE = "app/src/styles.css";
const TOKENS = [
  ["--rock-deep", "#0d1419"],
  ["--rock-raise", "#2b3b4a"],
  ["--dust-strong", "#9fb0bd"],
  ["--text-disabled", "#6b7b88"],
  ["--amber-fill", "rgba(240, 169, 60, 0.12)"],
  ["--amber-edge", "rgba(240, 169, 60, 0.45)"],
  ["--scrim", "rgba(10, 15, 20, 0.72)"],
  ["--scrim-mask", "rgba(6, 10, 14, 0.6)"],
  ["--shadow", "rgba(0, 0, 0, 0.45)"],
];

const REPLACEMENTS = [
  ["  color: #6b7b88;", "  color: var(--text-disabled);", 2],
  ["  background: #2b3b4a;", "  background: var(--rock-raise);", 2],
  ["  background: rgba(240, 169, 60, 0.12);", "  background: var(--amber-fill);", 1],
  ["  border: 1px solid rgba(240, 169, 60, 0.45);", "  border: 1px solid var(--amber-edge);", 1],
  ["  background: #0d1419;", "  background: var(--rock-deep);", 1],
  ["  color: #9fb0bd;", "  color: var(--dust-strong);", 1],
  ["  background: rgba(10, 15, 20, 0.72);", "  background: var(--scrim);", 1],
  ["  background: rgba(6, 10, 14, 0.6);", "  background: var(--scrim-mask);", 1],
  ["  box-shadow: 0 8px 28px rgba(0, 0, 0, 0.45);", "  box-shadow: 0 8px 28px var(--shadow);", 1],
  ["  box-shadow: 0 6px 18px rgba(0, 0, 0, 0.45);", "  box-shadow: 0 6px 18px var(--shadow);", 2],
];

let css = readFileSync(FILE, "utf8");

// 1) 先在 :root 里登记新 token（值与原写死值逐字相同）
const anchor = "  --bone: #c9d3dc;\r\n";
if (!css.includes(anchor)) throw new Error(":root 锚点没找到（--bone 那行）");
const block = TOKENS.map(([name, value]) => `  ${name}: ${value};`).join("\r\n");
css = css.replace(anchor, `${anchor}\r\n  /* 下面这批是从写死的字面值收进来的：值为原样，肉眼不会有任何变化 */\r\n${block}\r\n`);

// 2) 逐条替换，并核对出现次数
for (const [from, to, times] of REPLACEMENTS) {
  const found = css.split(from).length - 1;
  if (found !== times) throw new Error(`「${from.trim()}」应出现 ${times} 次，实际 ${found} 次`);
  css = css.split(from).join(to);
}

// 3) 换完之后不该再有写死颜色（:root 里的 token 定义除外）
const offenders = css
  .split(/\r?\n/)
  .map((line, i) => ({ line, no: i + 1 }))
  .filter(({ line }) => !/^\s*--/.test(line) && /#[0-9a-fA-F]{3,8}|rgba?\(/.test(line));
if (offenders.length) {
  console.error("还有写死颜色没换：");
  for (const o of offenders) console.error(`  ${o.no}: ${o.line.trim()}`);
  process.exit(1);
}

writeFileSync(FILE, css, "utf8");
console.log(`已登记 ${TOKENS.length} 个新 token，替换 ${REPLACEMENTS.reduce((n, r) => n + r[2], 0)} 处写死颜色`);
