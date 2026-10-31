// M8-3 交付②：三页的「空 / 错 / 满」各截一张，动作按钮按约定出现。
//   node _tools/_shot-3states.mjs
// 输出到 _tmp/ui-baseline/states-018/，并把每张里出现的按钮文字打出来（这才是验收点）。
import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, cpSync, existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const EXE = "rock-sprout.exe";
const TARGET = "_tools/target/release";
const ROOT = "_tools/_states-data";
const OUT = join("_tmp", "ui-baseline", "states-018");
const W = 1400;
const H = 900;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
mkdirSync(OUT, { recursive: true });

const setWindow = (w, h) =>
  spawnSync(
    "powershell",
    [
      "-NoProfile",
      "-Command",
      `Add-Type -Namespace W -Name U -MemberDefinition '[DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hWnd, int x, int y, int w, int h, bool repaint);'; $p = Get-Process rock-sprout | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1; if ($p) { [W.U]::MoveWindow($p.MainWindowHandle, 40, 40, ${w}, ${h}, $true) }`,
    ],
    { encoding: "utf8" },
  );

async function withApp(job) {
  spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
  const data = job.data ?? ROOT;
  mkdirSync(data, { recursive: true });
  if (job.seed) job.seed(data);

  const env = {
    ...process.env,
    ROCK_SPROUT_DATA: data,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9222",
  };
  if (job.offline) {
    env.HTTPS_PROXY = "http://127.0.0.1:9";
    env.ALL_PROXY = "http://127.0.0.1:9";
  }
  const child = spawn(join(TARGET, EXE), [], { env, detached: true, stdio: "ignore" });
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
  for (let i = 0; i < 100; i++) {
    if (await ev("!!window.__TAURI_INTERNALS__")) break;
    await sleep(300);
  }
  await ev(`(() => { const st = document.createElement('style'); st.textContent = '::-webkit-scrollbar{width:0!important;height:0!important} *{scrollbar-width:none!important}'; document.head.appendChild(st); return true; })()`);
  setWindow(W, H);
  await sleep(1200);

  const shot = async (name) => {
    await ev(`(() => { const a = document.activeElement; if (a && a.blur) a.blur(); return true; })()`);
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 2, y: 2, button: "none" });
    await sleep(400);
    const res = await send("Page.captureScreenshot", { format: "png" });
    writeFileSync(join(OUT, `${name}.png`), Buffer.from(res.result.data, "base64"));
    const buttons = await ev(`(() => {
      const scope = document.querySelector('.card.fail, .empty, .today-warn')?.closest('section') ?? document.querySelector('section');
      const texts = [...(scope?.querySelectorAll('button') ?? [])].map((b) => b.textContent.trim()).filter(Boolean);
      const empty = [...document.querySelectorAll('.empty')].map((e) => e.textContent.replace(/\\s+/g, ' ').trim());
      const fail = [...document.querySelectorAll('.card.fail .fail-title')].map((e) => e.textContent.trim());
      return { buttons: texts, empty, fail };
    })()`);
    return buttons;
  };

  const results = [];
  try {
    results.push(...(await job.run(ev, shot)));
  } finally {
    sock.close();
    spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
  }
  return results;
}

const go = async (ev, index) => {
  const name = ["今天", "矿脉", "背包", "设置"][index];
  await ev(`(() => { const b = [...document.querySelectorAll('.nav-item')].find((x) => x.textContent.trim() === ${JSON.stringify(name)}); if (b) b.click(); return !!b; })()`);
  await sleep(700);
};
const clickText = async (ev, text) => {
  await ev(`(() => { const b = [...document.querySelectorAll('button')].find((x) => x.textContent.trim() === ${JSON.stringify(text)}); if (b) b.click(); return !!b; })()`);
};

const all = [];

