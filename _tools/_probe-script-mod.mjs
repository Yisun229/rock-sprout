// 验 M7-4：脚本类 mod 走 ue4ss\Mods\<名字>\，并且要真在游戏里加载起来。
//   node _tools/_probe-script-mod.mjs install     # 铺 UE4SS + 装脚本 mod（会写游戏目录）
//   node _tools/_probe-script-mod.mjs verify      # 启动游戏，等日志里出现 Load Mod '...'
//   node _tools/_probe-script-mod.mjs uninstall   # 卸脚本 mod + 撤 UE4SS，最后自检快照
//
// 三条硬规矩（§0 第 33/34 条）：
//   1. 只认日志里的 Load Mod，目录建对了不算；
//   2. 每次动游戏目录前后各拍一次快照，退出前自己比一遍，不一致就非零退出；
//   3. 不碰 FSD\Mods、mods_P.pak、x3daudio1_7.dll。
import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

const PHASE = process.argv[2] ?? "install";
const TOOLS = import.meta.dirname;
const ROOT = join(TOOLS, "..");
const EXE = join(TOOLS, "target/release/rock-sprout.exe");
const DATA = join(TOOLS, "_m74-data");
const OUT = join(ROOT, "_tmp/ui-baseline");
const SNAP = join(ROOT, "_tmp/m7-2"); // 和 M7-2 用同一个目录、同一份口径
const SAMPLE = join(TOOLS, "u4ss-sample/4454464.zip");
const ZIP_SRC = join(TOOLS, "ue4ss-test/UE4SS_v3.0.1.zip");
const GAME = "D:\\Steam\\steamapps\\common\\Deep Rock Galactic\\FSD\\Binaries\\Win64";
const GAME_EXE = join(GAME, "FSD-Win64-Shipping.exe");
const LOG = join(GAME, "UE4SS.log");
const CDP = "http://127.0.0.1:9222/json";

mkdirSync(OUT, { recursive: true });
mkdirSync(SNAP, { recursive: true });
mkdirSync(join(DATA, "downloads"), { recursive: true });

/** 拍一份游戏目录快照（复用 M7-2 那个脚本，口径一致才好比）。 */
function snapshot(tag) {
  const res = spawnSync(process.execPath, [join(TOOLS, "_m7-snapshot.mjs"), tag], {
    cwd: ROOT,
    encoding: "utf8",
  });
  if (res.status !== 0) throw new Error(`快照失败：${res.stderr || res.stdout}`);
  return JSON.parse(readFileSync(join(SNAP, `snap-${tag}.json`), "utf8"));
}

/** 比两份快照；返回差异数组（空 = 一模一样）。 */
function diffSnap(a, b) {
  const out = [];
  const fa = new Map(a.files.map((f) => [f.rel, f.sha256]));
  const fb = new Map(b.files.map((f) => [f.rel, f.sha256]));
  for (const [rel, sha] of fa) {
    if (!fb.has(rel)) out.push(`少了 ${rel}`);
    else if (fb.get(rel) !== sha) out.push(`变了 ${rel}`);
  }
  for (const rel of fb.keys()) if (!fa.has(rel)) out.push(`多了 ${rel}`);
  if (a.watched?.pak?.sha256 !== b.watched?.pak?.sha256) out.push("mods_P.pak 变了");
  return out;
}

