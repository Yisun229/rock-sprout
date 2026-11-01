// 关于弹层截图（改名验收用）：确认界面上的名字跟着 productName 走。
import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const OUT = join("_tmp", "ui-baseline", "brand-about.png");

let page = null;
for (let i = 0; i < 120 && !page; i++) {
  await sleep(300);
  try {
    const list = await (await fetch("http://127.0.0.1:9222/json")).json();
    const t = list.find((x) => (x.url ?? "").includes("tauri.localhost"));
    if (t?.webSocketDebuggerUrl) page = t;
  } catch {}
}
if (!page) throw new Error("没连上调试端口");

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
const docTitle = await ev("document.title");
await ev(`[...document.querySelectorAll('button')].find((b) => b.textContent.includes('关于'))?.click()`);
await sleep(900);
const about = await ev(`(() => {
  const card = document.querySelector('.about-head');
  const ver = [...document.querySelectorAll('.modal-card p')].map((p) => p.textContent.trim());
  return { head: card ? card.textContent.trim() : null, lines: ver.slice(0, 3) };
})()`);
mkdirSync(join("_tmp", "ui-baseline"), { recursive: true });
const shot = await send("Page.captureScreenshot", { format: "png" });
writeFileSync(OUT, Buffer.from(shot.result.data, "base64"));
console.log(JSON.stringify({ docTitle, about }, null, 2));
sock.close();
spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
