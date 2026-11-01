// M8-3 第二轮验收：背包刷新路径 + roving tabindex 键盘走查 + 补两张截图。
//   node _tools/_probe-m83b.mjs
import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { writeSearchCache } from "./fixture-cache.mjs";

const EXE = "_tools/target/release/rock-sprout.exe";
const DATA = "_tools/_m83b-data";
const SHOT = join("_tmp", "ui-baseline", "states-018");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

mkdirSync(SHOT, { recursive: true });
spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
if (existsSync(DATA)) rmSync(DATA, { recursive: true, force: true });
mkdirSync(DATA, { recursive: true });
copyFileSync("_tools/modio-token.txt", join(DATA, "modio-token.txt"));
// 一份「搜了也没结果」的缓存（total=0），一份「有 6 条」的缓存给键盘走查用
writeSearchCache(DATA, "没有这个", 0);
writeSearchCache(DATA, "性能", 6);

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
if (!page) throw new Error("连不上调试端口");

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
const ev = async (expr) => {
  const res = await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true });
  return res.result?.result?.value;
};
const key = async (k, code) => {
  const base = { key: k, code: k, windowsVirtualKeyCode: code, nativeVirtualKeyCode: code };
  await send("Input.dispatchKeyEvent", { type: "rawKeyDown", ...base });
  await send("Input.dispatchKeyEvent", { type: "char", text: k, ...base });
  await send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
};
const tab = () => key("Tab", 9);
const shot = async (name) => {
  const res = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(SHOT, `${name}.png`), Buffer.from(res.result.data, "base64"));
};
const go = async (page) => {
  await ev(`[...document.querySelectorAll('.nav-item')].find((b) => b.textContent.includes(${JSON.stringify(page)}))?.click()`);
  await sleep(700);
};
const active = () =>
  ev(`(() => {
    const a = document.activeElement;
    if (!a) return null;
    return {
      tag: a.tagName.toLowerCase(),
      cls: a.className,
      idx: a.dataset ? (a.dataset.idx ?? null) : null,
      tab: a.tabIndex,
      value: a.value ?? null,
    };
  })()`);

for (let i = 0; i < 100; i++) {
  if (await ev("!!window.__TAURI_INTERNALS__")) break;
  await sleep(300);
}
await sleep(1200);
const out = {};

// ── D) 搜了 0 条 + 清空筛选 ─────────────────────────
await go("矿脉");
await ev(`(() => {
  const box = document.querySelector('.toolbar .searchbar input');
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
  setter.call(box, '没有这个');
  box.dispatchEvent(new Event('input', { bubbles: true }));
})()`);
await sleep(300);
// 先选一个类型，让「清空筛选」有出现的理由
await ev(`[...document.querySelectorAll('.filter > .btn')].find((b) => b.textContent.includes('类型'))?.click()`);
await sleep(300);
await ev(`[...document.querySelectorAll('.filter-panel .filter-item')].find((b) => b.textContent.includes('体验优化'))?.click()`);
await sleep(300);
await ev(`document.body.click()`);
await sleep(200);
await ev(`document.querySelector('.toolbar .searchbar .btn')?.click()`);
for (let i = 0; i < 40; i++) {
  if (await ev(`!!document.querySelector('[data-page] .empty, section .empty')`)) break;
  await sleep(300);
}
await sleep(500);
out.emptyFiltered = await ev(`(() => {
  const sec = [...document.querySelectorAll('section')].find((s) => s.offsetParent !== null);
  const box = sec?.querySelector('.empty');
  return box ? { text: box.textContent.trim(), buttons: [...box.querySelectorAll('button')].map((b) => b.textContent.trim()) } : null;
})()`);
await shot("04-mine-empty-filtered");

// ── C) 背包满（先导入，再进页面）─────────────────────
const modA = join(process.cwd(), "_tools", "fixtures", "small-mod.zip");
await ev(`window.__TAURI_INTERNALS__.invoke("import_local", { path: ${JSON.stringify(modA)} })`);
await sleep(2500);
await go("背包");
for (let i = 0; i < 20; i++) {
  if ((await ev(`document.querySelectorAll('[data-page] .row, .bag-list .row').length`)) > 0) break;
  await sleep(300);
}
await sleep(400);
out.bagAfterEntry = await ev(`document.querySelectorAll('[data-page] .row, .bag-list .row').length`);
await shot("05-bag-full");

// ── A) 背包开着时别处装上了：切页重读 vs 广播重读 ────
const modB = join(process.cwd(), "_tools", "fixtures", "other-mod.zip");
const before = await ev(`document.querySelectorAll('[data-page] .row, .bag-list .row').length`);
await ev(`window.__TAURI_INTERNALS__.invoke("import_local", { path: ${JSON.stringify(modB)} })`);
await sleep(2000);
const stale = await ev(`document.querySelectorAll('[data-page] .row, .bag-list .row').length`);
// 广播一次：这正是 useQueue 跑完 / 一键升级成功时会发的事件
await ev(`window.dispatchEvent(new Event('rs:installed'))`);
await sleep(1500);
const afterSignal = await ev(`document.querySelectorAll('[data-page] .row, .bag-list .row').length`);
out.bagRefresh = { before, staleWithoutSignal: stale, afterSignal };

// ── B) roving tabindex 键盘走查 ─────────────────────
await go("矿脉");
// 先把类型筛选退回「全部」：带筛选走的是 list_mods，不走缓存，断网下必报网络错
await ev(`[...document.querySelectorAll('.filter > .btn')].find((b) => b.textContent.includes('类型'))?.click()`);
await sleep(300);
await ev(`([...document.querySelectorAll('.filter-panel .filter-item')].find((b) => b.textContent.includes('全部')))?.click()`);
await sleep(300);
await ev(`document.body.click()`);
await sleep(300);
await ev(`(() => {
  const box = document.querySelector('.toolbar .searchbar input');
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
  setter.call(box, '性能');
  box.dispatchEvent(new Event('input', { bubbles: true }));
  box.focus();
})()`);
await sleep(200);
await key("Enter", 13);
for (let i = 0; i < 40; i++) {
  if ((await ev(`document.querySelectorAll('.vlist .row').length`)) > 0) break;
  await sleep(300);
}
await sleep(600);
out.focusAfterSearch = await active();

let tabsToRow = 0;
for (let i = 0; i < 12; i++) {
  await tab();
  tabsToRow += 1;
  const a = await active();
  if (a && String(a.cls).split(" ").includes("row")) break;
}
out.reachRow = { tabs: tabsToRow, ...(await active()) };

await key("ArrowDown", 40);
await sleep(400);
out.afterArrowDown = await active();
out.tabIndexMap = await ev(`[...document.querySelectorAll('.vlist .row')].slice(0, 3).map((r) => r.dataset.idx + ':' + r.tabIndex)`);
await shot("kbd-row");

await key("Enter", 13);
await sleep(1600);
out.drawerAfterEnter = await ev(`!!document.querySelector('.mod-preview')`);
await shot("kbd-drawer");

await key("Escape", 27);
await sleep(600);
out.afterEscape = { drawer: await ev(`!!document.querySelector('.mod-preview')`), focus: await active() };

console.log(JSON.stringify(out, null, 2));
sock.close();
spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
