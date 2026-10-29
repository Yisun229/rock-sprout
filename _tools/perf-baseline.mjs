// 量几个数，写进 _tools/perf-baseline.txt。不是回归，是尺子：每版做完再量一遍做对比。
//
//   node _tools/perf-baseline.mjs
//
// 量的事：
//   1. 冷启动到界面可用（起 3 次取中位数）
//   2. 空闲内存：只起界面不动，5 秒与 30 秒各读一次占用
//   3. 装一个 mod 的耗时（本地 zip，三次取中位数）
//   4. 1000 行列表的 DOM 渲染耗时（用页面里真实的 .row 样式，插到列表容器旁边量，不含 React 调度）
//   5. 已装 50 个时「矿脉 ↔ 背包」切页耗时（点导航到列表出现）
//   6. 1000 条搜索结果的滚动（真实虚拟列表）
import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

const TOOLS = import.meta.dirname;
const ROOT = dirname(TOOLS);
const EXE = join(TOOLS, "target/release/rock-sprout.exe");
const PEEK = join(TOOLS, "target/release/peek.exe");
const FIXTURE = join(TOOLS, "fixtures/small-mod.zip");
const OUT = join(TOOLS, "perf-baseline.txt");
const DATA = join(TOOLS, "_perfdata");
const DATA_INSTALL = join(TOOLS, "_perfdata-install");
const CDP = "http://127.0.0.1:9222/json";
const GAME_DLL = "D:/Steam/steamapps/common/Deep Rock Galactic/FSD/Binaries/Win64/x3daudio1_7.dll";
const STOCK_DLL = join(TOOLS, "game-backup/2026-10-28/FSD/Binaries/Win64/x3daudio1_7.dll");

function killApp() {
  spawnSync("taskkill", ["/IM", "rock-sprout.exe", "/F"], { stdio: "ignore" });
}

/** 把「我们这个程序」相关的进程都读出来：自家进程 + 它拉起来的 WebView2 子进程。
 *
 * 为什么不能只按进程名加总：WebView2 的进程名（msedgewebview2.exe）是公用的，
 * 这台机器上别的程序也在用，直接加总会把别人的算进来。所以按父子链从
 * rock-sprout.exe 往下走，只算真正的后代。
 */
function appMemory() {
  const ps = [
    "-NoProfile",
    "-Command",
    "Get-CimInstance Win32_Process -Filter \"Name='msedgewebview2.exe' or Name='rock-sprout.exe'\" | " +
      "Select-Object ProcessId,ParentProcessId,Name,WorkingSetSize | ConvertTo-Json -Compress",
  ];
  const out = spawnSync("powershell", ps, { encoding: "utf8", windowsHide: true });
  if (out.status !== 0 || !out.stdout) return null;
  let rows;
  try {
    rows = JSON.parse(out.stdout);
  } catch {
    return null;
  }
  if (!Array.isArray(rows)) rows = [rows];
  rows = rows.filter((r) => r && r.ProcessId);

  const mine = rows.filter((r) => r.Name === "rock-sprout.exe").map((r) => r.ProcessId);
  if (mine.length === 0) return null;

  const children = new Map();
  for (const r of rows) {
    if (!children.has(r.ParentProcessId)) children.set(r.ParentProcessId, []);
    children.get(r.ParentProcessId).push(r);
  }

  let ownKb = 0;
  let ownCount = 0;
  let wvKb = 0;
  let wvCount = 0;
  const queue = [...mine];
  const seen = new Set();
  while (queue.length > 0) {
    const pid = queue.shift();
    if (seen.has(pid)) continue;
    seen.add(pid);
    const self = rows.find((r) => r.ProcessId === pid);
    if (self) {
      const kb = (self.WorkingSetSize ?? 0) / 1024;
      if (self.Name === "rock-sprout.exe") {
        ownKb += kb;
        ownCount += 1;
      } else {
        wvKb += kb;
        wvCount += 1;
      }
    }
    for (const child of children.get(pid) ?? []) queue.push(child.ProcessId);
  }
  return { ownKb, ownCount, wvKb, wvCount };
}

