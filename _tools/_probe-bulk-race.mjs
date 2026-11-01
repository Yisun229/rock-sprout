// 竞态复验：批量卸载两条时，两个重建会抢同一个 mods_P.pak.tmp。
// M8-4 修完之后连跑 5 轮「并行卸两条」+ 1 轮「队列卸五条」，全绿才算数。
//   node _tools/_probe-bulk-race.mjs
import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, rmSync } from "node:fs";
import { join } from "node:path";

const EXE = "_tools/target/release/rock-sprout.exe";
const DATA = "_tools/_race-data";
const ROUNDS = 5;
const FIVE = ["small-mod.zip", "other-mod.zip", "small-mod.zip", "other-mod.zip", "small-mod.zip"];
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
if (existsSync(DATA)) rmSync(DATA, { recursive: true, force: true });
mkdirSync(DATA, { recursive: true });
copyFileSync("_tools/modio-token.txt", join(DATA, "modio-token.txt"));

const child = spawn(EXE, [], {
  env: { ...process.env, ROCK_SPROUT_DATA: DATA, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9222" },
  detached: true,
  stdio: "ignore",
});
child.unref();

let page = null;
for (let i = 0; i < 100 && !page; i++) {
  await sleep(300);
  try {
    const list = await (await fetch("http://127.0.0.1:9222/json")).json();
    const t = list.find((x) => (x.url ?? "").includes("tauri.localhost"));
    if (t?.webSocketDebuggerUrl) page = t;
  } catch {}
}
if (!page) throw new Error("连不上调试端口");

const sock = new WebSocket(page.webSocketDebuggerUrl);
let id = 0;
const pending = new Map();
sock.addEventListener("message", (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) {
    pending.get(m.id)(m);
    pending.delete(m.id);
  }
});
await new Promise((r) => sock.addEventListener("open", r));
const send = (method, params) =>
  new Promise((res) => {
    const my = ++id;
    pending.set(my, res);
    sock.send(JSON.stringify({ id: my, method, params }));
  });
const ev = async (expr) =>
  (await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true })).result?.result?.value;

for (let i = 0; i < 100; i++) {
  if (await ev("!!window.__TAURI_INTERNALS__")) break;
  await sleep(300);
}
await sleep(1000);

const fixture = (name) => join(process.cwd(), "_tools", "fixtures", name);
const importOne = (name) =>
  ev(`window.__TAURI_INTERNALS__.invoke("import_local", { path: ${JSON.stringify(fixture(name))} })`);
const uninstall = (id) => ev(`window.__TAURI_INTERNALS__.invoke("uninstall_mod", { id: ${id} })`);
const listInstalled = async () => {
  const r = await ev(`window.__TAURI_INTERNALS__.invoke("list_installed")`);
  return r?.ok ? r.data : [];
};

const rounds = [];
for (let round = 1; round <= ROUNDS; round++) {
  // 每轮换不同的夹具，保证是两条不同的记录
  const names = round % 2 === 1 ? ["small-mod.zip", "other-mod.zip"] : ["small-mod.zip", "other-mod.zip"];
  for (const n of names) {
    const r = await importOne(n);
    if (r?.ok !== true) throw new Error(`第 ${round} 轮导入失败：${JSON.stringify(r?.error)}`);
  }
  const before = await listInstalled();
  // 真并发：两个 uninstall_mod 同时发出去，后端必须自己排队
  const results = await Promise.all(before.slice(0, 2).map((m) => uninstall(m.id)));
  const after = await listInstalled();
  const bad = results.filter((r) => r?.ok !== true);
  rounds.push({
    round,
    installedBefore: before.length,
    installedAfter: after.length,
    errors: bad.map((r) => `${r?.error?.code}: ${String(r?.error?.message).slice(0, 60)}`),
  });
}

// 附加一轮：队列里卸五条（走 UI 那套批量卸载）
const many = [];
for (const n of FIVE) {
  const r = await importOne(n);
  if (r?.ok === true) many.push(r.data?.id ?? null);
}
const bulk = await Promise.all(many.filter(Boolean).map((mid) => uninstall(mid)));
const left = await listInstalled();

console.log(
  JSON.stringify(
    {
      rounds,
      allRoundsOk: rounds.every((r) => r.errors.length === 0 && r.installedAfter === r.installedBefore - 2),
      bulkFive: { fired: bulk.length, ok: bulk.filter((r) => r?.ok === true).length, left: left.length },
    },
    null,
    2,
  ),
);
sock.close();
spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
