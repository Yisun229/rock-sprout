// M8-1 的探针：点链接不换页、浏览器另开、底部一行提示 2 秒自消。
//   node _tools/_probe-nav.mjs
// 注意：它会真的用系统浏览器打开两个网页（这是被测的行为本身）。
import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { join } from "node:path";

const EXE = "_tools/target/release/rock-sprout.exe";
const DATA = "_tools/_nav-data";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
mkdirSync(DATA, { recursive: true });
if (existsSync("_tools/modio-token.txt")) {
  copyFileSync("_tools/modio-token.txt", join(DATA, "modio-token.txt"));
}

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
  if (res.result?.exceptionDetails) throw new Error(res.result.exceptionDetails.exception?.description ?? "页面里报错");
  return res.result?.result?.value;
};

for (let i = 0; i < 80; i++) {
  if (await evaluate("!!window.__TAURI_INTERNALS__")) break;
  await sleep(300);
}
await sleep(800);

const out = {};
out.startUrl = await evaluate("location.href");

// 没有 <a href="http…"> 了
out.leftoverAnchors = await evaluate(
  `[...document.querySelectorAll('a[href^="http"]')].map((a) => a.getAttribute('href'))`,
);

// 进设置页
await evaluate(`document.querySelectorAll('.nav-item')[3].click()`);
await sleep(900);

async function clickFirst(selector) {
  return evaluate(`(() => {
    const el = document.querySelector(${JSON.stringify(selector)});
    if (!el) return { found: false };
    el.scrollIntoView({ block: 'center' });
    el.click();
    return { found: true, before: el.textContent };
  })()`);
}

// ① 令牌卡里的链接
out.tokenClick = await clickFirst('.card .link-btn[data-url*="mod.io"]');
await sleep(250);
out.afterTokenUrl = await evaluate("location.href");
out.toastAfterToken = await evaluate("document.querySelector('.toast')?.textContent ?? null");
out.buttonTextAfterClick = await evaluate(
  `document.querySelector('.card .link-btn[data-url*="mod.io"]')?.textContent ?? null`,
);

// ② 提示 2 秒后消失
await sleep(2200);
out.toastLater = await evaluate("document.querySelector('.toast')?.textContent ?? null");
out.buttonTextLater = await evaluate(
  `document.querySelector('.card .link-btn[data-url*="mod.io"]')?.textContent ?? null`,
);

// ③ UE4SS 卡里的链接（在折叠区里，先展开）
await evaluate(`(() => {
  const box = document.querySelector('.ue4ss-manual');
  if (box && !box.open) box.open = true;
})()`);
await sleep(200);
out.ue4ssClick = await clickFirst('.ue4ss-manual .link-btn');
await sleep(250);
out.afterUe4ssUrl = await evaluate("location.href");
out.toastAfterUe4ss = await evaluate("document.querySelector('.toast')?.textContent ?? null");

// ④ 连点三次不出白屏（页面还在、还是应用自己的地址）
await evaluate(`(() => {
  const btn = document.querySelector('.card .link-btn[data-url*="mod.io"]');
  btn.click(); btn.click(); btn.click();
})()`);
await sleep(600);
out.afterTripleUrl = await evaluate("location.href");
out.appStillThere = await evaluate("!!document.querySelector('.shell') && document.querySelectorAll('.nav-item').length");

await sleep(2300);
const shot = await send("Page.captureScreenshot", { format: "png" });
const { writeFileSync } = await import("node:fs");
writeFileSync(join("_tmp", "ui-baseline", "nav-after-click.png"), Buffer.from(shot.result.data, "base64"));

console.log(JSON.stringify(out, null, 2));
sock.close();
spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