function memText(m) {
  if (!m) return "没读到";
  const own = Math.round(m.ownKb / 1024);
  const wv = Math.round(m.wvKb / 1024);
  return (
    `${own} MB + WebView2 ${wv} MB ≈ 合计 ${own + wv} MB` +
    `（自家 ${m.ownCount} 个进程，WebView2 ${m.wvCount} 个）`
  );
}

/** 把游戏目录复原（装了测试 mod 之后用）。 */
function restoreGameDir() {
  if (existsSync(PEEK)) spawnSync(PEEK, ["clear"], { stdio: "ignore" });
  if (existsSync(GAME_DLL) && existsSync(STOCK_DLL)) {
    const now = statSync(GAME_DLL).size;
    const orig = statSync(STOCK_DLL).size;
    if (now !== orig) copyFileSync(STOCK_DLL, GAME_DLL);
  }
}

async function startApp(env) {
  const t0 = Date.now();
  const child = spawn(EXE, [], {
    cwd: ROOT,
    detached: true,
    stdio: "ignore",
    env: {
      ...process.env,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9222",
      ...env,
    },
  });
  child.unref();
  for (let i = 0; i < 150; i++) {
    try {
      const targets = await (await fetch(CDP)).json();
      const page = targets.find((t) => t.url.startsWith("http://tauri.localhost"));
      if (page) return { readyMs: Date.now() - t0, wsUrl: page.webSocketDebuggerUrl };
    } catch {
      // 继续等
    }
    await sleep(100);
  }
  killApp();
  throw new Error("应用 15 秒内没起来");
}

async function connect(wsUrl) {
  const ws = new WebSocket(wsUrl);
  let id = 0;
  const pending = new Map();
  ws.addEventListener("message", (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
      pending.get(msg.id)(msg);
      pending.delete(msg.id);
    }
  });
  await new Promise((resolve) => ws.addEventListener("open", resolve));
  const send = (method, params) =>
    new Promise((resolve) => {
      const myId = ++id;
      pending.set(myId, resolve);
      ws.send(JSON.stringify({ id: myId, method, params }));
    });
  async function evalJs(expression) {
    const res = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    const det = res.result?.exceptionDetails;
    if (det) throw new Error("页面里报错：" + (det.exception?.description ?? JSON.stringify(det)));
    return res.result?.result?.value;
  }
  // 等页面就绪（否则没有 __TAURI_INTERNALS__）
  for (let i = 0; i < 100; i++) {
    const ok = await evalJs(
      "!!(window.__TAURI_INTERNALS__ && document.readyState === 'complete' && document.querySelector('button'))",
    ).catch(() => false);
    if (ok) break;
    await sleep(150);
  }
  return { evalJs, close: () => ws.close() };
}

function median(nums) {
  const s = [...nums].sort((a, b) => a - b);
  const mid = Math.floor(s.length / 2);
  return s.length % 2 ? s[mid] : Math.round((s[mid - 1] + s[mid]) / 2);
}

if (!EXE) throw new Error("没有 exe");

killApp();
rmSync(DATA, { recursive: true, force: true });
mkdirSync(join(DATA, "mods/installed"), { recursive: true });

// 造 50 条假记账（文件不存在也没关系：已装列表读的是记账，不校验文件在不在）
for (let i = 0; i < 50; i++) {
  const rec = {
    id: 900000 + i,
    name: `性能测试 mod ${i + 1}`,
    file_id: 0,
    filename: `perf-${i + 1}.zip`,
    size: 1024 * (i + 1),
    path: `E:/nowhere/perf-${i + 1}.zip`,
    source: "local",
  };
  writeFileSync(join(DATA, `mods/installed/${rec.id}.json`), JSON.stringify(rec, null, 2));
}

// ---- 1. 冷启动（3 次）----
const coldStarts = [];
for (let i = 0; i < 3; i++) {
  killApp();
  await sleep(800);
  const app = await startApp({ ROCK_SPROUT_DATA: DATA });
  coldStarts.push(app.readyMs);
  if (i < 2) {
    killApp();
  }
}
const coldMs = median(coldStarts);
console.log(`冷启动到界面可用：${coldStarts.join(" / ")} ms，中位数 ${coldMs} ms`);

