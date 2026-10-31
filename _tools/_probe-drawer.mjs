// 抽屉宽度与让位是否“跟着算”：在四档宽度下把 with-preview 挂上去，读实际像素。
//   node _tools/_probe-drawer.mjs
import { spawn, spawnSync } from "node:child_process";
import { mkdirSync } from "node:fs";

const EXE = "_tools/target/release/rock-sprout.exe";
const DATA = "_tools/_drawer-data";
const WIDTHS = [1100, 1400, 1700, 1920];
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
await sleep(900);
await evaluate(`document.querySelectorAll('.nav-item')[1].click()`);
await sleep(500);

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
  await sleep(800);
  const m = await evaluate(`(() => {
    const mine = document.querySelector('[data-page="mine"] section') ?? document.querySelector('[data-page="mine"]');
    mine.classList.add('with-preview');
    const fake = document.createElement('div');
    fake.className = 'mod-preview';
    document.body.appendChild(fake);
    const drawerW = Math.round(fake.getBoundingClientRect().width);
    const padRight = Math.round(parseFloat(getComputedStyle(mine).paddingRight));
    const rowRight = (() => {
      const row = mine.querySelector('.empty') ?? mine;
      return row ? Math.round(row.getBoundingClientRect().right) : null;
    })();
    fake.remove();
    mine.classList.remove('with-preview');
    return { viewport: document.documentElement.clientWidth, drawerW, padRight, contentRight: rowRight };
  })()`);
  rows.push({ window: w, ...m });
}

console.log(JSON.stringify(rows, null, 1));
sock.close();
spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