async function withApp(fn) {
  if (!existsSync(join(DATA, "downloads/UE4SS_v3.0.1.zip")) && existsSync(ZIP_SRC)) {
    copyFileSync(ZIP_SRC, join(DATA, "downloads/UE4SS_v3.0.1.zip"));
  }
  const child = spawn(EXE, [], {
    cwd: ROOT,
    detached: true,
    stdio: "ignore",
    env: {
      ...process.env,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9222",
      ROCK_SPROUT_DATA: DATA,
      HTTPS_PROXY: "",
      HTTP_PROXY: "",
      ALL_PROXY: "",
    },
  });
  child.unref();

  let wsUrl = null;
  for (let i = 0; i < 150 && !wsUrl; i++) {
    try {
      const targets = await (await fetch(CDP)).json();
      const page = targets.find((t) => t.url.startsWith("http://tauri.localhost"));
      if (page) wsUrl = page.webSocketDebuggerUrl;
    } catch {
      // 端口还没起来
    }
    if (!wsUrl) await sleep(200);
  }
  if (!wsUrl) throw new Error("应用没起来");

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
  await new Promise((r) => ws.addEventListener("open", r));
  const send = (method, params) =>
    new Promise((resolve) => {
      const myId = ++id;
      pending.set(myId, resolve);
      ws.send(JSON.stringify({ id: myId, method, params }));
    });
  const evalJs = async (expression) => {
    const res = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (res.result?.exceptionDetails) throw new Error(JSON.stringify(res.result.exceptionDetails).slice(0, 300));
    return res.result?.result?.value;
  };
  const shot = async (name) => {
    const res = await send("Page.captureScreenshot", { format: "png", captureBeyondViewport: false });
    writeFileSync(join(OUT, name), Buffer.from(res.result.data, "base64"));
  };
  const invoke = (cmd, args = {}) =>
    evalJs(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);

  for (let i = 0; i < 100; i++) {
    if (await evalJs("!!(window.__TAURI_INTERNALS__ && document.querySelector('.rail button'))")) break;
    await sleep(150);
  }

  try {
    await fn({ evalJs, shot, invoke });
  } finally {
    spawnSync("taskkill", ["/F", "/IM", "rock-sprout.exe"], { stdio: "ignore" });
    try {
      ws.close();
    } catch {
      // 已经断了
    }
  }
}

