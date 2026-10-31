// M8-2 补证：有数据（缓存可回放）的列表状态，1700 与 1920 各一张，并量三列位置。
//   node _tools/_shot-list-row.mjs
import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, cpSync, mkdirSync, existsSync } from "node:fs";
import { join } from "node:path";

const EXE = "_tools/target/release/rock-sprout.exe";
const SRC = "_tools/_probe-filters-data";
const DATA = "_tools/_rowshot-data";
const OUT = join("_tmp", "ui-baseline", "after-018");
const WIDTHS = [1700, 1920];
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
mkdirSync(DATA, { recursive: true });
mkdirSync(OUT, { recursive: true });
if (existsSync(join(SRC, "cache"))) cpSync(join(SRC, "cache"), join(DATA, "cache"), { recursive: true });
if (existsSync("_tools/modio-token.txt")) copyFileSync("_tools/modio-token.txt", join(DATA, "modio-token.txt"));

const child = spawn(EXE, [], {
  env: {
    ...process.env,
    ROCK_SPROUT_DATA: DATA,
    // 断网：让搜索结果走缓存回放，界面里才有行
    HTTPS_PROXY: "http://127.0.0.1:9",
    ALL_PROXY: "http://127.0.0.1:9",
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9222",
  },
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
await sleep(900);

// 进矿脉，直接点「搜索」取热门（缓存里就有）
await evaluate(`document.querySelectorAll('.nav-item')[1].click()`);
await sleep(500);
await evaluate(`[...document.querySelectorAll('button')].find((b) => b.textContent.trim() === '搜索').click()`);
for (let i = 0; i < 40; i++) {
  const rows = await evaluate(`document.querySelectorAll('.vlist .row').length`);
  if (rows > 3) break;
  await sleep(300);
}

const rows = [];
for (const w of WIDTHS) {
  spawnSync(
    "powershell",
    [
      "-NoProfile",
      "-Command",
      `Add-Type -Namespace W -Name U -MemberDefinition '[DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hWnd, int x, int y, int w, int h, bool repaint);'; $p = Get-Process rock-sprout | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1; [W.U]::MoveWindow($p.MainWindowHandle, 40, 40, ${w}, 900, $true)`,
    ],
    { encoding: "utf8" },
  );
  await sleep(900);
  const m = await evaluate(`(() => {
    const rows = [...document.querySelectorAll('.vlist .row')].filter((r) => r.getBoundingClientRect().height > 0);
    if (!rows.length) return { rows: 0 };
    const first = rows[0];
    const main = first.querySelector('.row-main');
    const side = first.querySelector('.row-side');
    const title = first.querySelector('.row-title');
    const install = [...first.querySelectorAll('.row-side button')].pop();
    const r = (el) => (el ? el.getBoundingClientRect() : null);
    const box = (el) => (el ? { left: Math.round(r(el).left), right: Math.round(r(el).right), width: Math.round(r(el).width) } : null);
    return {
      rows: rows.length,
      viewport: document.documentElement.clientWidth,
      horizontalScroll: document.documentElement.scrollWidth > document.documentElement.clientWidth,
      list: box(document.querySelector('.vlist')),
      main: box(main),
      title: box(title),
      side: box(side),
      install: box(install),
      installH: install ? Math.round(r(install).height) : null,
    };
  })()`);
  rows.push({ width: w, ...m });
  const shot = await send("Page.captureScreenshot", { format: "png" });
  const { writeFileSync } = await import("node:fs");
  writeFileSync(join(OUT, `mine-rows-${w}.png`), Buffer.from(shot.result.data, "base64"));
}

console.log(JSON.stringify(rows, null, 1));
sock.close();
spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