// ---- 2. 空闲内存：界面起来后什么都不点，5 秒与 30 秒各读一次 ----
// 自家进程 + 它拉起来的 WebView2 子进程（按父子链算，不算别人的）。
await sleep(5000);
const m5 = appMemory();
await sleep(25000);
const m30 = appMemory();
console.log(`空闲内存：5 秒 ${memText(m5)}，30 秒 ${memText(m30)}`);

// ---- 3. 装一个 mod 的耗时：本地 zip 从发起装到界面出现这一条 ----
// 走的是界面同一个命令（import_local）：复制文件 → 合并进游戏 → 记账 → 列表刷新。
// 三次取中位数；每次装的都是同一份东西，所以三次的合并量一样。
killApp();
await sleep(500);
rmSync(DATA_INSTALL, { recursive: true, force: true });
const installRuns = [];
let installNote = "";
{
  const app = await startApp({ ROCK_SPROUT_DATA: DATA_INSTALL });
  const si = await connect(app.wsUrl);
  for (let i = 0; i < 3; i++) {
    const one = await si.evalJs(`(async () => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      const t0 = performance.now();
      const res = await invoke("import_local", { path: ${JSON.stringify(FIXTURE)} });
      const t1 = performance.now();
      // 切到背包，等这一条真的出现在界面上（装完自动刷新，几乎不用我们再点）
      const nav = [...document.querySelectorAll("button")].find((b) => b.innerText.includes("背包"));
      if (nav) nav.click();
      const bag = document.querySelector('main [data-page="bag"]');
      const t2 = performance.now();
      let appeared = -1;
      for (let k = 0; k < 200; k++) {
        const n = bag ? bag.querySelectorAll(".row").length : 0;
        if (bag && bag.hidden === false && n > 0) {
          appeared = performance.now() - t2;
          break;
        }
        await new Promise((r) => setTimeout(r, 16));
      }
      return {
        ok: res && res.ok,
        error: res && res.error && res.error.message,
        work: Math.round(t1 - t0),
        appear: Math.round(appeared),
      };
    })()`);
    if (!one.ok) installNote = `第 ${i + 1} 次没装成：${one.error}`;
    installRuns.push(one.work + Math.max(0, one.appear));
  }
  si.close();
  killApp();
}
const installMs = median(installRuns);
console.log(`装一个 mod：${installRuns.join(" / ")} ms，中位数 ${installMs} ms${installNote ? "（" + installNote + "）" : ""}`);
// 装的那个 mod 写进了游戏目录，量完立刻复原
restoreGameDir();
await sleep(500);

// ---- 4 & 5：重新起一个窗口量（前面装 mod 那轮把窗口关了）----
const appS = await startApp({ ROCK_SPROUT_DATA: DATA });
const s = await connect(appS.wsUrl);

// 1000 行 DOM 渲染
const render1000 = await s.evalJs(`(() => {
  const host = document.createElement("div");
  host.className = "list";
  host.style.position = "absolute";
  host.style.left = "-99999px";
  host.style.width = "600px";
  document.body.appendChild(host);
  const frag = document.createDocumentFragment();
  const t0 = performance.now();
  for (let i = 0; i < 1000; i++) {
    const li = document.createElement("li");
    li.className = "row";
    li.innerHTML = '<div class="row-main"><div class="row-title">性能测试 mod ' + i +
      '</div><div class="row-sub">这是一段说明文字，用来模拟真实的卡片内容</div></div>' +
      '<div class="row-side">↓ 12345</div>';
    frag.appendChild(li);
  }
  host.appendChild(frag);
  // 读一次布局，把「排完版」的成本也算进去，不然只量到 DOM 构建
  void host.offsetHeight;
  const ms = performance.now() - t0;
  const rows = host.querySelectorAll(".row").length;
  host.remove();
  return { ms: Math.round(ms * 10) / 10, rows };
})()`);
console.log(`1000 行渲染（DOM 层）：${render1000.ms} ms，插了 ${render1000.rows} 行`);

