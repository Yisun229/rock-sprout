// 一次性：M7-5「一键换新版」对着本地假服务器的验收。
//
//   node _tools/_probe-upgrade-local.mjs setup   造一份「装了旧版」的数据目录 + 拍快照
//   node _tools/_probe-upgrade-local.mjs run     起假服务器与 debug 版应用，四种下载方式各走一次
//   node _tools/_probe-upgrade-local.mjs clean   撤掉合并包、比对游戏目录快照
//
// 走假服务器是因为真 mod.io 的令牌已经失效（401），而且真接口上「正好有个已装 mod
// 出了新版」不可控。升级要验的是我们自己的顺序：下载 → 校验 → 合并 → 做成了才写记账 → 删旧文件。
//
// 应用必须用 debug 构建（`tauri.js build --debug --no-bundle`）：读 ROCK_SPROUT_MODIO_BASE
// 的那段代码只在 debug_assertions 里存在，release 包里没有。
import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const EXE = "_tools/target/debug/rock-sprout.exe";
const DATA = "_tools/_m76-local";
const MOD_ID = 1034060;
const NEW_FILE_ID = 777000111;
const FAKE_ORIGIN = "http://127.0.0.1:8899";
// api_root() 替换的是整段根地址（真接口那串自带 /v1），所以这里也要带 /v1
const FAKE = `${FAKE_ORIGIN}/v1`;
const PAK = "D:/Steam/steamapps/common/Deep Rock Galactic/FSD/Content/Paks/mods_P.pak";
const UPGRADE_DIR = join(DATA, "mods", "upgrade");
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

const recordPath = (id) => join(DATA, "mods", "installed", `${id}.json`);
const readRecord = () => JSON.parse(readFileSync(recordPath(MOD_ID), "utf8"));
const leftover = () => (existsSync(UPGRADE_DIR) ? readdirSync(UPGRADE_DIR) : []);

function killApp() {
  spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
}

function launch() {
  const env = {
    ...process.env,
    ROCK_SPROUT_DATA: DATA,
    ROCK_SPROUT_MODIO_BASE: FAKE,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9222",
  };
  const child = spawn(EXE, [], { env, detached: true, stdio: "ignore" });
  child.unref();
}

async function connect() {
  for (let i = 0; i < 80; i++) {
    await sleep(300);
    try {
      const res = await fetch("http://127.0.0.1:9222/json");
      const list = await res.json();
      const page = list.find((t) => t.url?.startsWith("http://tauri.localhost"));
      if (page?.webSocketDebuggerUrl) {
        const sock = new WebSocket(page.webSocketDebuggerUrl);
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
        const evaluate = async (expression) => {
          const res = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
          if (res.result?.exceptionDetails) return `抛错：${res.result.exceptionDetails.text}`;
          return res.result?.result?.value;
        };
        return { evaluate, close: () => sock.close() };
      }
    } catch {}
  }
  throw new Error("连不上调试端口");
}

async function setup() {
  rmSync(DATA, { recursive: true, force: true });
  mkdirSync(join(DATA, "mods", "installed"), { recursive: true });
  mkdirSync(join(DATA, "downloads"), { recursive: true });
  copyFileSync("_tools/modio-token.txt", join(DATA, "modio-token.txt"));

  // 「旧版」用另一份真包：升级失败时它必须原样还在
  const old = join(DATA, "downloads", `old-${MOD_ID}.zip`);
  copyFileSync("_tools/fixtures/other-mod.zip", old);
  const rec = {
    id: MOD_ID,
    name: "旧版假 mod",
    file_id: 1,
    filename: `old-${MOD_ID}.zip`,
    size: 462029,
    path: old,
    source: "modio",
    original_path: null,
    content_hash: null,
    missing: false,
    installed_at: Date.now() - 86_400_000,
    kind: "pak",
  };
  writeFileSync(recordPath(MOD_ID), JSON.stringify(rec, null, 2));
  console.log(`造好：${DATA}（装的「旧版」file_id=1，${rec.size} 字节）`);
  snapshot("m76-before");
}

