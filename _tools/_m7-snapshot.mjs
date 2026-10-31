// M7-2 验证用：给 Win64 目录拍一份「字节级快照」。
//   node _tools/_m7-snapshot.mjs A-before
// 内容：Win64 整棵树里每个文件的 相对路径 + 字节数 + sha256，另外单独记 Paks\mods_P.pak
// （用户点名要盯它和 x3daudio1_7.dll / FSD-Win64-Shipping.exe）。结果写 _tmp/m7-2/snap-<标签>.json。
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const WIN64 = "D:/Steam/steamapps/common/Deep Rock Galactic/FSD/Binaries/Win64";
const PAKS = "D:/Steam/steamapps/common/Deep Rock Galactic/FSD/Content/Paks";
const OUT = join(import.meta.dirname, "..", "_tmp/m7-2");

const label = process.argv[2] ?? "snapshot";
mkdirSync(OUT, { recursive: true });

function walk(dir, prefix = "") {
  const out = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const abs = join(dir, entry.name);
    const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) out.push(...walk(abs, rel));
    else if (entry.isFile()) out.push({ rel, abs });
  }
  return out;
}

const files = walk(WIN64)
  .map(({ rel, abs }) => ({
    rel,
    size: statSync(abs).size,
    sha256: createHash("sha256").update(readFileSync(abs)).digest("hex"),
  }))
  .sort((a, b) => (a.rel < b.rel ? -1 : 1));

const watched = ["mods_P.pak"].map((name) => {
  const abs = join(PAKS, name);
  if (!existsSync(abs)) return { path: `Paks/${name}`, exists: false };
  return {
    path: `Paks/${name}`,
    exists: true,
    size: statSync(abs).size,
    sha256: createHash("sha256").update(readFileSync(abs)).digest("hex"),
  };
});

const snap = {
  label,
  taken_at: new Date().toISOString(),
  win64: { path: WIN64, fileCount: files.length, bytes: files.reduce((n, f) => n + f.size, 0), files },
  watched,
};
const outFile = join(OUT, `snap-${label}.json`);
writeFileSync(outFile, `${JSON.stringify(snap, null, 2)}\n`, "utf8");

console.log(`快照 ${label}：${files.length} 个文件 / ${(snap.win64.bytes / 1024 / 1024).toFixed(1)} MB`);
for (const f of files.filter((f) => !f.rel.includes("/"))) {
  console.log(`  ${f.rel}  ${f.size}  ${f.sha256.slice(0, 16)}…`);
}
console.log(`  ue4ss/ 下 ${files.filter((f) => f.rel.startsWith("ue4ss/")).length} 个文件`);
for (const w of watched) console.log(`  ${w.path}：${w.exists ? `${w.size} 字节 ${w.sha256.slice(0, 16)}…` : "不存在"}`);
console.log(`写到 ${outFile}`);