// 已装 50 个时切页：矿脉 → 背包 → 矿脉
const switchMs = await s.evalJs(`(async () => {
  const click = (label) => {
    const el = [...document.querySelectorAll("button")].find((b) => b.innerText.includes(label));
    if (!el) throw new Error("找不到导航：" + label);
    el.click();
  };
  // 三块页面都挂在 DOM 里、用 hidden 藏，所以按 data-page 精确看：那块露出来了吗、它自己有几行
  const waitFor = async (pageKey, rows, limit = 3000) => {
    const t0 = performance.now();
    while (performance.now() - t0 < limit) {
      const slot = document.querySelector('main [data-page="' + pageKey + '"]');
      const n = slot ? slot.querySelectorAll(".row").length : 0;
      if (slot && slot.hidden === false && n >= rows) return performance.now() - t0;
      await new Promise((r) => setTimeout(r, 16));
    }
    return -1;
  };
  const out = {};
  click("背包");
  out.toBackpack = Math.round(await waitFor("bag", 50));
  click("矿脉");
  out.toMineVein = Math.round(await waitFor("mine", 0));
  return out;
})()`);
console.log(`切页（已装 50 个）：切到背包 ${switchMs.toBackpack} ms，切回矿脉 ${switchMs.toMineVein} ms`);

s.close();
killApp();
await sleep(500);

// ---- 4. 1000 条搜索结果的滚动（走真实列表，靠缓存喂数据，不碰网络） ----
// 造一份「1000 个 mod」的搜索缓存，然后断网启动：后端读不到 mod.io 就回放这份缓存，
// 这样量到的是真的 React 列表，而不是临时插进去的 DOM。
function fnv1a32(text) {
  let h = 0x811c9dc5;
  for (const byte of Buffer.from(text, "utf8")) {
    h ^= byte;
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, "0");
}

const QUERY = "性能";
// 文件名规则跟后端 cache.rs 一样：字母/数字/`-`/`_` 留着，其它换成 `_`，最多 32 个字符。
// 后端用的是 Rust 的 char::is_alphanumeric，汉字算字母，所以「性能」会原样留下。
const safe = (() => {
  const kept = Array.from(QUERY.trim())
    .map((ch) => (/[\p{L}\p{N}_-]/u.test(ch) ? ch : "_"))
    .slice(0, 32)
    .join("");
  return kept === "" ? "热门" : kept;
})();
const cacheDir = join(DATA, "cache/search");
mkdirSync(cacheDir, { recursive: true });

// 搜索前要先有令牌（后端先查令牌再发请求），这里把真的令牌文件拷一份过来；
// 断网时后端只走到「连不上」这一步，回放缓存，所以令牌内容不会真的发出去。
const tokenSrc = join(TOOLS, "modio-token.txt");
if (existsSync(tokenSrc)) copyFileSync(tokenSrc, join(DATA, "modio-token.txt"));
const fakeItems = Array.from({ length: 1000 }, (_, i) => ({
  id: 800000 + i,
  name: `性能测试 mod ${i + 1}`,
  summary: "这是一段说明文字，用来模拟真实的卡片内容，好不好看就看这里",
  downloads: 100000 - i * 37,
}));
writeFileSync(
  join(cacheDir, `${safe}-${fnv1a32(QUERY.trim())}-p1.json`),
  JSON.stringify({
    cached_at: Date.now(),
    value: { query: QUERY, page: 1, total: 1000, items: fakeItems, stale: false, cached_at: null },
  }),
);

const app2 = await startApp({
  ROCK_SPROUT_DATA: DATA,
  HTTPS_PROXY: "http://127.0.0.1:9",
  ALL_PROXY: "http://127.0.0.1:9",
});
const s2 = await connect(app2.wsUrl);
await s2.evalJs(`(() => {
  const input = document.querySelector('input.input');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
  setter.call(input, ${JSON.stringify(QUERY)});
  input.dispatchEvent(new Event('input', { bubbles: true }));
  return true;
})()`);

// 等列表出现（等的是真的 .vlist 里那几条）
for (let i = 0; i < 100; i++) {
  const ready = await s2.evalJs("document.querySelectorAll('.vlist .row').length > 0");
  if (ready) break;
  await sleep(150);
}

