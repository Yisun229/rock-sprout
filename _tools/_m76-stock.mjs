// 一次性：M7-6 的第一件事——纯原版起点的「装一遍、卸一遍」。
//
//   node _tools/_m76-stock.mjs move      把 MintCat 那套移出游戏目录（留档），拍「纯原版」快照
//   node _tools/_m76-stock.mjs install   起应用，把官方 UE4SS 铺进去
//   node _tools/_m76-stock.mjs uninstall 撤掉，确认回到纯原版（没有 dwmapi / ue4ss\ / UE4SS.log）
//   node _tools/_m76-stock.mjs restore   把 MintCat 那套原样放回，和移出前逐字节比
//
// 为什么先做这个：§0 第 39 条指出本轮所有装/卸的起点都是「已经装了 MintCat 那套」，
// 新用户的起点是纯原版。纯原版下卸载必须把我们写的 dwmapi.dll 删掉（而不是「还原」）。
// 这一步不绿，后面不用做。
import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, cpSync, existsSync, mkdirSync, readdirSync, readFileSync, renameSync, rmSync, statSync } from "node:fs";
import { join } from "node:path";

const EXE = "_tools/target/release/rock-sprout.exe";
const DATA = "_tools/_m76s-data";
const WIN64 = "D:/Steam/steamapps/common/Deep Rock Galactic/FSD/Binaries/Win64";
const KEEP = "_tmp/m76-stock";
const MOVED = join(KEEP, "moved");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let failures = 0;
function check(ok, label, extra = "") {
  if (!ok) failures++;
  console.log(`  ${ok ? "PASS" : "FAIL"}  ${label}${extra ? `  ${extra}` : ""}`);
}

function snapshot(label) {
  const r = spawnSync(process.execPath, ["_tools/_m7-snapshot.mjs", label], { encoding: "utf8" });
  const out = (r.stdout || "") + (r.stderr || "");
  const m = out.match(/快照 ([^：]+)：(\d+) 个文件/);
  console.log(`快照 ${label}：${m ? `${m[2]} 个文件` : out.trim().split("\n").slice(-1)[0]}`);
}

function compare(a, b) {
  const A = JSON.parse(readFileSync(join("_tmp/m7-2", `snap-${a}.json`), "utf8"));
  const B = JSON.parse(readFileSync(join("_tmp/m7-2", `snap-${b}.json`), "utf8"));
  const key = (s) => new Map(s.win64.files.map((f) => [f.rel, `${f.size}:${f.sha256}`]));
  const ka = key(A);
  const kb = key(B);
  const diffs = [];
  for (const [k, v] of ka) if (kb.get(k) !== v) diffs.push(`变了或没了：${k}`);
  for (const k of kb.keys()) if (!ka.has(k)) diffs.push(`多出来：${k}`);
  for (const w of A.watched ?? []) {
    const x = (B.watched ?? []).find((y) => y.path === w.path);
    if (x && x.exists !== w.exists) diffs.push(`盯着的 ${w.path}：${w.exists} → ${x.exists}`);
  }
  return diffs;
}

const has = (rel) => existsSync(join(WIN64, rel));

/// 目录里所有文件的总字节数（跨盘复制后拿它比一下）
function countBytes(dir) {
  let total = 0;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, entry.name);
    total += entry.isDirectory() ? countBytes(p) : statSync(p).size;
  }
  return total;
}

function killApp() {
  spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
}

function launch() {
  const env = {
    ...process.env,
    ROCK_SPROUT_DATA: DATA,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9222",
  };
  const child = spawn(EXE, [], { env, detached: true, stdio: "ignore" });
  child.unref();
}

async function withPage(fn) {
  killApp();
  await sleep(600);
  launch();
  let page = null;
  for (let i = 0; i < 80; i++) {
    await sleep(300);
    try {
      const res = await fetch("http://127.0.0.1:9222/json");
      const list = await res.json();
      const t = list.find((x) => x.url?.startsWith("http://tauri.localhost"));
      if (t?.webSocketDebuggerUrl) {
        const sock = new WebSocket(t.webSocketDebuggerUrl);
        let id = 0;
        const pending = new Map();
        sock.addEventListener("message", (ev) => {
          const msg = JSON.parse(ev.data);
          if (msg.id && pending.has(msg.id)) {
            pending.get(msg.id)(msg);
            pending.delete(msg.id);
          }
        });
        await new Promise((r) => sock.addEventListener("open", r));
        const send = (method, params) =>
          new Promise((resolve) => {
            const myId = ++id;
            pending.set(myId, resolve);
            sock.send(JSON.stringify({ id: myId, method, params }));
          });
        page = {
          evaluate: async (expr) => {
            const res = await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true });
            if (res.result?.exceptionDetails) return `抛错：${res.result.exceptionDetails.text}`;
            return res.result?.result?.value;
          },
          close: () => sock.close(),
        };
        break;
      }
    } catch {}
  }
  if (!page) throw new Error("连不上调试端口");
  for (let i = 0; i < 80; i++) {
    if (await page.evaluate("!!window.__TAURI_INTERNALS__")) break;
    await sleep(300);
  }
  await sleep(1200);
  try {
    return await fn(page);
  } finally {
    page.close();
    killApp();
  }
}

