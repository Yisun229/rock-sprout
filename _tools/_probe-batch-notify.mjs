// 数一数：批量装的时候 rs:installed 发几次、背包页因此重读几次。
//   node _tools/_probe-batch-notify.mjs [条数]
import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { writeSearchCache } from "./fixture-cache.mjs";

const N = Number(process.argv[2] ?? 3);
const EXE = "_tools/target/release/rock-sprout.exe";
const DATA = "_tools/_m83d-data";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
if (existsSync(DATA)) rmSync(DATA, { recursive: true, force: true });
mkdirSync(DATA, { recursive: true });
copyFileSync("_tools/modio-token.txt", join(DATA, "modio-token.txt"));
writeSearchCache(DATA, "性能", 30);

const child = spawn(EXE, [], {
  env: {
    ...process.env,
    ROCK_SPROUT_DATA: DATA,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9222",
    HTTPS_PROXY: "http://127.0.0.1:9",
    HTTP_PROXY: "http://127.0.0.1:9",
    ALL_PROXY: "http://127.0.0.1:9",
  },
  detached: true,
  stdio: "ignore",
});
child.unref();
let page = null;
for (let i = 0; i < 100 && !page; i++) {
  await sleep(300);
  try {
    const list = await (await fetch("http://127.0.0.1:9222/json")).json();
    const t = list.find((x) => x.url?.startsWith("http://tauri.localhost"));
    if (t?.webSocketDebuggerUrl) page = t;
  } catch {}
}
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
await sleep(1200);
// 埋计数器：rs:installed 次数 + list_installed 次数
await ev(`(() => {
  window.__rs = { signals: 0, reads: 0 };
  window.addEventListener('rs:installed', () => window.__rs.signals++);
  const orig = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = (cmd, args, opts) => {
    if (cmd === 'list_installed') window.__rs.reads++;
    return orig(cmd, args, opts);
  };
})()`);

// 先进一次背包页：useInstalled 只有挂载后才订阅广播，这样才数得到「重读几次」
await ev(`[...document.querySelectorAll('.nav-item')].find((b) => b.textContent.includes('背包'))?.click()`);
await sleep(1500);
await ev(`[...document.querySelectorAll('.nav-item')].find((b) => b.textContent.includes('矿脉'))?.click()`);
await sleep(600);
await ev(`(() => {
  const box = document.querySelector('.toolbar .searchbar input');
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
  setter.call(box, '性能');
  box.dispatchEvent(new Event('input', { bubbles: true }));
})()`);
await sleep(300);
await ev(`document.querySelector('.toolbar .searchbar .btn')?.click()`);
for (let i = 0; i < 30; i++) {
  if ((await ev(`document.querySelectorAll('.vlist .row').length`)) >= N) break;
  await sleep(400);
}
// 勾 N 个，然后「全部装上」——断网会全失败，但队列照样跑完并广播
for (let i = 0; i < N; i++) {
  await ev(`document.querySelectorAll('.vlist .row .pick')[${i}]?.click()`);
  await sleep(120);
}
await ev(`[...document.querySelectorAll('.bulkbar .btn')].find((b) => b.textContent.includes('全部装上'))?.click()`);
await sleep(800);
// 预检弹窗：直接确认
await ev(`[...document.querySelectorAll('.modal .btn')].find((b) => /装|确定|继续/.test(b.textContent))?.click()`);
for (let i = 0; i < 60; i++) {
  const running = await ev(`!!document.querySelector('.barbar .queue, .bulkbar.queue') && !document.querySelector('.bulkbar.queue')?.textContent.includes('完了')`);
  const done = await ev(`(() => { const q = document.querySelector('.bulkbar.queue'); return !!q && /没成|完成|装好/.test(q.textContent); })()`);
  if (done && !running) break;
  await sleep(500);
}
await sleep(2500);
const out = { n: N, signals: await ev(`window.__rs.signals`), reads: await ev(`window.__rs.reads`) };
console.log(JSON.stringify(out, null, 2));
sock.close();
spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
