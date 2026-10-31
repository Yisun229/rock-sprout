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
// Lua 夹具：证「成功」那条链路（§0 第 36 条要求成功和失败各一条）
const LUA_SAMPLE = join(TOOLS, "u4ss-sample/ProSproutLua.zip");
const ZIP_SRC = join(TOOLS, "ue4ss-test/UE4SS_v3.0.1.zip");
const GAME = "D:\\Steam\\steamapps\\common\\Deep Rock Galactic\\FSD\\Binaries\\Win64";
const GAME_EXE = join(GAME, "FSD-Win64-Shipping.exe");
const LOG = join(GAME, "UE4SS.log");
const CDP = "http://127.0.0.1:9222/json";

mkdirSync(OUT, { recursive: true });
mkdirSync(SNAP, { recursive: true });
mkdirSync(join(DATA, "downloads"), { recursive: true });

/** 游戏进程起没起来（给「启动超时」用）。 */
function gameRunning() {
  const res = spawnSync("tasklist", ["/FI", "IMAGENAME eq FSD-Win64-Shipping.exe", "/NH"], {
    encoding: "utf8",
  });
  return /FSD-Win64-Shipping/i.test(res.stdout ?? "");
}

/** 只留最近一份备份（§0 第 31 条）。 */
function keepLatestBackup(dir) {
  if (!existsSync(dir)) return [];
  const entries = readdirSync(dir, { withFileTypes: true })
    .filter((e) => e.isDirectory())
    .map((e) => ({ name: e.name, path: join(dir, e.name) }))
    .sort((a, b) => a.name.localeCompare(b.name));
  const removed = [];
  for (const old of entries.slice(0, Math.max(0, entries.length - 1))) {
    spawnSync(
      "powershell",
      ["-NoProfile", "-Command", `Remove-Item -LiteralPath '${old.path}' -Recurse -Force`],
      { stdio: "ignore" },
    );
    removed.push(old.name);
  }
  return removed;
}

/** 拍一份游戏目录快照（复用 M7-2 那个脚本，口径一致才好比）。 */
function snapshot(tag) {
  const res = spawnSync(process.execPath, [join(TOOLS, "_m7-snapshot.mjs"), tag], {
    cwd: ROOT,
    encoding: "utf8",
  });
  if (res.status !== 0) throw new Error(`快照失败：${res.stderr || res.stdout}`);
  return JSON.parse(readFileSync(join(SNAP, `snap-${tag}.json`), "utf8"));
}

/** 快照里的文件清单（拍平成一维，方便两句代码比）。 */
function filesOf(snap) {
  return snap?.win64?.files ?? [];
}

/** 快照里 mods_P.pak 在不在。 */
function pakExists(snap) {
  return (snap?.watched ?? []).some((w) => /mods_P\.pak$/i.test(w.path) && w.exists);
}