const virtual = await s2.evalJs(`(async () => {
  const box = document.querySelector('.vlist');
  if (!box) return { error: '页面上没有 .vlist' };
  const rendered = box.querySelectorAll('.row').length;
  const total = box.querySelector('.vlist-inner')?.style.height ?? '';
  // 从头滚到底，一帧一步，记下每帧间隔
  const frames = [];
  const step = Math.max(1, Math.round((box.scrollHeight - box.clientHeight) / 60));
  let last = performance.now();
  for (let y = 0; y <= box.scrollHeight; y += step) {
    box.scrollTop = y;
    await new Promise((r) => requestAnimationFrame(r));
    const now = performance.now();
    frames.push(now - last);
    last = now;
  }
  box.scrollTop = 0;
  frames.shift();
  const avg = frames.reduce((a, b) => a + b, 0) / Math.max(1, frames.length);
  return {
    rendered,
    total,
    frames: frames.length,
    avg: Math.round(avg * 10) / 10,
    worst: Math.round(Math.max(...frames) * 10) / 10,
  };
})()`);
if (virtual.error) throw new Error(virtual.error);
console.log(
  `1000 条滚动（真实列表）：DOM 里只有 ${virtual.rendered} 行，内层高度 ${virtual.total}，` +
    `滚 ${virtual.frames} 帧，平均 ${virtual.avg} ms/帧，最差 ${virtual.worst} ms`,
);

s2.close();
killApp();
await sleep(500);

// 输出文件第一行写个标签，方便对比「改之前 / 改之后」。用法：
//   node _tools/perf-baseline.mjs                 → 「v0.1.3 瘦身之前」
//   node _tools/perf-baseline.mjs 瘦身之后         → 「v0.1.3 瘦身之后」
const LABEL = process.argv[2] ? `v0.1.3 ${process.argv[2]}` : "v0.1.3 瘦身之前";

const lines = [
  `量于 ${new Date().toLocaleString("zh-CN")}（${LABEL}）`,
  "",
  `冷启动到界面可用：${coldMs} ms（三次：${coldStarts.join(" / ")} ms）目标 < 1500 ms`,
  `空闲内存（只起界面不操作）：5 秒 ${memText(m5)}；30 秒 ${memText(m30)}。目标 < 150 MB`,
  `装一个 mod（本地 zip → 合并进游戏 → 界面出现）：${installMs} ms` +
    `（三次：${installRuns.join(" / ")} ms）${installNote ? "，注意：" + installNote : ""}`,
  `1000 行渲染（DOM 层，不含 React 调度）：${render1000.ms} ms`,
  `已装 50 个切页：矿脉→背包 ${switchMs.toBackpack} ms，背包→矿脉 ${switchMs.toMineVein} ms`,
  `1000 条搜索结果滚动（真实虚拟列表）：DOM 里 ${virtual.rendered} 行 / 共 1000 条，` +
    `内层高度 ${virtual.total}，滚 ${virtual.frames} 帧，平均 ${virtual.avg} ms/帧，最差 ${virtual.worst} ms`,
  "",
  "说明：",
  "  - 冷启动＝起进程到调试端口出现且页面 DOM 就绪（含 WebView2 初始化）。",
  "  - 空闲内存＝界面起来后不点任何东西，用 PowerShell 读进程工作集，沿父子链只算「我们这个程序」的：",
  "    自家进程 rock-sprout.exe＋它拉起来的 WebView2 子进程（msedgewebview2.exe 这个进程名是公用的，",
  "    别的程序也在用，所以按父子链筛，不按名字加总）。两块加起来才是任务管理器里看到的占用。",
  "  - 装一个 mod 走的是界面同一个命令（import_local）：复制文件 → 合并进游戏 → 记账 → 列表刷新；",
  "    含第一次合并的一次性开销，三次都是同一份夹具所以可比。",
  "  - 1000 行渲染是直接往真实 .row 样式里插 DOM 量的，量的是布局+样式成本，不是 React 的成本；",
  "    要看 React + 虚拟滚动的真实数字，看最后一行。",
  "  - 切页是点导航到列表行数出现的时间，含一次 list_installed 往返。",
  "  - 最后一行靠写一份假的搜索缓存 + 断网启动拿到 1000 条，全程不访问 mod.io。",
];
mkdirSync(TOOLS, { recursive: true });
writeFileSync(OUT, lines.join("\n") + "\n", "utf8");
console.log(`\n写进 ${OUT}：\n` + lines.join("\n"));
rmSync(DATA, { recursive: true, force: true });
rmSync(DATA_INSTALL, { recursive: true, force: true });
