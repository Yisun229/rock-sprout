// 快速诊断：只关心「按回车/点按钮之后到底有没有行」。
import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { writeSearchCache } from "./fixture-cache.mjs";

const EXE = "_tools/target/release/rock-sprout.exe";
const DATA = "_tools/_m83c-data";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
if (existsSync(DATA)) rmSync(DATA, { recursive: true, force: true });
mkdirSync(DATA, { recursive: true });
copyFileSync("_tools/modio-token.txt", join(DATA, "modio-token.txt"));
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
const ev = async (expr) => (await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true })).result?.result?.value;
for (let i = 0; i < 100; i++) {
  if (await ev("!!window.__TAURI_INTERNALS__")) break;
  await sleep(300);
}
await sleep(1200);
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
  if ((await ev(`document.querySelectorAll('.vlist .row').length`)) > 0) break;
  await sleep(400);
}
const key = async (k, code) => {
  const base = { key: k, code: k, windowsVirtualKeyCode: code, nativeVirtualKeyCode: code };
  await send("Input.dispatchKeyEvent", { type: "rawKeyDown", ...base });
  await send("Input.dispatchKeyEvent", { type: "char", text: k, ...base });
  await send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
};
const active = () => ev(`(() => { const a = document.activeElement; return a ? { tag: a.tagName.toLowerCase(), cls: a.className, idx: a.dataset?.idx ?? null, tab: a.tabIndex } : null; })()`);
await ev(`document.querySelector('.toolbar .searchbar input')?.focus()`);
await sleep(200);
const before = await active();
let tabs = 0;
for (let i = 0; i < 12; i++) {
  await key("Tab", 9);
  tabs += 1;
  const a = await active();
  if (a && String(a.cls).split(" ").includes("row")) break;
}
const rowFocus = await active();
await key("ArrowDown", 40);
await sleep(500);
const afterDown = await active();
const map = await ev(`[...document.querySelectorAll('.vlist .row')].slice(0, 3).map((r) => r.dataset.idx + ':' + r.tabIndex)`);
await key("Enter", 13);
await sleep(1500);
const drawer = await ev(`!!document.querySelector('.mod-preview')`);
await key("Escape", 27);
await sleep(600);
const afterEsc = { drawer: await ev(`!!document.querySelector('.mod-preview')`), focus: await active() };
const out = {
  rows: await ev(`document.querySelectorAll('.vlist .row').length`),
  tabsToRow: tabs,
  before, rowFocus, afterDown, map, drawer, afterEsc,
  tabIndexes: await ev(`[...document.querySelectorAll('.vlist .row')].slice(0, 3).map((r) => r.dataset.idx + ':' + r.tabIndex)`),
  text: await ev(`(() => {
    const sec = [...document.querySelectorAll('section')].find((s) => s.offsetParent !== null);
    return sec ? sec.innerText.slice(0, 260) : null;
  })()`),
};
console.log(JSON.stringify(out, null, 2));
sock.close();
spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