// ── 1. 全新的数据目录：矿脉没搜过（看热门）、背包一个没装（去矿脉找一个）
all.push(
  ...(await withApp({
    data: join(ROOT, "fresh"),
    seed: (data) => {
      rmSync(data, { recursive: true, force: true });
      mkdirSync(data, { recursive: true });
      if (existsSync("_tools/modio-token.txt")) copyFileSync("_tools/modio-token.txt", join(data, "modio-token.txt"));
    },
    offline: true,
    run: async (ev, shot) => {
      await go(ev, 1);
      const mineIdle = await shot("01-mine-idle");
      await go(ev, 2);
      const bagEmpty = await shot("02-bag-empty");
      return [
        { shot: "01-mine-idle", ...mineIdle },
        { shot: "02-bag-empty", ...bagEmpty },
      ];
    },
  })),
);

// ── 2. 有缓存：矿脉满列表；再设筛选搜个搜不到的词 → 清空筛选
all.push(
  ...(await withApp({
    data: join(ROOT, "cache"),
    seed: (data) => {
      rmSync(data, { recursive: true, force: true });
      mkdirSync(data, { recursive: true });
      if (existsSync("_tools/_probe-filters-data/cache")) cpSync("_tools/_probe-filters-data/cache", join(data, "cache"), { recursive: true });
      if (existsSync("_tools/modio-token.txt")) copyFileSync("_tools/modio-token.txt", join(data, "modio-token.txt"));
    },
    offline: true,
    run: async (ev, shot) => {
      await go(ev, 1);
      await clickText(ev, "搜索");
      for (let i = 0; i < 40; i++) {
        if ((await ev(`document.querySelectorAll('.vlist .row').length`)) > 3) break;
        await sleep(300);
      }
      const mineFull = await shot("03-mine-full");

      // 打开类型面板勾一个标签，再搜一个肯定没有的词
      await ev(`[...document.querySelectorAll('.filter')][0].querySelector('.btn').click()`);
      await sleep(300);
      await ev(`[...document.querySelectorAll('.filter-panel .filter-item')][0].click()`);
      await sleep(300);
      await ev(`document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }))`);
      await ev(`(() => { const i = document.querySelector('.searchbar input'); const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set; setter.call(i, 'zzzzzzzz'); i.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`);
      await clickText(ev, "搜索");
      for (let i = 0; i < 60; i++) {
        const done = await ev(`(() => {
          const sec = [...document.querySelectorAll('section')].find((s) => s.offsetParent !== null && s.querySelector('.vlist, .empty'));
          if (!sec) return false;
          return !!sec.querySelector('.empty') || sec.querySelectorAll('.vlist .row').length > 0;
        })()`);
        if (done) break;
        await sleep(400);
      }
      await sleep(800);
      const mineNoHit = await shot("04-mine-empty-filtered");

      // 背包：导入一个本地包 → 满
      await go(ev, 2);
      await ev(`window.__TAURI_INTERNALS__.invoke('import_local', { path: '${join("E:", "岩笋 · Rock Sprout", "_tools", "fixtures", "small-mod.zip").replaceAll("\\", "\\\\")}' })`);
      for (let i = 0; i < 40; i++) {
        const n = await ev(`document.querySelectorAll('.bag-list .row, [data-page="bag"] .row').length`);
        if (n > 0) break;
        await sleep(400);
      }
      await sleep(600);
      const bagFull = await shot("05-bag-full");
      return [
        { shot: "03-mine-full", ...mineFull },
        { shot: "04-mine-empty-filtered", ...mineNoHit },
        { shot: "05-bag-full", ...bagFull },
      ];
    },
  })),
);

// ── 3. 断网 + 没缓存：今天页两块取不到（重试），矿脉没缓存 → 网络错误（重试）
all.push(
  ...(await withApp({
    data: join(ROOT, "offline"),
    seed: (data) => {
      rmSync(data, { recursive: true, force: true });
      mkdirSync(data, { recursive: true });
      if (existsSync("_tools/modio-token.txt")) copyFileSync("_tools/modio-token.txt", join(data, "modio-token.txt"));
    },
    offline: true,
    run: async (ev, shot) => {
      await sleep(3000);
      const today = await shot("06-today-offline");
      await go(ev, 1);
      await clickText(ev, "搜索");
      await sleep(3500);
      const net = await shot("07-mine-error-network");
      return [
        { shot: "06-today-offline", ...today },
        { shot: "07-mine-error-network", ...net },
      ];
    },
  })),
);