async function run() {
  if (!existsSync(EXE)) {
    console.error(`找不到 debug 版：${EXE}\n先跑：cd app && node node_modules/@tauri-apps/cli/tauri.js build --debug --no-bundle`);
    process.exit(2);
  }
  killApp();
  const fake = spawn(process.execPath, ["_tools/_fake-modio.mjs"], { stdio: "inherit" });
  await sleep(800);

  launch();
  const page = await connect();
  for (let i = 0; i < 80; i++) {
    if (await page.evaluate("!!window.__TAURI_INTERNALS__")) break;
    await sleep(300);
  }
  await sleep(1200);

  const modes = [
    ["truncate", "下到一半断"],
    ["size", "校验不过（声明大小不一致）"],
    ["merge", "合并报错（内容是坏的）"],
    ["success", "成功"],
  ];

  for (const [mode, label] of modes) {
    console.log(`\n== ${mode}：${label} ==`);
    await fetch(`${FAKE_ORIGIN}/control?mode=${mode}`);
    const before = readRecord();

    const t0 = Date.now();
    const out = await page.evaluate(`(async () => {
      try {
        const res = await window.__TAURI_INTERNALS__.invoke("upgrade_mod", { id: ${MOD_ID} });
        return { ok: res.ok, error: res.error, data: res.data ? { items: res.data.items, backup: res.data.backup_dir } : null };
      } catch (e) { return { thrown: String(e) }; }
    })()`);
    const took = ((Date.now() - t0) / 1000).toFixed(1);
    console.log(`  ${took}s  ${JSON.stringify(out)}`);

    const after = readRecord();
    const left = leftover();
    if (mode === "success") {
      check(out?.ok === true, "调用成功");
      check(after.file_id === NEW_FILE_ID, `记账换成新文件 id`, `file_id=${after.file_id}`);
      check(after.path !== before.path && existsSync(after.path), "新文件落盘了", after.path);
      check(existsSync(PAK), "合并包生成了");
      check(left.length === 0, "临时目录清干净了", `剩 ${left.length} 项`);
    } else {
      check(out?.ok === false, "调用失败（符合预期）");
      check(after.file_id === 1, "记账没变", `file_id=${after.file_id}`);
      check(after.path === before.path && existsSync(before.path), "旧文件还在");
      check(!existsSync(PAK), "没有半截合并包");
      check(left.length === 0, "临时目录没留半成品", `剩 ${left.length} 项`);
      const msg = out?.error?.message ?? "";
      check(msg.includes("没换成，旧版还能用"), "错误说的是人话", msg);
    }
  }

  page.close();
  killApp();
  fake.kill();
  await sleep(500);
  snapshot("m76-after");
  console.log(`\n失败项：${failures}`);
  process.exit(failures === 0 ? 0 : 1);
}

async function clean() {
  killApp();
  if (existsSync(PAK)) {
    rmSync(PAK, { force: true });
    console.log("删掉测试生成的合并包");
  } else {
    console.log("没有合并包要删");
  }
  // 升级成功那次会把我们自己的加载器写进游戏目录（跟正常装 mod 一样），这里换回原版
  const loader = "D:/Steam/steamapps/common/Deep Rock Galactic/FSD/Binaries/Win64/x3daudio1_7.dll";
  const stock = "_tools/game-backup/2026-10-28/FSD/Binaries/Win64/x3daudio1_7.dll";
  if (existsSync(loader) && existsSync(stock) && statSync(loader).size !== statSync(stock).size) {
    copyFileSync(stock, loader);
    console.log(`加载器换回原版：${statSync(stock).size} 字节`);
  }
  snapshot("m76-clean");
  const diffs = compare("m76-before", "m76-clean");
  if (diffs.length === 0) {
    console.log("游戏目录和动手前逐字节一致。");
  } else {
    console.log(`游戏目录有 ${diffs.length} 处差异：`);
    for (const d of diffs) console.log(`  · ${d}`);
    process.exit(1);
  }
}

const cmd = process.argv[2] ?? "run";
if (cmd === "setup") await setup();
else if (cmd === "run") await run();
else if (cmd === "clean") await clean();
else console.log("用法：node _tools/_probe-upgrade-local.mjs setup|run|clean");