/** 比两份快照；返回差异数组（空 = 一模一样）。 */
function diffSnap(a, b) {
  const out = [];
  const fa = new Map(filesOf(a).map((f) => [f.rel, f.sha256]));
  const fb = new Map(filesOf(b).map((f) => [f.rel, f.sha256]));
  for (const [rel, sha] of fa) {
    if (!fb.has(rel)) out.push(`少了 ${rel}`);
    else if (fb.get(rel) !== sha) out.push(`变了 ${rel}`);
  }
  for (const rel of fb.keys()) if (!fa.has(rel)) out.push(`多了 ${rel}`);
  if (pakExists(a) !== pakExists(b)) out.push(`mods_P.pak 从 ${pakExists(a) ? "有" : "无"} 变成 ${pakExists(b) ? "有" : "无"}`);
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

/**
 * 失败时先自动退回快照 A，再停下等人。
 *
 * 退法是「用程序自己的卸载」：脚本类 mod 撤掉、UE4SS 用备份还原；
 * 然后重新拍快照跟 A 逐字节比，不一致就以 4 退出（说明连回滚都没干净）。
 */
async function rollbackToA(reason) {
  console.error(`\n[失败] ${reason}`);
  console.error("按规矩先自动回滚到快照 A，再停。");

  try {
    await withApp(async ({ invoke }) => {
      const list = await invoke("list_installed");
      for (const m of list.data ?? []) {
        if (m.kind === "script" || m.source === "local") {
          const r = await invoke("uninstall_mod", { id: m.id });
          console.error(`  撤 ${m.name}：`, JSON.stringify(r).slice(0, 120));
        }
      }
      const out = await invoke("ue4ss_uninstall");
      console.error("  撤 UE4SS：", JSON.stringify(out).slice(0, 160));
    });
  } catch (e) {
    console.error("  回滚时应用起不来：", String(e).slice(0, 200));
  }

  if (existsSync(LOG)) {
    writeFileSync(join(SNAP, "UE4SS.log.rollback"), readFileSync(LOG));
    spawnSync("powershell", ["-NoProfile", "-Command", `Remove-Item -LiteralPath '${LOG}' -Force`], {
      stdio: "ignore",
    });
  }

  const now = snapshot("m74-rollback");
  const a = JSON.parse(readFileSync(join(SNAP, "snap-m74-A.json"), "utf8"));
  const d = diffSnap(a, now);
  if (d.length > 0) {
    console.error(`回滚后仍与 A 不一致（${d.length} 处）：`, d.slice(0, 20));
    process.exit(4);
  }
  console.error("回滚完成：游戏目录与快照 A 逐字节一致。停在这里等指示。");
  process.exit(5);
}

if (PHASE === "install") {

  // 先把上一轮没跑完的东西撤干净，再拍 A——不然 A 拍到的是脏状态
  await withApp(async ({ invoke }) => {
    const list = await invoke("list_installed");
    for (const m of (list.data ?? []).filter((m) => m.kind === "script")) {
      const r = await invoke("uninstall_mod", { id: m.id });
      console.log(`清掉上一轮残留的脚本 mod ${m.name}：`, JSON.stringify(r).slice(0, 120));
    }
    const gs = await invoke("ue4ss_game_status");
    if (gs.data?.kind === "ours") {
      const r = await invoke("ue4ss_uninstall");
      console.log("先把上一轮铺的 UE4SS 撤掉：", JSON.stringify(r).slice(0, 160));
    }
  });

  const before = snapshot("m74-A");
  console.log(`快照 A：${filesOf(before).length} 个文件，mods_P.pak ${pakExists(before) ? "存在" : "不存在"}`);

  try {
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
      if (!st.data?.package_ready) throw new Error("UE4SS 包没备好（解包没成）");

      // 先把官方 UE4SS 铺上（MintCat 那套会撞名 → 确认替换，备份落在数据目录）
      const gs = await invoke("ue4ss_game_status");
      if (gs.data?.kind !== "ours") {
        const r = await invoke("ue4ss_install", { confirmReplace: true });
        if (!r.ok) throw new Error(`铺 UE4SS 失败：${JSON.stringify(r.error)}`);
        console.log(`已铺 UE4SS，备份在：${r.data.backup_dir}`);
        // 铺完立刻看一眼 pak：这一步不该碰合并包
        const mid = snapshot("m74-mid-ue4ss");
        console.log(
          `铺完 UE4SS 时 mods_P.pak ${pakExists(mid) ? "存在（要查清是谁写的）" : "仍不存在"}`,
        );
      }

      // 装脚本 mod
      const imp = await invoke("import_local", { path: SAMPLE });
      console.log("装脚本 mod：", JSON.stringify(imp).slice(0, 400));
      if (!imp.ok) throw new Error(`装脚本 mod 失败：${JSON.stringify(imp.error)}`);
      // 再装一个 Lua 的：这条要真能跑起来，用来证「装法本身没问题」
      const impLua = await invoke("import_local", { path: LUA_SAMPLE });
      console.log("装 Lua 夹具：", JSON.stringify(impLua).slice(0, 300));
      if (!impLua.ok) throw new Error(`装 Lua 夹具失败：${JSON.stringify(impLua.error)}`);


      const list = await invoke("list_installed");
      const script = (list.data ?? []).filter((m) => m.kind === "script");
      console.log("背包里的脚本类：", JSON.stringify(script));
      if (script.length === 0) throw new Error("装完记账里没有脚本类，kind 没写上");

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
    console.log(`快照 B：${filesOf(after).length} 个文件`);
    const d = diffSnap(before, after);
    console.log(`A→B 差异 ${d.length} 处：`, d.slice(0, 12));
    writeFileSync(
      join(SNAP, "report-install.json"),
      JSON.stringify(
        { before: filesOf(before).length, after: filesOf(after).length, diff: d, pak: pakExists(after) },
        null,
        2,
      ),
    );
    if (pakExists(after) && !pakExists(before)) {
      console.log("注意：这次装完 mods_P.pak 出现了。它是导入脚本类之外的路径产生的，得单独说明。");
    }
  } catch (e) {
    await rollbackToA(`install 段失败：${String(e).slice(0, 300)}`);
  }
} else if (PHASE === "verify") {
  // 真启动一次游戏，按 §0 第 35 条的真实日志口径判两个夹具：
  //   ProSproutLua → 成功行（Mod 'X' has enabled.txt, starting mod. / Starting Lua mod 'X'）
  //   4454464     → 失败行（Failed to load dll … for mod X, error code: 0x7e）
  const NAMES = { lua: "ProSproutLua", cpp: "4454464" };

  /** 看日志正文，判一个 mod 这次是起来了还是载不进去。 */
  function verdictOf(lines, name) {
    const enabled = `Mod '${name}' has enabled.txt, starting mod.`;
    const luaStart = `Starting Lua mod '${name}'`;
    const failPrefix = `for mod ${name}, error code: 0x`;
    let loaded = false;
    let code = null;
    let line = "";
    for (const raw of lines) {
      const l = raw.trim();
      if (l.includes(failPrefix)) {
        const tail = l.slice(l.indexOf(failPrefix) + failPrefix.length);
        const hex = tail.match(/^[0-9a-fA-F]+/);
        if (hex) {
          code = parseInt(hex[0], 16);
          line = l;
        }
        continue;
      }
      if (l.includes(enabled) || l.includes(luaStart)) {
        loaded = true;
        line = l;
      }
    }
    if (code !== null) return { state: "failed", code, line };
    if (loaded) return { state: "ok", code: null, line };
    return { state: "unknown", code: null, line: "" };
  }

  const startedAt = Date.now();
  const child = spawn(GAME_EXE, [], { cwd: GAME, detached: true, stdio: "ignore" });
  child.unref();
  console.log("游戏已启动，先等它起来（最多 30 秒）…");

  let up = false;
  for (let i = 0; i < 30; i++) {
    await sleep(1000);
    if (gameRunning()) {
      up = true;
      break;
    }
  }
  if (!up) {
    console.error("30 秒内没有看到游戏进程 —— 启动失败");
    spawnSync("taskkill", ["/F", "/IM", "FSD-Win64-Shipping.exe"], { stdio: "ignore" });
    await rollbackToA("verify 段：游戏没启动起来");
  }
  const secs = Math.round((Date.now() - startedAt) / 1000);
  console.log(`游戏进程起来了（第 ${secs} 秒），等日志里两边各自的结论…`);

  let got = { lua: { state: "unknown" }, cpp: { state: "unknown" } };
  for (let i = 0; i < 60; i++) {
    await sleep(1000);
    if (!existsSync(LOG)) continue;
    const lines = readFileSync(LOG, "utf8").split(/\r?\n/);
    got = { lua: verdictOf(lines, NAMES.lua), cpp: verdictOf(lines, NAMES.cpp) };
    if (got.lua.state !== "unknown" && got.cpp.state !== "unknown") break;
  }
  const waited = Math.round((Date.now() - startedAt) / 1000);
  console.log(`等了 ${waited} 秒：`);
  console.log("  Lua 夹具：", got.lua.state, got.lua.line || "");
  console.log(
    "  4454464：",
    got.cpp.state,
    got.cpp.code === null ? "" : `0x${got.cpp.code.toString(16)}`,
    got.cpp.line || "",
  );

  spawnSync("taskkill", ["/F", "/IM", "FSD-Win64-Shipping.exe"], { stdio: "ignore" });
  await sleep(1500);
  writeFileSync(
    join(SNAP, "report-verify.json"),
    JSON.stringify({ at: new Date().toISOString(), waited, lua: got.lua, cpp: got.cpp }, null, 2),
  );

  const problems = [];
  if (got.lua.state !== "ok") problems.push(`Lua 夹具没有起来（${got.lua.state}）`);
  if (got.cpp.state !== "failed") problems.push(`4454464 竟然没被判成失败（${got.cpp.state}）`);
  if (got.cpp.code !== 0x7e) problems.push(`4454464 的错误码不是 0x7e`);
  if (problems.length > 0) {
    writeFileSync(join(SNAP, "report-verify-problems.json"), JSON.stringify(problems, null, 2));
    await rollbackToA(`verify 段：${problems.join("；")}`);
  }

  // 界面那一侧：重开一个应用，看背包里两个脚本 mod 的状态
  await withApp(async ({ evalJs, shot, invoke }) => {
    const toBag = [
      "(() => {",
      "  const btn = [...document.querySelectorAll('.rail button')].find((b) => b.textContent.includes('背包'));",
      "  btn.click();",
      "  return true;",
      "})()",
    ].join("\n");
    await evalJs(toBag);
    await sleep(1400);
    const readRows = [
      "(() => [...document.querySelectorAll('.row[data-kind=\"script\"]')].map((r) => ({",
      "  title: r.querySelector('.row-title')?.textContent,",
      "  sub: r.querySelector('.row-sub')?.textContent,",
      "})))()",
    ].join("\n");
    const rows = await evalJs(readRows);
    console.log("背包里脚本类那一行：", JSON.stringify(rows));
    const list = await invoke("list_installed");
    const script = (list.data ?? []).filter((m) => m.kind === "script");
    console.log(
      "后端给的脚本状态：",
      JSON.stringify(script.map((m) => [m.name, m.script_state, m.script_detail])),
    );
    const bad = script.filter((m) => m.script_state === "unknown");
    if (bad.length > 0) throw new Error(`有脚本 mod 还是 unknown：${JSON.stringify(bad.map((m) => m.name))}`);
    const failed = script.find((m) => m.name === NAMES.cpp);
    if (!failed || failed.script_state !== "failed" || !failed.script_detail) {
      throw new Error("4454464 在界面上没有带原因地标成没生效");
    }
    const ok = script.find((m) => m.name === NAMES.lua);
    if (!ok || ok.script_state !== "ok") throw new Error("Lua 夹具在界面上没有标成已生效");
    await shot("script-mod-health.png");
  });
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

  // 备份只留最近一份（§0 第 31 条）
  const droppedUe4ss = keepLatestBackup(join(DATA, "backup"));
  const droppedMods = keepLatestBackup(join(DATA, "backup/ue4ss-mods"));
  if (droppedUe4ss.length + droppedMods.length > 0) {
    console.log("清掉旧备份：", [...droppedUe4ss, ...droppedMods]);
  }

  const after = snapshot("m74-C");
  const d = diffSnap(before, after);
  console.log(`快照 C：${filesOf(after).length} 个文件`);
  console.log(`A→C 差异 ${d.length} 处：`, d.slice(0, 12));
  writeFileSync(join(SNAP, "report-uninstall.json"), JSON.stringify({ before: filesOf(before).length, after: filesOf(after).length, diff: d }, null, 2));
  if (d.length > 0) {
    console.error("游戏目录没还原干净 —— 按 §0 第 34 条，这里必须非零退出");
    process.exit(3);
  }
  console.log("游戏目录和动手前逐字节一致。");
} else {
  console.error(`不认识的阶段：${PHASE}`);
  process.exit(1);
}