// ── 4. 令牌失效：401 → 只给「去设置页」，不给重试
all.push(
  ...(await withApp({
    data: join(ROOT, "badtoken"),
    seed: (data) => {
      rmSync(data, { recursive: true, force: true });
      mkdirSync(data, { recursive: true });
      writeFileSync(join(data, "modio-token.txt"), "0000000000000000000000000000000000000000000000000000000000000000");
    },
    run: async (ev, shot) => {
      await go(ev, 1);
      await clickText(ev, "搜索");
      await sleep(4000);
      const r = await shot("08-mine-error-401");
      return [{ shot: "08-mine-error-401", ...r }];
    },
  })),
);

// ── 5. 游戏在跑：起一个同名的假进程 → 装的时候被挡（只知道，不给重试）
all.push(
  ...(await withApp({
    data: join(ROOT, "gamerun"),
    seed: (data) => {
      rmSync(data, { recursive: true, force: true });
      mkdirSync(data, { recursive: true });
      if (existsSync("_tools/_probe-filters-data/cache")) cpSync("_tools/_probe-filters-data/cache", join(data, "cache"), { recursive: true });
      if (existsSync("_tools/modio-token.txt")) copyFileSync("_tools/modio-token.txt", join(data, "modio-token.txt"));
      // 拿系统里的 ping 冒充游戏进程（名字对就行，内核检查的是进程名）
      cpSync("C:\\Windows\\System32\\PING.EXE", join(ROOT, "FSD-Win64-Shipping.exe"), { force: true });
      spawn(join(ROOT, "FSD-Win64-Shipping.exe"), ["-n", "60", "127.0.0.1"], { detached: true, stdio: "ignore" }).unref();
    },
    offline: true,
    run: async (ev, shot) => {
      await go(ev, 1);
      await clickText(ev, "搜索");
      for (let i = 0; i < 40; i++) {
        if ((await ev(`document.querySelectorAll('.vlist .row').length`)) > 2) break;
        await sleep(300);
      }
      await ev(`document.querySelector('.vlist .row .row-side button:last-child').click()`);
      await sleep(3000);
      const r = await shot("09-mine-error-gamerunning");
      return [{ shot: "09-mine-error-gamerunning", ...r }];
    },
  })),
);

// ── 6. 不认识的错误：把 search_mods 换成抛未知错误 → 复制诊断 + 重试
all.push(
  ...(await withApp({
    data: join(ROOT, "unknown"),
    seed: (data) => {
      rmSync(data, { recursive: true, force: true });
      mkdirSync(data, { recursive: true });
      if (existsSync("_tools/modio-token.txt")) copyFileSync("_tools/modio-token.txt", join(data, "modio-token.txt"));
    },
    run: async (ev, shot) => {
      await go(ev, 1);
      await ev(`(() => {
        const orig = window.__TAURI_INTERNALS__.invoke;
        window.__TAURI_INTERNALS__.invoke = (cmd, args) => {
          if (cmd === 'search_mods' || cmd === 'list_mods') {
            return Promise.resolve({ ok: false, error: { code: 'weird_state', message: '接口回了个我们不认识的错' } });
          }
          return orig(cmd, args);
        };
        return true;
      })()`);
      await clickText(ev, "搜索");
      await sleep(2500);
      const r = await shot("10-mine-error-unknown");
      return [{ shot: "10-mine-error-unknown", ...r }];
    },
  })),
);

spawnSync("taskkill", ["/F", "/IM", "FSD-Win64-Shipping.exe"], { stdio: "ignore" });
console.log(JSON.stringify(all, null, 1));
