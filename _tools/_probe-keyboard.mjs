// M8-3 交付③：一条键盘路径走到底，逐步记录。
//   node _tools/_probe-keyboard.mjs
// Tab 到搜索框 → 输入 → Enter 搜索 → Tab 到第一行 → Enter 开抽屉 → Tab 到「装」→ Enter → Esc 关抽屉。
import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, cpSync, existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const DATA = "_tools/_kbd-data";
const OUT = join("_tmp", "ui-baseline", "states-018");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
mkdirSync(OUT, { recursive: true });
spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
rmSync(DATA, { recursive: true, force: true });
mkdirSync(DATA, { recursive: true });
if (existsSync("_tools/_probe-filters-data/cache")) cpSync("_tools/_probe-filters-data/cache", join(DATA, "cache"), { recursive: true });
if (existsSync("_tools/modio-token.txt")) copyFileSync("_tools/modio-token.txt", join(DATA, "modio-token.txt"));

const child = spawn("_tools/target/release/rock-sprout.exe", [], {
  env: {
    ...process.env,
    ROCK_SPROUT_DATA: DATA,
    HTTPS_PROXY: "http://127.0.0.1:9",
    ALL_PROXY: "http://127.0.0.1:9",
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9222",
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
    const t = list.find((x) => (x.url ?? "").startsWith("http://tauri.localhost"));
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
  if (res.result?.exceptionDetails) throw new Error(res.result.exceptionDetails.exception?.description ?? "页面里报错");
  return res.result?.result?.value;
};
const shot = async (name) => {
  await sleep(400);
  const res = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(OUT, `${name}.png`), Buffer.from(res.result.data, "base64"));
};
// 真键盘事件（走浏览器默认行为，不靠 JS 模拟 click）
const key = async (k, code, vk, text) => {
  await send("Input.dispatchKeyEvent", { type: "rawKeyDown", key: k, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk });
  if (text) await send("Input.dispatchKeyEvent", { type: "char", text });
  await send("Input.dispatchKeyEvent", { type: "keyUp", key: k, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk });
  await sleep(160);
};
const tab = () => key("Tab", "Tab", 9);
const enter = () => key("Enter", "Enter", 13, "\r");
const esc = () => key("Escape", "Escape", 27);
const typeText = async (text) => {
  for (const ch of text) {
    await send("Input.dispatchKeyEvent", { type: "char", text: ch });
    await sleep(40);
  }
};
const where = () =>
  ev(`(() => {
    const a = document.activeElement;
    if (!a) return "无焦点";
    const cls = (a.className || "").toString().split(" ").filter(Boolean).join(".");
    const txt = (a.textContent || "").trim().slice(0, 18);
    const tag = a.tagName.toLowerCase();
    return tag + (cls ? "." + cls : "") + (txt ? " 「" + txt + "」" : "") + (a.value ? " 值=" + a.value : "");
  })()`);

for (let i = 0; i < 100; i++) {
  if (await ev("!!window.__TAURI_INTERNALS__")) break;
  await sleep(300);
}
await sleep(1000);
// 先把鼠标挪开、焦点归位，然后纯靠键盘走
await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 2, y: 2, button: "none" });
await ev(`document.body.focus()`);

const log = [];
const step = async (name, note) => {
  log.push({ name, focus: await where(), note });
};

// 0) 先到矿脉页（路由这一步不算键盘路径的一部分）
await ev(`(() => { const b = [...document.querySelectorAll('.nav-item')].find((x) => x.textContent.trim() === "矿脉"); if (b) b.click(); return true; })()`);
await sleep(700);
await ev(`document.body.focus()`);

// 1) Tab 一直按到搜索框（顺序里前面还有今天页的按钮，所以不预设次数）
for (let i = 0; i < 40; i++) {
  await tab();
  const isInput = await ev(`document.activeElement && document.activeElement.tagName === 'INPUT'`);
  if (isInput) break;
}
await step("Tab 到搜索框", "连续 Tab，停在 input 上");

// 2) 输入关键词（缓存里有的那个）
await typeText("性能");
await step("输入关键词", "用真实键盘事件逐字输入");

// 3) Enter 搜索
await enter();
for (let i = 0; i < 40; i++) {
  if ((await ev(`document.querySelectorAll('.vlist .row').length`)) > 2) break;
  await sleep(300);
}
await step("Enter 搜索", "结果行出现");
await shot("11-kbd-search");

// 4) Tab 到第一行
for (let i = 0; i < 30; i++) {
  await tab();
  const onRow = await ev(`(() => { const a = document.activeElement; return !!a && (a.classList.contains('row') || !!a.closest('.row')); })()`);
  if (onRow) break;
}
await step("Tab 到第一行", "焦点落在列表行上");
await shot("12-kbd-row-focus");

// 5) Enter 打开抽屉
await enter();
for (let i = 0; i < 40; i++) {
  if (await ev(`!!document.querySelector('.mod-preview')`)) break;
  await sleep(300);
}
await sleep(600);
log.push({ name: "Enter 开抽屉", focus: await where(), drawer: await ev(`!!document.querySelector('.mod-preview')`), note: "抽屉是否真的出现，看 drawer 字段" });
await shot("13-kbd-drawer");

// 6) Tab 到「装」
for (let i = 0; i < 20; i++) {
  await tab();
  const onInstall = await ev(`(() => { const a = document.activeElement; return !!a && a.closest('.mod-preview') && /装/.test(a.textContent || ''); })()`);
  if (onInstall) break;
}
await step("Tab 到抽屉里的「装」", "焦点在装按钮上");
await shot("14-kbd-install-focus");

// 7) Enter（断网环境下会失败，这没关系——验的是键盘能走到并触发）
await enter();
await sleep(1500);
await step("Enter 触发装", "按下了装；离线所以会失败，键盘链路本身通了");

// 8) Esc 关抽屉
await esc();
for (let i = 0; i < 20; i++) {
  if (!(await ev(`!!document.querySelector('.mod-preview')`))) break;
  await sleep(200);
}
log.push({ name: "Esc 关抽屉", focus: await where(), drawer: await ev(`!!document.querySelector('.mod-preview')`), note: "drawer=false 才算关掉" });
await shot("15-kbd-closed");

console.log(JSON.stringify(log, null, 1));
sock.close();
spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