function move() {
  rmSync(DATA, { recursive: true, force: true });
  mkdirSync(join(DATA, "downloads"), { recursive: true });
  copyFileSync("_tools/modio-token.txt", join(DATA, "modio-token.txt"));
  copyFileSync("_tools/ue4ss-test/UE4SS_v3.0.1.zip", join(DATA, "downloads", "UE4SS_v3.0.1.zip"));

  snapshot("m76m-before");
  rmSync(MOVED, { recursive: true, force: true });
  mkdirSync(MOVED, { recursive: true });
  for (const rel of ["dwmapi.dll", "ue4ss"]) {
    if (!has(rel)) {
      console.log(`  ${rel} 不在游戏目录里（可能已经是纯原版）`);
      continue;
    }
    // D 盘挪到 E 盘不能用 rename（跨盘），先复制、验过大小再删原件
    const src = join(WIN64, rel);
    const dst = join(MOVED, rel);
    cpSync(src, dst, { recursive: true });
    const srcSize = statSync(src).isDirectory() ? countBytes(src) : statSync(src).size;
    const dstSize = statSync(dst).isDirectory() ? countBytes(dst) : statSync(dst).size;
    if (srcSize !== dstSize) {
      throw new Error(`复制到留档目录后大小对不上：${rel}（原 ${srcSize} / 留档 ${dstSize}）`);
    }
    rmSync(src, { recursive: true, force: true });
    console.log(`  移出：${rel}（${srcSize} 字节已留档到 ${dst}）`);
  }
  check(!has("dwmapi.dll"), "游戏目录里没有 dwmapi.dll 了");
  check(!has("ue4ss"), "游戏目录里没有 ue4ss\\ 了");
  snapshot("m76s-pure");
}

async function install() {
  await withPage(async (page) => {
    const before = await page.evaluate(`window.__TAURI_INTERNALS__.invoke("ue4ss_game_status")`);
    console.log("  装之前的状态：", JSON.stringify(before?.data ?? before));
    check(before?.data?.kind === "none", "起点是纯原版（kind=none）", `kind=${before?.data?.kind}`);

    const prep = await page.evaluate(`(async () => {
      const res = await window.__TAURI_INTERNALS__.invoke("ue4ss_prepare");
      return { ok: res.ok, error: res.error };
    })()`);
    console.log("  准备安装包：", JSON.stringify(prep));
    check(prep?.ok === true, "安装包解出来了");

    const out = await page.evaluate(`(async () => {
      const res = await window.__TAURI_INTERNALS__.invoke("ue4ss_install", { confirmReplace: false });
      return { ok: res.ok, error: res.error, items: res.data?.items ?? null };
    })()`);
    console.log("  装：", JSON.stringify(out));
    check(out?.ok === true, "装成功");
    for (const rel of ["UE4SS.dll", "dwmapi.dll", "UE4SS-settings.ini", "Mods"]) {
      check(has(rel), `游戏目录里有 ${rel}`);
    }
    const after = await page.evaluate(`window.__TAURI_INTERNALS__.invoke("ue4ss_game_status")`);
    check(after?.data?.kind === "ours", "状态变成 ours", `kind=${after?.data?.kind}`);
  });
  snapshot("m76s-installed");
}

async function uninstall() {
  await withPage(async (page) => {
    const out = await page.evaluate(`(async () => {
      const res = await window.__TAURI_INTERNALS__.invoke("ue4ss_uninstall");
      return { ok: res.ok, error: res.error };
    })()`);
    console.log("  卸：", JSON.stringify(out));
    check(out?.ok === true, "卸成功");
    for (const rel of ["UE4SS.dll", "dwmapi.dll", "UE4SS-settings.ini", "Mods", "ue4ss", "UE4SS.log", "UE4SS-settings.ini.bak"]) {
      check(!has(rel), `游戏目录里没有 ${rel} 了`);
    }
    const after = await page.evaluate(`window.__TAURI_INTERNALS__.invoke("ue4ss_game_status")`);
    check(after?.data?.kind === "none", "状态回到 none", `kind=${after?.data?.kind}`);
  });
  snapshot("m76s-after");
  const diffs = compare("m76s-pure", "m76s-after");
  if (diffs.length === 0) {
    console.log("  纯原版 → 装 → 卸：与动手前逐字节一致");
  } else {
    failures++;
    console.log(`  纯原版往返有 ${diffs.length} 处差异：`);
    for (const d of diffs) console.log(`    · ${d}`);
  }
}

function restore() {
  for (const rel of ["dwmapi.dll", "ue4ss"]) {
    const src = join(MOVED, rel);
    if (!existsSync(src)) continue;
    const dst = join(WIN64, rel);
    if (existsSync(dst)) rmSync(dst, { recursive: true, force: true });
    cpSync(src, dst, { recursive: true });
    console.log(`  放回：${rel}`);
  }
  snapshot("m76m-restored");
  const diffs = compare("m76m-before", "m76m-restored");
  if (diffs.length === 0) {
    console.log("  MintCat 那套原样回来了：与移出前逐字节一致");
  } else {
    failures++;
    console.log(`  放回后有 ${diffs.length} 处差异：`);
    for (const d of diffs) console.log(`    · ${d}`);
  }
}

const cmd = process.argv[2] ?? "move";
if (cmd === "move") move();
else if (cmd === "install") await install();
else if (cmd === "uninstall") await uninstall();
else if (cmd === "restore") restore();
else console.log("用法：node _tools/_m76-stock.mjs move|install|uninstall|restore");
process.exit(failures === 0 ? 0 : 1);
