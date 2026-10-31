// 第二层（导航拦截）单独验：用 JS 强行跳转，窗口也不许离开应用。
//   node _tools/_probe-nav2.mjs
// 它会真的让系统浏览器打开 example.com（拦截后转交浏览器，这是设计行为）。
import { spawn, spawnSync } from "node:child_process";
import { mkdirSync } from "node:fs";

const EXE = "_tools/target/release/rock-sprout.exe";
const DATA = "_tools/_nav2-data";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
mkdirSync(DATA, { recursive: true });

const child = spawn(EXE, [], {
  env: { ...process.env, ROCK_SPROUT_DATA: DATA, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9222" },
  detached: true,
  stdio: "ignore",
});
child.unref();

let page = null;
for (let i = 0; i < 80 && !page; i++) {
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
const evaluate = async (expr) => {
  const res = await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true });
  return res.result?.result?.value;
};

for (let i = 0; i < 80; i++) {
  if (await evaluate("!!window.__TAURI_INTERNALS__")) break;
  await sleep(300);
}
await sleep(800);

const out = {};
out.startUrl = await evaluate("location.href");

// ① 注入一个漏网的 <a>，点它
await evaluate(`(() => {
  const a = document.createElement('a');
  a.id = 'rogue';
  a.href = 'https://example.com/rogue';
  a.textContent = 'rogue';
  document.body.appendChild(a);
})()`);
await evaluate("document.getElementById('rogue').click()");
await sleep(1200);
out.urlAfterAnchor = await evaluate("location.href");
out.shellAlive1 = await evaluate("!!document.querySelector('.shell')");

// ② 直接用 location.href 跳
await evaluate("location.href = 'https://example.com/direct'");
await sleep(1200);
out.urlAfterLocation = await evaluate("location.href");
out.shellAlive2 = await evaluate("!!document.querySelector('.shell')");

// ③ 应用自己的页面照常
await evaluate(`document.querySelectorAll('.nav-item')[1]?.click()`);
await sleep(800);
out.urlAfterNav = await evaluate("location.href");
out.pageAfterNav = await evaluate("document.querySelector('[data-page]:not([hidden])')?.dataset.page");

console.log(JSON.stringify(out, null, 2));
sock.close();
spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
