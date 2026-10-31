// M8-2 的四档宽度取证：1100 / 1400 / 1700 / 1920，每档截四页 + 记侧栏/列表/卡片的实际宽度。
//   node _tools/_shot-widths.mjs
// 真窗口用 Win32 MoveWindow 改大小（不是模拟视口），所以看到的就是真机效果。
import { spawn, spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const EXE = "_tools/target/release/rock-sprout.exe";
const DATA = "_tools/_width-data";
const OUT = join("_tmp", "ui-baseline", "after-018");
const WIDTHS = [1100, 1400, 1700, 1920];
const HEIGHT = 900;
const PAGES = [
  ["today", 0],
  ["mine", 1],
  ["bag", 2],
  ["settings", 3],
];
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
mkdirSync(DATA, { recursive: true });
mkdirSync(OUT, { recursive: true });

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
await sleep(1000);

function resize(w, h) {
  const ps = spawnSync(
    "powershell",
    [
      "-NoProfile",
      "-Command",
      `Add-Type -Namespace W -Name U -MemberDefinition '[DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hWnd, int x, int y, int w, int h, bool repaint);'; $p = Get-Process rock-sprout | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1; [W.U]::MoveWindow($p.MainWindowHandle, 40, 40, ${w}, ${h}, $true)`,
    ],
    { encoding: "utf8" },
  );
  if (ps.status !== 0) throw new Error(`改窗口大小失败：${ps.stderr || ps.stdout}`);
}

const report = [];
for (const w of WIDTHS) {
  resize(w, HEIGHT);
  await sleep(900);
  for (const [name, index] of PAGES) {
    await evaluate(`document.querySelectorAll('.nav-item')[${index}].click()`);
    await sleep(700);
    const metrics = await evaluate(`(() => {
      const de = document.documentElement;
      const page = document.querySelector('[data-page]:not([hidden])');
      const list = document.querySelector('[data-page]:not([hidden]) .list, [data-page]:not([hidden]) .vlist, [data-page]:not([hidden]) .today-block, [data-page]:not([hidden]) .empty');
      const card = document.querySelector('[data-page]:not([hidden]) .card');
      const rail = document.querySelector('.rail');
      return {
        viewport: de.clientWidth,
        scrollWidth: de.scrollWidth,
        horizontalScroll: de.scrollWidth > de.clientWidth,
        railW: rail ? Math.round(rail.getBoundingClientRect().width) : null,
        pageW: page ? Math.round(page.getBoundingClientRect().width) : null,
        listW: list ? Math.round(list.getBoundingClientRect().width) : null,
        cardW: card ? Math.round(card.getBoundingClientRect().width) : null,
        cardLeft: card ? Math.round(card.getBoundingClientRect().left) : null,
      };
    })()`);
    report.push({ width: w, page: name, ...metrics });
    const shot = await send("Page.captureScreenshot", { format: "png" });
    writeFileSync(join(OUT, `${name}-${w}.png`), Buffer.from(shot.result.data, "base64"));
  }
}

console.log(JSON.stringify(report, null, 1));
sock.close();
spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
