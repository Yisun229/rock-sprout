// 截「矿脉·搜了 0 条 → 清空筛选」这张图。
// 前提：debug 版应用（连 localhost:1420）+ 假 mod.io 服务器（FAKE_EMPTY=1）。
//   node _tools/_shot-empty-filtered.mjs
import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SHOT = join("_tmp", "ui-baseline", "states-018");

let page = null;
for (let i = 0; i < 120 && !page; i++) {
  await sleep(400);
  try {
    const list = await (await fetch("http://127.0.0.1:9222/json")).json();
    const t = list.find((x) => /^(http:\/\/tauri\.localhost|http:\/\/localhost:1420)/.test(x.url ?? ""));
    if (t?.webSocketDebuggerUrl) page = t;
  } catch {}
}
if (!page) throw new Error("没找到调试页面（debug 版是否已启动？）");

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

for (let i = 0; i < 120; i++) {
  if (await ev("!!window.__TAURI_INTERNALS__")) break;
  await sleep(300);
}
await sleep(1500);
await ev(`[...document.querySelectorAll('.nav-item')].find((b) => b.textContent.includes('矿脉'))?.click()`);
await sleep(800);
await ev(`[...document.querySelectorAll('.filter > .btn')].find((b) => b.textContent.includes('类型'))?.click()`);
await sleep(300);
await ev(`[...document.querySelectorAll('.filter-panel .filter-item')].find((b) => b.textContent.includes('体验优化'))?.click()`);
await sleep(300);
await ev(`document.body.click()`);
await sleep(300);
await ev(`(() => {
  const box = document.querySelector('.toolbar .searchbar input');
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
  setter.call(box, '钻头');
  box.dispatchEvent(new Event('input', { bubbles: true }));
})()`);
await sleep(300);
await ev(`document.querySelector('.toolbar .searchbar .btn')?.click()`);

let state = null;
for (let i = 0; i < 40; i++) {
  state = await ev(`(() => {
    const sec = [...document.querySelectorAll('section')].find((s) => s.offsetParent !== null);
    const box = sec?.querySelector('.empty');
    if (!box) return null;
    return { text: box.textContent.trim(), buttons: [...box.querySelectorAll('button')].map((b) => b.textContent.trim()) };
  })()`);
  if (state) break;
  await sleep(300);
}
// 把筛选面板收起来再截，图里只留空态本身
await ev(`[...document.querySelectorAll('.filter > .btn')].find((b) => b.textContent.includes('类型'))?.click()`);
await sleep(400);
mkdirSync(SHOT, { recursive: true });
await sleep(500);
const shot = await send("Page.captureScreenshot", { format: "png" });
writeFileSync(join(SHOT, "04-mine-empty-filtered.png"), Buffer.from(shot.result.data, "base64"));
console.log(JSON.stringify({ state }, null, 2));
sock.close();
spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
