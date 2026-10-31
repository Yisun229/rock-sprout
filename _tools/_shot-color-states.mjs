// M8-3 逐像素比对的取图工具：把受颜色改动影响的每个界面状态各截一张。
//   node _tools/_shot-color-states.mjs before   ← 改之前拍
//   node _tools/_shot-color-states.mjs after    ← 改之后拍
// 同一视口、同一份缓存数据、每次滚到顶，diff 才干净。
import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, cpSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const LABEL = process.argv[2] ?? "before";
const EXE = "_tools/target/release/rock-sprout.exe";
const CACHE_SRC = "_tools/_probe-filters-data/cache";
const DATA = "_tools/_color-data";
const OUT = join("_tmp", "color", LABEL);
const W = 1400;
const H = 900;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
mkdirSync(DATA, { recursive: true });
mkdirSync(OUT, { recursive: true });
if (existsSync(CACHE_SRC)) cpSync(CACHE_SRC, join(DATA, "cache"), { recursive: true });
if (existsSync("_tools/modio-token.txt")) copyFileSync("_tools/modio-token.txt", join(DATA, "modio-token.txt"));

const child = spawn(EXE, [], {
  env: {
    ...process.env,
    ROCK_SPROUT_DATA: DATA,
    // 统一断网：让「当前离线」横幅、缓存回放、装不了的失败都稳定出现
    HTTPS_PROXY: "http://127.0.0.1:9",
    ALL_PROXY: "http://127.0.0.1:9",
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9222",
  },
  detached: true,
  stdio: "ignore",
});
child.unref();

const setWindow = (w, h) =>
  spawnSync(
    "powershell",
    [
      "-NoProfile",
      "-Command",
      `Add-Type -Namespace W -Name U -MemberDefinition '[DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hWnd, int x, int y, int w, int h, bool repaint);'; $p = Get-Process rock-sprout | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1; [W.U]::MoveWindow($p.MainWindowHandle, 40, 40, ${w}, ${h}, $true)`,
    ],
    { encoding: "utf8" },
  );

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
const evaluate = async (expr) => {
  const res = await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true });
  if (res.result?.exceptionDetails) throw new Error(res.result.exceptionDetails.exception?.description ?? "页面里报错");
  return res.result?.result?.value;
};
const shot = async (name) => {
  // 截图前把「会自己动」的东西按住：滚动条拇指、输入框光标、焦点描边。
  // 换 token 是零视觉变化，这些噪声留着会让 diff 分不清是改坏了还是没对齐。
  await evaluate(`(() => { const a = document.activeElement; if (a && a.blur) a.blur(); return true; })()`);
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 2, y: 2, button: "none" });
  await sleep(450);
  const res = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(OUT, `${name}.png`), Buffer.from(res.result.data, "base64"));
};
const moveTo = async (selector) => {
  const box = await evaluate(`(() => {
    const el = document.querySelector(${JSON.stringify(selector)});
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) };
  })()`);
  if (!box) return false;
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: box.x, y: box.y, button: "none" });
  return true;
};
const click = async (selector) => {
  await evaluate(`document.querySelector(${JSON.stringify(selector)}).click()`);
};
const scrollTop = () => evaluate(`(() => { const s = document.querySelector('.stage'); if (s) s.scrollTop = 0; window.scrollTo(0, 0); return true; })()`);

for (let i = 0; i < 100; i++) {
  if (await evaluate("!!window.__TAURI_INTERNALS__")) break;
  await sleep(300);
}
// 滚动条是有状态的（拇指位置随内容高度变），它会污染逐像素比对。
// 两边都关掉，比分的是颜色和布局，不是滚动条状态。
await evaluate(`(() => {
  const st = document.createElement('style');
  st.textContent = '::-webkit-scrollbar{width:0!important;height:0!important} *{scrollbar-width:none!important}';
  document.head.appendChild(st);
  return true;
})()`);
setWindow(W, H);
await sleep(1200);

// ── 矿脉：满（有行）+ 离线横幅
await evaluate(`document.querySelectorAll('.nav-item')[1].click()`);
await sleep(600);
await evaluate(`[...document.querySelectorAll('button')].find((b) => b.textContent.trim() === '搜索').click()`);
for (let i = 0; i < 40; i++) {
  if ((await evaluate(`document.querySelectorAll('.vlist .row').length`)) > 3) break;
  await sleep(300);
}
await scrollTop();
await shot("mine-full");

// ── 矿脉：鼠标停在「装」上（.btn:hover 的底色）
await moveTo(".vlist .row .row-side button:last-child");
await shot("mine-hover-install");

// ── 矿脉：筛选面板（.filter-panel 阴影）
await evaluate(`[...document.querySelectorAll('.filter')][0].querySelector('.btn').click()`);
await sleep(400);
await shot("filter-open");
await evaluate(`document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }))`);
await sleep(300);

// ── 抽屉（宽屏：.drawer / .drawer-card）
await evaluate(`document.querySelector('.vlist .row').click()`);
for (let i = 0; i < 40; i++) {
  if (await evaluate(`!!document.querySelector('.mod-preview .preview-title, .mod-preview h2, .mod-preview .preview-name')`)) break;
  await sleep(300);
}
await shot("drawer-wide");
await evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))`);
await sleep(400);

// ── 抽屉（窄屏：遮罩 .preview-mask）
setWindow(900, H);
await sleep(900);
await scrollTop();
await evaluate(`document.querySelector('.vlist .row').click()`);
await sleep(1200);
await shot("drawer-narrow");
await evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))`);
await sleep(400);
setWindow(W, H);
await sleep(900);

// ── 队列：断网下点装，出现失败条目（.queue-item 文字色）
await evaluate(`document.querySelector('.vlist .row .row-side button:last-child').click()`);
await sleep(2500);
await scrollTop();
await shot("queue-failed");

// ── 设置：诊断块（.diag 底色）+ 分组标题 + 禁用按钮
await evaluate(`document.querySelectorAll('.nav-item')[3].click()`);
await sleep(800);
await evaluate(`(() => { const d = document.querySelector('.diag'); if (d) d.scrollIntoView({ block: 'center' }); return true; })()`);
await shot("settings-diag");
await evaluate(`(() => { const s = document.querySelector('.stage'); if (s) s.scrollTop = s.scrollHeight; return true; })()`);
await sleep(400);
await shot("settings-bottom");

// ── 轻提示（.toast 阴影）
await evaluate(`window.dispatchEvent(new CustomEvent('rs:toast', { detail: '已在浏览器打开' }))`);
await shot("toast");

console.log(`已截图到 ${OUT}`);
sock.close();
spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