if (PHASE === "install") {
  const before = snapshot("m74-A");
  console.log(`快照 A：${before.files.length} 个文件`);

  await withApp(async ({ evalJs, shot, invoke }) => {
    await evalJs(`(() => {
      const btn = [...document.querySelectorAll('.rail button')].find((b) => b.textContent.includes('设置'));
      btn.click();
      return true;
    })()`);
    await sleep(900);

    // 包没备好的话先解出来
    let st = await invoke("ue4ss_status");
    if (!st.data?.package_ready) {
      await evalJs(`document.querySelector('[data-block="ue4ss"] .ue4ss-actions .btn').click()`);
      for (let i = 0; i < 60; i++) {
        await sleep(500);
        st = await invoke("ue4ss_status");
        if (st.data?.package_ready) break;
      }
    }

    // 先把官方 UE4SS 铺上（MintCat 那套会撞名 → 确认替换，备份落在数据目录）
    let gs = await invoke("ue4ss_game_status");
    if (gs.data?.kind !== "ours") {
      const r = await invoke("ue4ss_install", { confirmReplace: true });
      if (!r.ok) throw new Error(`铺 UE4SS 失败：${JSON.stringify(r.error)}`);
      console.log(`已铺 UE4SS，备份在：${r.data.backup_dir}`);
    }

    // 装脚本 mod
    const imp = await invoke("import_local", { path: SAMPLE });
    console.log("装脚本 mod：", JSON.stringify(imp).slice(0, 400));
    if (!imp.ok) throw new Error(`装脚本 mod 失败：${JSON.stringify(imp.error)}`);

    const list = await invoke("list_installed");
    const script = (list.data ?? []).filter((m) => m.kind === "script");
    console.log("背包里的脚本类：", JSON.stringify(script));

    // 看界面：切到背包，截一张
    await evalJs(`(() => {
      const btn = [...document.querySelectorAll('.rail button')].find((b) => b.textContent.includes('背包'));
      btn.click();
      return true;
    })()`);
    await sleep(1200);
    const row = await evalJs(`(() => {
      const row = document.querySelector('.row[data-kind="script"]');
      if (!row) return null;
      return { title: row.querySelector('.row-title')?.textContent, sub: row.querySelector('.row-sub')?.textContent };
    })()`);
    console.log("背包那一行：", JSON.stringify(row));
    await shot("script-mod-installed.png");
  });

  const after = snapshot("m74-B");
  console.log(`快照 B：${after.files.length} 个文件`);
  const d = diffSnap(before, after);
  console.log(`A→B 差异 ${d.length} 处：`, d.slice(0, 12));
  writeFileSync(join(SNAP, "report-install.json"), JSON.stringify({ before: before.files.length, after: after.files.length, diff: d }, null, 2));
} else if (PHASE === "verify") {
  // 真启动一次游戏，等日志里出现 Load Mod '<名字>'
  const startedAt = Date.now();
  const child = spawn(GAME_EXE, [], { cwd: GAME, detached: true, stdio: "ignore" });
  child.unref();
  console.log("游戏已启动，等日志…");

  let hits = [];
  for (let i = 0; i < 90; i++) {
    await sleep(1000);
    if (!existsSync(LOG)) continue;
    const text = readFileSync(LOG, "utf8");
    hits = text.split(/\r?\n/).filter((l) => /Load Mod '/.test(l));
    if (hits.length > 0) break;
  }
  console.log(`等了 ${Math.round((Date.now() - startedAt) / 1000)} 秒，Load Mod 行：`);
  for (const line of hits) console.log("   ", line.trim());

  const head = existsSync(LOG) ? readFileSync(LOG, "utf8").split(/\r?\n/).slice(0, 6) : [];
  console.log("日志开头：", head);
  spawnSync("taskkill", ["/F", "/IM", "FSD-Win64-Shipping.exe"], { stdio: "ignore" });
  await sleep(1500);
  writeFileSync(join(SNAP, "report-verify.json"), JSON.stringify({ at: new Date().toISOString(), hits }, null, 2));
  if (hits.length === 0) {
    console.error("没有看到 Load Mod —— 脚本类没真加载，这一项不算过");
    process.exit(2);
  }
} else if (PHASE === "uninstall") {
  const before = JSON.parse(readFileSync(join(SNAP, "snap-m74-A.json"), "utf8"));

  await withApp(async ({ evalJs, shot, invoke }) => {
    const list = await invoke("list_installed");
    for (const m of list.data ?? []) {
      if (m.kind !== "script") continue;
      const r = await invoke("uninstall_mod", { id: m.id });
      console.log(`卸掉脚本 mod ${m.name}：`, JSON.stringify(r).slice(0, 200));
    }
    const out = await invoke("ue4ss_uninstall");
    console.log("撤 UE4SS：", JSON.stringify(out).slice(0, 200));

    await evalJs(`(() => {
      const btn = [...document.querySelectorAll('.rail button')].find((b) => b.textContent.includes('背包'));
      btn.click();
      return true;
    })()`);
    await sleep(1000);
    await shot("script-mod-uninstalled.png");
  });

  // 把这次验活长出来的日志清掉（它算运行期文件，卸载时我们已经清过一次；
  // 这里再兜一层，因为 verify 那轮是独立进程启动的）
  if (existsSync(LOG)) {
    const text = readFileSync(LOG);
    writeFileSync(join(SNAP, "UE4SS.log.saved"), text);
    spawnSync("powershell", ["-NoProfile", "-Command", `Remove-Item -LiteralPath '${LOG}' -Force`], { stdio: "ignore" });
  }

  const after = snapshot("m74-C");
  const d = diffSnap(before, after);
  console.log(`快照 C：${after.files.length} 个文件`);
  console.log(`A→C 差异 ${d.length} 处：`, d.slice(0, 12));
  writeFileSync(join(SNAP, "report-uninstall.json"), JSON.stringify({ before: before.files.length, after: after.files.length, diff: d }, null, 2));
  if (d.length > 0) {
    console.error("游戏目录没还原干净 —— 按 §0 第 34 条，这里必须非零退出");
    process.exit(3);
  }
  console.log("游戏目录和动手前逐字节一致。");
} else {
  console.error(`不认识的阶段：${PHASE}`);
  process.exit(1);
}
