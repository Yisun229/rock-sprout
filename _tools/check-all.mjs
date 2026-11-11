
// 一条命令跑完回归。
//
//   node _tools/check-all.mjs            只跑不需要网的一组（默认）
//   node _tools/check-all.mjs --online   再加需要 mod.io 可达的一组（要 _tools/modio-token.txt）
//
// 每个场景会自己起一个新的应用窗口（带 9222 调试端口），跑完就关掉，
// 所以一条失败了不会把后面的场景带歪。最后打印 PASS/FAIL 汇总，任一条失败则退出码非零。
import { spawn, spawnSync } from "node:child_process";import { copyFileSync, cpSync, existsSync, mkdirSync, readdirSync, readFileSync, renameSync, rmSync, statSync, utimesSync, writeFileSync } from "node:fs";import { createHash } from "node:crypto";import { basename, dirname, join } from "node:path";import { setTimeout as sleep } from "node:timers/promises";import { writeSearchCache } from "./fixture-cache.mjs";
const TOOLS = import.meta.dirname;
const ROOT = dirname(TOOLS);
const EXE = join(TOOLS, "target/release/rock-sprout.exe");
const PEEK = join(TOOLS, "target/release/peek.exe");
const FIXTURE = join(TOOLS, "fixtures/small-mod.zip");
const TOKEN = join(TOOLS, "modio-token.txt");
const TESTDATA = join(TOOLS, "_testdata");
const GAME = "D:/Steam/steamapps/common/Deep Rock Galactic/FSD";
const PAKS = join(GAME, "Content/Paks");
const DLL = join(GAME, "Binaries/Win64/x3daudio1_7.dll");
const CDP = "http://127.0.0.1:9222/json";
const ONLINE = process.argv.includes("--online");
// 截图基线：node _tools/check-all.mjs --ui-snap before|after
// 存到 _tmp/ui-baseline/<标签>/ 下，三页各一张；只截图，不跑回归。
const snapIdx = process.argv.indexOf("--ui-snap");
const SNAP = snapIdx >= 0 ? process.argv[snapIdx + 1] || "before" : null;
const results = [];
let currentPhase = "";
// ---- 小工具 ----
function assert(cond, msg) {
if (!cond) throw new Error(msg);}
function appAlive() {
  const out = spawnSync("tasklist", ["/FI", "IMAGENAME eq rock-sprout.exe", "/NH"], { encoding: "utf8" });
  const text = (out.stdout || "").toLowerCase();
  if (!text.includes("rock-sprout.exe")) return false;
  // 没有匹配时 tasklist 会输出一句「信息: 没有运行的任务…」，那种不算
  return !text.includes("no tasks") && !text.includes("没有运行");
}
/** 杀进程并等到它真的消失。
 *
 * 单实例守卫上线后，上一个场景的窗口还没退干净就起下一个，会被当成「已经开着了」
 * 挡回去——那就从「偶发红」变成「稳定红」。所以这里必须等。 */
async function killAppAndWait() {
  killApp();
  for (let i = 0; i < 40; i++) {
    if (!appAlive()) return true;
    await sleep(100);
  }
  return false;
}
function killApp() {  spawnSync("taskkill", ["/IM", "rock-sprout.exe", "/F"], { stdio: "ignore" });  
// 便携包里的 exe 叫 RockSprout.exe（没有连字符），名字不一样，得单独杀
spawnSync("taskkill", ["/IM", "RockSprout.exe", "/F"], { stdio: "ignore" });}
function clearGameDir() {  
// 测试可能留下合并包/加载器，收尾时用 peek 复原（它调的是程序里同一段 clear 逻辑）
  if (!existsSync(PEEK)) return "没有 peek.exe，跳过复原";
  const out = spawnSync(PEEK, ["clear"], { encoding: "utf8" });
  let msg = (out.stdout || "").trim() + (out.stderr || "").trim();  
// 兜底：peek 用的是它自己的数据目录，里面的备份可能早就没了（备份跟着数据目录走）。  
// 万一加载器还是我们那个大文件，就从仓库里那份「游戏原版」备份拷回去，别把游戏留成坏的。
  const dll = DLL;
  const stock = join(TOOLS, "game-backup/2026-10-28/FSD/Binaries/Win64/x3daudio1_7.dll");
  if (existsSync(dll) && existsSync(stock)) {    const now = statSync(dll).size;
    const orig = statSync(stock).size;
    if (now !== orig) {      copyFileSync(stock, dll);      msg += `（加载器不是原版，已从备份换回：${now} → ${orig} 字节）`;    }  }  return msg;}
function freshDataDir(name) {  const dir = join(TESTDATA, name);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(join(dir, "mods/installed"), { recursive: true });  return dir;}/** 起一个应用窗口并等它的调试端口就绪。 * * `opts.exe` 可以换一个可执行文件（测便携模式时要拷到别的目录去跑）； * `opts.cwd` 换工作目录。 */
async function startApp(env, opts = {}) {
  // 单实例守卫：上一个窗口没退干净就起新的会被挡回去，所以先等到它真的没了
  await killAppAndWait();
  const t0 = Date.now();
  const exe = opts.exe || EXE;
  const child = spawn(exe, [], {    cwd: opts.cwd || ROOT,    detached: true,    stdio: "ignore",    env: {      ...process.env,      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9222",      ...env,    },  });  child.unref();
  for (let i = 0; i < 150; i++) {    try {      const targets = await (await fetch(CDP)).json();
      const page = targets.find((t) => t.url.startsWith("http://tauri.localhost"));
      if (page) return { readyMs: Date.now() - t0, wsUrl: page.webSocketDebuggerUrl };    } catch {      
// 端口还没起来，继续等
  }
    await sleep(200);  }
  killApp();
  throw new Error("应用 30 秒内没起来（调试端口没开）");}/** 连上面那个窗口，给两个能力：在页面里跑 JS、调后端命令。 */
async function connect(wsUrl) {  const ws = new WebSocket(wsUrl);
  let id = 0;
  const pending = new Map();  ws.addEventListener("message", (ev) => {    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {      pending.get(msg.id)(msg);      pending.delete(msg.id);    }  });
  await new Promise((resolve) => ws.addEventListener("open", resolve));
  const send = (method, params) =>    new Promise((resolve) => {      const myId = ++id;      pending.set(myId, resolve);      ws.send(JSON.stringify({ id: myId, method, params }));    });  async function evalJs(expression) {    const res = await send("Runtime.evaluate", {      expression,      awaitPromise: true,      returnByValue: true,    });
    const det = res.result?.exceptionDetails;
    if (det) throw new Error("页面里报错：" + (det.exception?.description ?? JSON.stringify(det)));    return res.result?.result?.value;  }  /** 调后端命令，返回后端原始结果 { ok, data, error }。 */
  const invoke = (command, args = null) =>    evalJs(      `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)})`,    );
  const text = () => evalJs("document.body.innerText");
  const clickText = async (selector, label) => {    const hit = await evalJs(      `(() => {        const all = [...document.querySelectorAll(${JSON.stringify(selector)})].filter((e) => e.innerText.includes(${JSON.stringify(label)}));        // 三块页面（矿脉/背包/设置）都挂在 DOM 里，没显示的那块也有同样的字，        // 所以要优先挑真占着位置的元素，别点到一个看不见的按钮上。
const el = all.find((e) => e.getClientRects().length > 0) ?? all[0];
        if (!el) return false;        el.click();        return true;      })()`);
    assert(hit, `页面上找不到「${label}」`);  };  /** 截一张图存成 png（CDP 的 Page.captureScreenshot 回来是 base64）。 */
  async function screenshot(filePath) {    const res = await send("Page.captureScreenshot", { format: "png", captureBeyondViewport: false });
    const b64 = res.result?.data;
    assert(b64, `截图失败：${JSON.stringify(res.error ?? res).slice(0, 300)}`);
    const buf = Buffer.from(b64, "base64");
    mkdirSync(dirname(filePath), { recursive: true });    writeFileSync(filePath, buf);    return { path: filePath, bytes: buf.length };  }  return { evalJs, invoke, text, clickText, screenshot, send, close: () => ws.close() };}/** 等页面真正就绪：没等这个就调后端，会撞上「__TAURI_INTERNALS__ 是 undefined」。 */
async function waitReady(s, timeoutMs = 15000) {  const t0 = Date.now();
  while (Date.now() - t0 < timeoutMs) {    try {      const ok = await s.evalJs(        "!!(window.__TAURI_INTERNALS__ && document.readyState === 'complete' && document.querySelector('button'))",      );
      if (ok) return Date.now() - t0;    } catch {      
// 页面还没进入可执行状态，继续等
  }
    await sleep(150);  }
  throw new Error("页面 15 秒内没就绪");}
/** 现在默认落在「今天」页；要摸矿脉的东西，先点一下侧栏（切页是 hidden，不卸载）。 */
async function gotoMine(s) {
  await s.clickText(".rail button", "矿脉");
  await sleep(600);
}
async function scenario(name, fn) {  process.stdout.write(`  … ${name}\n`);
  const t0 = Date.now();
  try {    await fn();
    const ms = Date.now() - t0;    results.push({ phase: currentPhase, name, ok: true, ms });
    console.log(`  PASS ${name}  ${ms}ms`);  } catch (e) {    const ms = Date.now() - t0;
    const msg = e instanceof Error ? e.message : String(e);    results.push({ phase: currentPhase, name, ok: false, ms, msg });
    console.log(`  FAIL ${name}  ${ms}ms\n       ${msg}`);  }}
function phase(title) {  currentPhase = title;
  console.log(`\n== ${title} ==`);}
// ---- 开跑 ----
if (!existsSync(EXE)
) {  console.log(`没有找到 ${EXE}\n先跑一遍：cd app && node node_modules/@tauri-apps/cli/tauri.js build --no-bundle`);
  process.exit(2);}
if (!existsSync(FIXTURE)) {  console.log(`没有测试用的 mod 包：${FIXTURE}`);
  process.exit(2);}
killApp();
rmSync(TESTDATA, { recursive: true, force: true });
mkdirSync(TESTDATA, { recursive: true });
// ---------- 截图基线模式：只截三页就退出，不跑回归 ----------
if (SNAP) {
const outDir = join(ROOT, "_tmp/ui-baseline", SNAP);
  const snapDir = freshDataDir(`snap-${SNAP}`);
  if (existsSync(TOKEN)) copyFileSync(TOKEN, join(snapDir, "modio-token.txt"));  writeSearchCache(snapDir, "性能", 30);  
// 断网起：矿脉会回放缓存，不会真去连 mod.io
const app = await startApp({    ROCK_SPROUT_DATA: snapDir,    HTTPS_PROXY: "http://127.0.0.1:9",    ALL_PROXY: "http://127.0.0.1:9",  });
  const s = await connect(app.wsUrl);
  await waitReady(s);  
// 背包里先放一个 mod，三页才都有东西可看
const imported = await s.invoke("import_local", { path: FIXTURE });
  assert(imported?.ok === true, `导入测试 mod 失败：${JSON.stringify(imported?.error)}`);
  const shots = [];
  const snap = async (file) => {    const it = await s.screenshot(join(outDir, file));    shots.push(it);    return it;  };  
// 现在默认落在「今天」页，先把它截下来；等骨架块换成正式内容（断网时会退化成一行说明）
for (let i = 0; i < 60; i++) {
    const loading = await s.evalJs("!!document.querySelector('.skeleton')");
    if (!loading) break;
    await sleep(200);
  }
  await sleep(300);
  await snap("00-今天.png");
  await s.clickText(".rail button", "矿脉");
  await sleep(600);
// 矿脉：用假缓存把列表喂满
await s.evalJs(`(() => {    const input = document.querySelector('[data-page="mine"] input.input');
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;    setter.call(input, '性能');    input.dispatchEvent(new Event('input', { bubbles: true }));    return true;  })()`);
  for (let i = 0; i < 80; i++) {    const rows = await s.evalJs(`document.querySelectorAll('[data-page="mine"] .vlist .row').length`);
    if (rows > 0) break;
    await sleep(150);  }
  await sleep(400);
  await snap("01-矿脉.png");
  await s.clickText(".rail button", "背包");
  await sleep(700);
  await snap("02-背包.png");
  await s.clickText(".rail button", "设置");
  await sleep(700);
  await snap("03-设置.png");
  s.close();
  killApp();
  await sleep(500);
  const cleaned = clearGameDir();
  console.log(`\n== 截图（${SNAP}）==`);
  for (const it of shots) console.log(`  ${it.path}  ${it.bytes} 字节`);
  console.log(`  收尾：${cleaned}`);
  process.exit(0);}
// ---------- 第一组：不需要网 ----------
phase("基础（不需要网）"
);
const emptyDir = freshDataDir("empty");{  const app = await startApp({ ROCK_SPROUT_DATA: emptyDir });
  const s = await connect(app.wsUrl);
  const readyMs = app.readyMs + (await waitReady(s));
  await scenario("冷启动后界面可用", async () => {    const rail = await s.text();
    assert(rail.includes("矿脉") && rail.includes("背包") && rail.includes("设置"), "左下导航没渲染出来");
    assert(readyMs < 5000, `冷启动到窗口可用用了 ${readyMs}ms，超过 5 秒`);
    console.log(`       冷启动到界面可用：${readyMs}ms`);  });
  await scenario("没填令牌时搜索给出可读提示", async () => {    await gotoMine(s);
    await s.clickText("button", "搜索");
    await sleep(1200);
    const page = await s.text();
    assert(page.includes("还没填访问令牌"), `页面上没出现「还没填访问令牌」，实际：\n${page.slice(0, 400)}`);  });
  await scenario("空数据目录下已装列表为空", async () => {    const res = await s.invoke("list_installed");
    assert(res?.ok === true, `list_installed 失败：${JSON.stringify(res?.error)}`);
    assert(Array.isArray(res.data) && res.data.length === 0, `应该 0 个，实际 ${JSON.stringify(res.data)}`);  });
  s.close();
  killApp();
  await sleep(600);}
// 导入 → 配置 → 卸载：同一个数据目录连着走
const mainDir = freshDataDir("main"
);
const importedNames = [];{  const app = await startApp({ ROCK_SPROUT_DATA: mainDir });
  const s = await connect(app.wsUrl);
  await waitReady(s);
  await scenario("导入本地 mod（.zip）", async () => {    const res = await s.invoke("import_local", { path: FIXTURE });
    assert(res?.ok === true, `导入失败：${JSON.stringify(res?.error)}`);
    assert(res.data.installed === true, "后端没报装好");
    assert(      typeof res.data.pak === "string" && res.data.pak.endsWith("mods_P.pak"),      `合并包路径不对：${res.data.pak}`,    );    importedNames.push(res.data.name);  });
  await scenario("背包里能看到刚导入的 mod", async () => {    await s.clickText("button", "背包");
    await sleep(800);
    const page = await s.text();
    assert(page.includes(importedNames[0]), `背包里没看到「${importedNames[0]}」`);  });
  await scenario("记账里来源标成 local", async () => {    const res = await s.invoke("list_installed");
    assert(res?.ok === true && res.data.length === 1, `应该有 1 条，实际 ${JSON.stringify(res.data)}`);
    assert(res.data[0].source === "local", `source 应该是 local，实际 ${res.data[0].source}`);  });
  await scenario("预检能问出装同一个包会撞，并说清谁被盖", async () => {    const res = await s.invoke("precheck_install", { modIds: [FIXTURE] });
    assert(res?.ok === true, "预检失败：" + JSON.stringify(res?.error));
    assert(res.data.conflicts.length > 0, "同一个包应该撞");
    assert(      res.data.summary.includes("只有排在后面的会生效"),      "说明要讲人话，实际：" + res.data.summary,    );
    assert(      res.data.conflicts.every((c) => !c.path.endsWith("assetregistry.bin")),      "AssetRegistry.bin 不该算撞",    );  });
  await scenario("第二次预检命中缓存，1 秒内返回", async () => {    const res = await s.invoke("precheck_install", { modIds: [FIXTURE] });
    assert(res?.ok === true, "预检失败：" + JSON.stringify(res?.error));
    assert(res.data.elapsed_ms < 1000, "命中缓存应该很快，实际 " + res.data.elapsed_ms + " ms");  });
  await scenario("装不撞的包时预检明确说不撞", async () => {    const other = join(TOOLS, "fixtures/other-mod.zip");
    const res = await s.invoke("precheck_install", { modIds: [other] });
    assert(res?.ok === true, "预检失败：" + JSON.stringify(res?.error));
    assert(res.data.conflicts.length === 0, "不该撞，实际：" + JSON.stringify(res.data.conflicts));
    assert(res.data.summary.includes("不撞"), "说明该说不撞，实际：" + res.data.summary);  });
  await scenario("同一个文件从另一个路径再导入一次，还是一条", async () => {    const copyDir = join(TESTDATA, "copy");
    mkdirSync(copyDir, { recursive: true });
    const copy = join(copyDir, "改了名字的副本.zip");    copyFileSync(FIXTURE, copy);
    const res = await s.invoke("import_local", { path: copy });
    assert(res?.ok === true, `第二次导入失败：${JSON.stringify(res?.error)}`);
    const list = await s.invoke("list_installed");
    assert(      list?.ok === true && list.data.length === 1,      `同一份内容不该变成两条：${JSON.stringify(list.data)}`,    );  });
  let configPath = "";
  await scenario("配置导出（不含令牌）", async () => {    const res = await s.invoke("export_config", { dir: mainDir });
    assert(res?.ok === true, `导出失败：${JSON.stringify(res?.error)}`);    configPath = res.data.path;
    assert(existsSync(configPath), `文件没落盘：${configPath}`);
    const body = readFileSync(configPath, "utf8");
    assert(res.data.count === 1, `应该 1 个 mod，实际 ${res.data.count}`);
    assert(body.includes("rock-sprout-config"), "文件里没有 kind 标记");
    if (existsSync(TOKEN)) {      const secret = readFileSync(TOKEN, "utf8").trim();
      assert(!body.includes(secret), "导出的配置里出现了令牌！");    }  });
  await scenario("配置读回来能比对", async () => {    const res = await s.invoke("import_config", { path: configPath });
    assert(res?.ok === true, `读配置失败：${JSON.stringify(res?.error)}`);
    assert(      res.data.same.length === 1 &&        res.data.only_in_file.length === 0 &&        res.data.only_local.length === 0,      `比对结果不对：${JSON.stringify(res.data)}`,    );  });
  await scenario("诊断信息里有游戏路径且不含令牌", async () => {    const res = await s.invoke("diagnostics");
    assert(res?.ok === true, `诊断失败：${JSON.stringify(res?.error)}`);
    const text = res.data;
    assert(text.includes("岩笋"), "诊断里没有版本行");
    assert(/FSD|Deep Rock Galactic/.test(text), `诊断里没看到游戏路径：\n${text.slice(0, 300)}`);
    if (existsSync(TOKEN)) {      const secret = readFileSync(TOKEN, "utf8").trim();
      assert(!text.includes(secret), "诊断信息里出现了令牌！");    }  });
  await scenario("界面点卸载后游戏回到没装 mod 的样子", async () => {    await s.clickText("button", "卸载");
    // 合并包要重写一遍，这台机器上读 2.5 GB 主 pak 约 30 秒：等它落账，别只等 1.5 秒
    let res = await s.invoke("list_installed");
    for (let i = 0; i < 560 && !(res?.ok === true && res.data.length === 0); i++) {
      await sleep(250);
      res = await s.invoke("list_installed");
    }
    const page = await s.text();
    // 卸载要重写合并包（约 30 秒），「卸掉了」那类提示是过场 toast，轮询结束时多半已经收掉；
    // 这里只要求队列已经不在处理中，真正的结果由下面的记账与诊断断言兜底。
    assert(!page.includes("正在处理"), `卸载后页面还在处理：\n${page.slice(0, 400)}`);
    assert(res?.ok === true && res.data.length === 0, `卸载后仍有 ${JSON.stringify(res.data)}`);
    const diag = await s.invoke("diagnostics");
    assert(!/合并包.*\d+ 字节/.test(diag.data), "诊断里还能看到合并包，说明没清干净");  });
  s.close();
  killApp();
  await sleep(600);}
// 断网：搜索会失败，其余照常
{
const offlineDir = freshDataDir("offline");
  if (existsSync(TOKEN)) copyFileSync(TOKEN, join(offlineDir, "modio-token.txt"));  
// 造一份「搜过 性能」的缓存：断网时后端会回放它，界面就能真的显示一屏 mod
writeSearchCache(offlineDir, "性能", 30);
  const app = await startApp({    ROCK_SPROUT_DATA: offlineDir,    HTTPS_PROXY: "http://127.0.0.1:9",    ALL_PROXY: "http://127.0.0.1:9",  });
  const s = await connect(app.wsUrl);
  await waitReady(s);
  phase("断网（代理指到死端口）");
  await scenario("连不上时搜索报 network 而不是崩", async () => {    const res = await s.invoke("search_mods", { query: "brighter", page: 1 });
    if (!existsSync(TOKEN)) {      assert(res?.ok === false, "没令牌时也该失败");      return;    }
    assert(res?.ok === false, `离线时竟然成功了：${JSON.stringify(res?.data)}`);
    assert(res.error.code === "network", `错误码应该是 network，实际 ${res.error.code}：${res.error.message}`);  });
  await scenario("断网时有缓存就回放，并标明这是旧数据", async () => {    const res = await s.invoke("search_mods", { query: "性能", page: 1 });
    assert(res?.ok === true, "回放缓存失败：" + JSON.stringify(res?.error));
    assert(res.data.stale === true, "没标记成离线回放");
    assert(res.data.items.length === 30, "应该有 30 条，实际 " + res.data.items.length);
    assert(typeof res.data.cached_at === "number", "没带抓取时间");  });
  await scenario("切到背包再切回矿脉，搜索结果还在（懒挂载）", async () => {    await gotoMine(s);
    const typeQuery = [      "(() => {",      "  const input = document.querySelector('[data-page=\"mine\"] input.input');",      "  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;",      "  setter.call(input, '性能');",      "  input.dispatchEvent(new Event('input', { bubbles: true }));",      "  return true;",      "})()",    ].join("\n");
    await s.evalJs(typeQuery);
    const countRows = "document.querySelectorAll('[data-page=\"mine\"] .vlist .row').length";
    let rows = 0;
    for (let i = 0; i < 60; i++) {      rows = await s.evalJs(countRows);
      if (rows > 0) break;
      await sleep(150);    }
    assert(rows > 0, "搜索结果没出现在列表里");
    await s.clickText("button", "背包");
    await sleep(300);
    await s.clickText("button", "矿脉");
    await sleep(300);
    const after = await s.evalJs(countRows);
    assert(after > 0, "切回来之后列表空了（懒挂载没保住状态）");  });
  await scenario("断网也能导入本地 mod", async () => {    const res = await s.invoke("import_local", { path: FIXTURE });
    assert(res?.ok === true, `断网导入失败：${JSON.stringify(res?.error)}`);
    assert(res.data.pak?.endsWith("mods_P.pak"), "没写出合并包");  });
  await scenario("断网也能导出配置", async () => {    const res = await s.invoke("export_config", { dir: offlineDir });
    assert(res?.ok === true, `断网导出失败：${JSON.stringify(res?.error)}`);
    assert(res.data.count === 1, `应该 1 个，实际 ${res.data.count}`);  });
  await scenario("断网也能卸载", async () => {    const list = await s.invoke("list_installed");
    const id = list.data[0].id;
    const res = await s.invoke("uninstall_mod", { id });
    assert(res?.ok === true, `断网卸载失败：${JSON.stringify(res?.error)}`);
    const after = await s.invoke("list_installed");
    assert(after.data.length === 0, `卸载后还有 ${JSON.stringify(after.data)}`);  });
  s.close();
  killApp();
  await sleep(600);}
// 批量操作（M2）：队列、多选、失败不整体回滚
{
const queueDir = freshDataDir("queue");  
// 有令牌才走「连不上」这条失败路径：没令牌会先卡在「还没填访问令牌」，  
// 而那条错误不回放缓存，列表就是空的，勾不到东西。
  if (existsSync(TOKEN)) copyFileSync(TOKEN, join(queueDir, "modio-token.txt"));  writeSearchCache(queueDir, "性能", 30);
  const app = await startApp({    ROCK_SPROUT_DATA: queueDir,    HTTPS_PROXY: "http://127.0.0.1:9",    ALL_PROXY: "http://127.0.0.1:9",  });
  const s = await connect(app.wsUrl);
  await waitReady(s);
  phase("批量操作（M2，断网）");
  const rowSel = '.vlist .row .pick';
  const queueText = "document.body.innerText";
  const countBad = `document.querySelectorAll('[data-page="mine"] .queue-item.bad').length`;  /** 等队列全部停下（没有「排队中/下载中/写进游戏」这类中间状态）。 */
  async function waitQueueSettled(limit = 560) {    const busyJs = `(() => {      const text = document.body.innerText;
      const shown = document.querySelectorAll('.queue-item').length > 0;
      const working = ["排队中", "先看会不会撞", "下载中", "写进游戏"].some((s) => text.includes(s))        || text.includes("正在处理");      return !shown || working;    })()`;
    for (let i = 0; i < limit; i++) {      if (!(await s.evalJs(busyJs))) return;
      await sleep(250);    }
    throw new Error("队列等了 140 秒还没停");  }  /** 点弹层里那个确认按钮；没有弹层就返回 null（装前的提醒不是每次都有）。 */
  async function confirmModalIfAny() {    const label = await s.evalJs(`(() => {      // 弹层里按钮顺序是「算了 / 就这么做」，所以取最后一个
const btns = [...document.querySelectorAll('.modal .modal-actions button')]        .filter((b) => b.getClientRects().length > 0);
      const btn = btns[btns.length - 1];
      if (!btn) return null;      btn.click();      return btn.innerText.trim();    })()`);    return label;  }  /** 批量卸载一定会弹，没有就是 bug */
  async function confirmModal() {    const label = await confirmModalIfAny();
    if (!label) throw new Error("应该弹一层确认框，但没有");    return label;  }
  await scenario("勾 3 个一起装：断网时每条都写明原因，不留半截", async () => {    await gotoMine(s);
    if (!existsSync(TOKEN)) {      console.log("        （没有令牌文件，跳过：这条路径要先过令牌检查才能走到网络失败）");      return;    }    
// 先用缓存把矿脉喂满 30 条（断网时只会走到「连不上」这一步）
    await s.evalJs(`(() => {      const input = document.querySelector('[data-page="mine"] input.input');
      const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;      setter.call(input, '性能');      input.dispatchEvent(new Event('input', { bubbles: true }));      return true;    })()`);
    for (let i = 0; i < 60; i++) {      const rows = await s.evalJs(`document.querySelectorAll('[data-page="mine"] ${rowSel}').length`);
      if (rows >= 3) break;
      await sleep(150);    }
    await s.evalJs(`(() => {      const picks = [...document.querySelectorAll('[data-page="mine"] ${rowSel}')].slice(0, 3);      picks.forEach((p) => p.click());      return picks.length;    })()`);
    await sleep(300);
    const picked = await s.evalJs(queueText);
    assert(picked.includes("选了 3 个"), `多选没生效：\n${picked.slice(0, 300)}`);
    await s.clickText("button", "全部装上");    
// 这些 id 还没下载过，后端手里没有包、算不出会不会撞，所以直接进队列；    
// 万一以后改成先下载再问，这里也要能把那层点掉。
    await confirmModalIfAny();
    await waitQueueSettled();
    const bad = await s.evalJs(countBad);
    assert(bad === 3, `应该有 3 条失败，实际 ${bad}`);
    const page = await s.evalJs(queueText);
    assert(page.includes("3 个没成"), `队列没报总数：\n${page.slice(0, 400)}`);
    assert(/连不上|网络|network/.test(page), `没写明原因：\n${page.slice(0, 600)}`);
    assert(page.includes("重试没成的"), "失败后该给「重试没成的」");
    const list = await s.invoke("list_installed");
    assert(list?.ok === true && list.data.length === 0, `失败的项不该留下记录：${JSON.stringify(list.data)}`);    
// 队列条上那条细进度：这一批全跑完了，宽度应该是满格
  const bar = await s.evalJs(`(() => {      const i = document.querySelector('[data-page="mine"] .queue-progress i');      return i ? i.style.width : null;    })()`);
    assert(bar === "100%", `进度条没走满：${bar}`);  });
  await scenario("再开一个实例：它自己退出，也不碰游戏目录", async () => {
    // 单实例守卫：第二个窗口弹一句「已经开着了」然后自己退，绝不能改游戏目录
    const pak = join(PAKS, "mods_P.pak");
    const mtimeOf = (p) => (existsSync(p) ? statSync(p).mtimeMs : 0);
    const before = [mtimeOf(pak), mtimeOf(DLL)];
    const second = spawn(EXE, [], {
      env: { ...process.env, ROCK_SPROUT_DATA: freshDataDir("second-instance") },
      stdio: "ignore",
    });
    const code = await new Promise((resolve) => {
      const timer = setTimeout(() => resolve("timeout"), 15000);
      second.on("exit", (c) => {
        clearTimeout(timer);
        resolve(c);
      });
    });
    assert(code !== "timeout", "第二个实例没有自己退出");
    const after = [mtimeOf(pak), mtimeOf(DLL)];
    assert(before[0] === after[0], "第二个实例动了合并包");
    assert(before[1] === after[1], "第二个实例动了加载器");
  });
  await scenario("全选（Ctrl+A）再批量卸掉，一次清空", async () => {    const a = await s.invoke("import_local", { path: FIXTURE });
    assert(a?.ok === true, "导入第一个失败：" + JSON.stringify(a?.error));
    const b = await s.invoke("import_local", { path: join(TOOLS, "fixtures/other-mod.zip") });
    assert(b?.ok === true, "导入第二个失败：" + JSON.stringify(b?.error));
    await s.clickText("button", "背包");
    await sleep(900);    
// Ctrl+A 在窗口上按（不在输入框里）
    await s.evalJs(`(() => {      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'a', ctrlKey: true, bubbles: true }));      return true;    })()`);
    await sleep(300);
    const bagText = await s.evalJs(queueText);
    assert(bagText.includes("选了 2 个"), `Ctrl+A 没全选：\n${bagText.slice(0, 400)}`);
    await s.clickText("button", "全部卸掉");    
// 批量卸载现在会先弹一层确认框，把那层点掉
  await confirmModal();
    await sleep(200);
    await waitQueueSettled();
    const list = await s.invoke("list_installed");
    assert(list?.ok === true && list.data.length === 0, `批量卸完还剩：${JSON.stringify(list.data)}`);
    // 后端已经空了，界面重取是异步的：给它一点时间，别读得太早
    let after = "";
    for (let i = 0; i < 25; i++) {
      after = await s.evalJs(queueText);
      if (after.includes("还没有装任何 mod")) break;
      await sleep(120);
    }
    assert(after.includes("还没有装任何 mod"), `界面没跟着更新：\n${after.slice(0, 400)}`);  });
  await scenario("部分成功：成了的留下，没成的写明原因（不整体回滚）", async () => {    const a2 = await s.invoke("import_local", { path: FIXTURE });
    assert(a2?.ok === true, "导入 A 失败：" + JSON.stringify(a2?.error));
    const b2 = await s.invoke("import_local", { path: join(TOOLS, "fixtures/other-mod.zip") });
    assert(b2?.ok === true, "导入 B 失败：" + JSON.stringify(b2?.error));    
// 先让背包把两条都读进界面，再从磁盘上把 B 的记账抽走 ——    
// B 在界面上还看得见，但它去卸载时后端会说「不在已装列表里」，正好造出「一条成一条败」。
    await s.clickText("button", "背包");
    await sleep(600);
    await s.clickText("button", "刷新");
    await sleep(900);
    const before = await s.invoke("list_installed");
    assert(before?.ok === true && before.data.length === 2, `应该 2 条：${JSON.stringify(before.data)}`);
    const gone = before.data[1];
    rmSync(join(queueDir, `mods/installed/${gone.id}.json`), { force: true });
    await s.evalJs(`(() => {      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'a', ctrlKey: true, bubbles: true }));      return true;    })()`);
    await sleep(200);
    await s.clickText("button", "全部卸掉");    
// 批量卸载现在会先弹一层确认框，把那层点掉
  await confirmModal();
    await sleep(200);
    await waitQueueSettled();
    const page = await s.evalJs(queueText);
    assert(page.includes("1 个没成"), `队列该报 1 个没成：\n${page.slice(0, 400)}`);
    assert(page.includes("好了"), `成了的那条要显示「好了」：\n${page.slice(0, 400)}`);
    assert(page.includes("不在已装列表"), `失败要写明原因：\n${page.slice(0, 600)}`);    
// 没回滚：A 该是真空了，B 的记账本来就没有
  const after = await s.invoke("list_installed");
    assert(after?.ok === true && after.data.length === 0, `成了的不该被回滚：${JSON.stringify(after.data)}`);  });
  s.close();
  killApp();
  await sleep(600);}
// 切页保留（总验收第 5 条）：勾选与滚动位置在，且不会因此重复搜索
{
const keepDir = freshDataDir("keep");
  if (existsSync(TOKEN)) copyFileSync(TOKEN, join(keepDir, "modio-token.txt"));  writeSearchCache(keepDir, "性能", 30);
  const app = await startApp({    ROCK_SPROUT_DATA: keepDir,    HTTPS_PROXY: "http://127.0.0.1:9",    ALL_PROXY: "http://127.0.0.1:9",  });
  const s = await connect(app.wsUrl);
  await waitReady(s);
  phase("切页保留（状态层）");
  await scenario("切页 10 次：勾选还在、滚动位置还在、不重复搜索", async () => {    await gotoMine(s);
// 统计后端搜索被调了几次（把 invoke 包一层，只数 search_mods）
    await s.evalJs(`(() => {      const inner = window.__TAURI_INTERNALS__.invoke;      window.__counts = { search: 0 };      window.__TAURI_INTERNALS__.invoke = (cmd, args, opts) => {        if (cmd === 'search_mods') window.__counts.search += 1;        return inner(cmd, args, opts);      };      return true;    })()`);
    await s.evalJs(`(() => {      const input = document.querySelector('[data-page="mine"] input.input');
      const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;      setter.call(input, '性能');      input.dispatchEvent(new Event('input', { bubbles: true }));      return true;    })()`);
    for (let i = 0; i < 60; i++) {      const rows = await s.evalJs(`document.querySelectorAll('[data-page="mine"] .vlist .row').length`);
      if (rows >= 3) break;
      await sleep(150);    }    
// 勾两个、把列表滚下去一点
  await s.evalJs(`(() => {      const picks = [...document.querySelectorAll('[data-page="mine"] .vlist .row .pick')].slice(0, 2);      picks.forEach((p) => p.click());
      const list = document.querySelector('[data-page="mine"] .vlist');      list.scrollTop = 400;      return picks.length;    })()`);
    await sleep(300);
    const searchBefore = await s.evalJs("window.__counts.search");
    const scrollBefore = await s.evalJs(`document.querySelector('[data-page="mine"] .vlist').scrollTop`);
    assert(scrollBefore > 0, `列表没滚动起来：${scrollBefore}`);    
// 来回切 10 次
  for (let i = 0; i < 10; i++) {      await s.clickText("button", "背包");
      await sleep(120);
      await s.clickText("button", "矿脉");
      await sleep(120);    }
    await sleep(400);
    const text = await s.evalJs("document.body.innerText");
    assert(text.includes("选了 2 个"), `切回来勾选丢了：\n${text.slice(0, 700)}`);
    const scrollAfter = await s.evalJs(`document.querySelector('[data-page="mine"] .vlist').scrollTop`);
    assert(scrollAfter === scrollBefore, `滚动位置变了：${scrollBefore} → ${scrollAfter}`);
    const searchAfter = await s.evalJs("window.__counts.search");
    assert(searchAfter === searchBefore, `切页又去搜了：${searchBefore} → ${searchAfter}`);  });
  s.close();
  killApp();
  await sleep(600);}
// 冲突界面（M4）：同一批文件被两个包改过时，得看得出谁盖了谁
{
const cDir = freshDataDir("conflict");
  const app = await startApp({    ROCK_SPROUT_DATA: cDir,    HTTPS_PROXY: "http://127.0.0.1:9",    ALL_PROXY: "http://127.0.0.1:9",  });
  const s = await connect(app.wsUrl);
  await waitReady(s);
  phase("冲突界面（M4，离线）");
  await scenario("同一批文件被两个包改过：背包里看得出谁盖了谁", async () => {    const a = await s.invoke("import_local", { path: FIXTURE });
    assert(a?.ok === true, "导入失败：" + JSON.stringify(a?.error));    
// 造第二份：内容一样、只换文件名和 id，于是两边改的是同一批文件
  const recPath = join(cDir, `mods/installed/${a.data.id}.json`);
    const rec = JSON.parse(readFileSync(recPath, "utf8"));
    const twinPath = join(cDir, `mods/local/编号二-${a.data.id % 1000000}.zip`);    copyFileSync(rec.path, twinPath);
    const twinId = a.data.id - 1;    writeFileSync(      join(cDir, `mods/installed/${twinId}.json`),      JSON.stringify(        {          ...rec,          id: twinId,          name: "编号二",          filename: "编号二.zip",          path: twinPath,          installed_at: (rec.installed_at ?? 0) + 1000,        },        null,        2,      ),    );
    await s.clickText("button", "背包");
    await sleep(600);
    await s.clickText("button", "刷新");
    await sleep(1500);
    const text = await s.evalJs("document.body.innerText");
    assert(text.includes("被覆盖"), `没标出被盖住的那个：${text.slice(0, 900)}`);
    assert(text.includes("盖了"), `没标出盖了别人的那个：${text.slice(0, 900)}`);
    await s.clickText("button", "看看谁盖了谁");
    await sleep(500);
    const drawer = await s.evalJs("document.querySelector('.drawer-card')?.innerText ?? ''");
    assert(drawer.includes("谁盖了谁"), `抽屉没打开：${drawer.slice(0, 300)}`);
    assert(/盖住|盖了/.test(drawer), `抽屉里没说清谁盖谁：${drawer.slice(0, 500)}`);
    await s.clickText("button", "知道了");
    await sleep(400);    
// 「只看有冲突的」打开后，两个都在（它俩互相冲突）
    await s.evalJs(`(() => {      const box = document.querySelector('[data-page="bag"] .checkline input[type=checkbox]');      box.click();      return true;    })()`);
    await sleep(500);
    const rows = await s.evalJs(`document.querySelectorAll('[data-page="bag"] .row').length`);
    assert(rows === 2, `两个都有冲突，应该还剩 2 行，实际 ${rows}`);  });
  s.close();
  killApp();
  await sleep(600);}
// 老记账迁移：v0.1 的本地记录（id 按路径算、没有内容指纹）在启动时应被换成新格式
{
const migrateDir = freshDataDir("migrate");  
// 一份「还在的」老记录：文件真的放在我们的 mods\local 下
const keepFile = join(migrateDir, "mods/local/旧文件-123456.pak");
  mkdirSync(dirname(keepFile), { recursive: true });  writeFileSync(keepFile, "real-pak-content");
  const legacyId = 0x0011111111111111;  writeFileSync(    join(migrateDir, `mods/installed/${legacyId}.json`),    JSON.stringify(      {        id: legacyId,        name: "旧文件",        file_id: 0,        filename: "旧文件-123456.pak",        size: 16,        path: keepFile,        source: "local",      },      null,      2,    ),  );  
// 一份「文件已经不在的」老记录
const goneId = 0x0012222222222222;  writeFileSync(    join(migrateDir, `mods/installed/${goneId}.json`),    JSON.stringify(      {        id: goneId,        name: "已经删掉的",        file_id: 0,        filename: "已经删掉的-654321.pak",        size: 1,        path: join(migrateDir, "没有这个文件.pak"),        source: "local",      },      null,      2,    ),  );
  const app = await startApp({ ROCK_SPROUT_DATA: migrateDir });
  const s = await connect(app.wsUrl);
  await waitReady(s);
  phase("老记账迁移（启动时自动跑一次）");
  const listed = await s.invoke("list_installed");
  const items = listed?.ok === true ? listed.data : [];
  const kept = items.find((m) => m.name === "旧文件");
  const gone = items.find((m) => m.name === "已经删掉的");
  await scenario("本地老记录换成了按内容算的新 id", async () => {    assert(kept, `迁移后找不到这条记录了：${JSON.stringify(listed)}`);
    assert(kept.id !== legacyId, "id 没换（还是按路径算的老值）");
    assert(kept.missing === false, "文件明明还在，不该标成缺失");
    assert(!existsSync(join(migrateDir, `mods/installed/${legacyId}.json`)), "老记账文件没清掉");  });
  await scenario("原文件已经不在的记录标成缺失但没删掉", async () => {    assert(gone, "缺失的记录被删了——迁移不该删记录");
    assert(gone.missing === true, "应该标成 missing");  });
  await scenario("迁移前留了带时间戳的备份目录", async () => {    const backups = readdirSync(join(migrateDir, "mods")).filter((n) => n.startsWith("installed.bak_"));
    assert(backups.length === 1, `应该有一个备份目录，实际：${JSON.stringify(backups)}`);  });
  await scenario("迁移只跑一次（标记文件在，不会每次重扫）", async () => {    assert(existsSync(join(migrateDir, "migrated_v2")), "没有写 migrated_v2 标记");  });
  s.close();
  killApp();
  await sleep(600);}
// ---------- 第三组：v0.1.2（每个阶段往这里加一条） ----------
//
// v0.1.2 主题「敢用」：原子写、快照回滚、更新检测、进程检查、体检、陌生产物。
// 现在只放两条基线，后面每做完一个阶段，往上一段里加它自己的场景。
{
const v3Dir = freshDataDir("v3");
  const app = await startApp({ ROCK_SPROUT_DATA: v3Dir });
  const s = await connect(app.wsUrl);
  await waitReady(s);
  phase("v0.1.2 场景");
  await scenario("装完一个 mod 之后，Paks 目录里没有 .tmp / .prev 残留", async () => {    const res = await s.invoke("import_local", { path: FIXTURE });
    assert(res?.ok === true, "导入失败：" + JSON.stringify(res?.error));
    const junk = readdirSync(PAKS).filter((n) => n.endsWith(".tmp") || n.endsWith(".prev"));
    assert(junk.length === 0, `Paks 里留了临时文件：${JSON.stringify(junk)}`);  });
  await scenario("基线数字：原 pak / 合并包 / 加载器各多大", async () => {    const main = join(PAKS, "FSD-WindowsNoEditor.pak");
    const pak = join(PAKS, "mods_P.pak");
    assert(existsSync(main), "原 pak 不见了");
    assert(existsSync(pak), "合并包没生成");
    console.log(      `       原 pak ${statSync(main).size} / 合并包 ${statSync(pak).size} / 加载器 ${statSync(DLL).size} 字节`,    );  });
  s.close();
  killApp();
  await sleep(600);  
// 上次写到一半断掉会留下 .tmp / .prev，启动时应该自己收拾干净
writeFileSync(join(PAKS, "mods_P.pak.tmp"), "half");  writeFileSync(join(PAKS, "mods_P.pak.prev"), "old");
  const app2 = await startApp({ ROCK_SPROUT_DATA: v3Dir });
  const s2 = await connect(app2.wsUrl);
  await waitReady(s2);
  await scenario("启动时清掉上次留下的 .tmp / .prev", async () => {    assert(!existsSync(join(PAKS, "mods_P.pak.tmp")), "启动后 .tmp 还在");
    assert(!existsSync(join(PAKS, "mods_P.pak.prev")), "启动后 .prev 还在");
    const pak = join(PAKS, "mods_P.pak");
    assert(existsSync(pak), "清理时把正式包也弄没了");
    const size = statSync(pak).size;
    assert(size > 1_000_000, `正式包变小了，像是被覆盖过：${size} 字节`);  });  
// 写不进去时（拿一个同名目录冒充临时包，模拟磁盘/权限出错），旧包必须原样还在
await scenario("合并失败时旧包不动、游戏目录不留半截文件", async () => {    const pak = join(PAKS, "mods_P.pak");
    const before = { size: statSync(pak).size, mtime: statSync(pak).mtimeMs };
    const blocker = join(PAKS, "mods_P.pak.tmp");
    mkdirSync(blocker);
    try {      const res = await s2.invoke("import_local", { path: FIXTURE });
      assert(res?.ok === false, `写不进去居然还成功了：${JSON.stringify(res?.data)}`);
      assert(typeof res.error?.message === "string" && res.error.message.length > 0, "失败没给出原因");
      const after = { size: statSync(pak).size, mtime: statSync(pak).mtimeMs };
      assert(after.size === before.size, `旧包大小变了：${before.size} → ${after.size}`);
      assert(after.mtime === before.mtime, "旧包被重写过（修改时间变了）");
      console.log(`       失败原因：${res.error.message}`);    } finally {      rmSync(blocker, { recursive: true, force: true });    }  });  s2.close();
  killApp();
  await sleep(600);}
// ---------- 第三组续：快照与回滚（v0.1.2 M3-2） ----------
//
// 这里只用导入本地包来造现场（不依赖网络），验的是「装坏了能退回来」：
// 每次装/导入/卸之前自动拍快照，退回时合并包和记账一起退。
{
const snapDir = freshDataDir("snap");
  const app = await startApp({ ROCK_SPROUT_DATA: snapDir });
  const s = await connect(app.wsUrl);
  await waitReady(s);
  phase("快照与回滚");
  const OTHER = join(TOOLS, "fixtures/other-mod.zip");
  const pakPath = join(PAKS, "mods_P.pak");  async function snapshots() {    const res = await s.invoke("list_snapshots");
    assert(res?.ok === true, "列快照失败：" + JSON.stringify(res?.error));    return res.data ?? [];  }
  await scenario("导入两个 mod 之前都自动留了现场（合并包 + 记账）", async () => {    for (const p of [FIXTURE, OTHER]) {      const res = await s.invoke("import_local", { path: p });
      assert(res?.ok === true, "导入失败：" + JSON.stringify(res?.error));    }
    const list = await snapshots();
    assert(list.length >= 2, `自动快照太少：${list.length} 份`);
    const newest = list[0];
    const dir = newest.dir;
    assert(existsSync(join(dir, "installed")), "快照里没有记账目录");
    assert(existsSync(join(dir, "meta.json")), "快照里没有 meta.json");
    assert(newest.meta.mods >= 1, `快照记的 mod 数量不对：${newest.meta.mods}`);
    console.log(`       自动快照 ${list.length} 份，最新一份 ${newest.meta.reason}（${newest.meta.mods} 个 mod）`);  });
  await scenario("手动保存点不会被自动清理清掉", async () => {    const made = await s.invoke("create_snapshot", { reason: "手动保存点" });
    assert(made?.ok === true, "存保存点失败：" + JSON.stringify(made?.error));    
// 再导两次，制造超过 3 份自动快照的局面
  for (const p of [FIXTURE, OTHER]) {      await s.invoke("import_local", { path: p });    }
    const list = await snapshots();
    const manual = list.filter((x) => x.meta.manual);
    const auto = list.filter((x) => !x.meta.manual);
    assert(manual.length === 1, `手动保存点应该正好 1 份，实际 ${manual.length}`);
    assert(auto.length <= 3, `自动快照超过 3 份了：${auto.length}`);  });
  s.close();
  killApp();
  await sleep(600);  
// 回滚要验「退回之前那一刻也在」，所以换一个干净的目录、只导一次，  
// 这样自动快照不会因为「只留 3 份」把要退的那份清掉。
  const rollDir = freshDataDir("snap-rollback");
  const app3 = await startApp({ ROCK_SPROUT_DATA: rollDir });
  const s3 = await connect(app3.wsUrl);
  await waitReady(s3);  async function snapshots3() {    const res = await s3.invoke("list_snapshots");
    assert(res?.ok === true, "列快照失败：" + JSON.stringify(res?.error));    return res.data ?? [];  }
  await scenario("退回到「一个都没装」的那一刻：列表和合并包一起退", async () => {    const res = await s3.invoke("import_local", { path: FIXTURE });
    assert(res?.ok === true, "导入失败：" + JSON.stringify(res?.error));
    const list = await snapshots3();
    assert(list.length === 1, `应该只有 1 份自动快照，实际 ${list.length}`);
    const oldest = list[0];
    assert(oldest.meta.mods === 0, `导入前的快照应该记 0 个 mod，实际 ${oldest.meta.mods}`);
    const back = await s3.invoke("rollback_snapshot", { id: oldest.id });
    assert(back?.ok === true, "回滚失败：" + JSON.stringify(back?.error));
    assert(back.data.mods === 0, `回滚后应该是 0 个 mod，实际 ${back.data.mods}`);
    const installed = await s3.invoke("list_installed");
    assert(installed.data.length === 0, `回滚后已装列表还有 ${installed.data.length} 条`);
    assert(!existsSync(pakPath), "回滚到一个都没装的状态，合并包应该消失");
    const after = await snapshots3();
    const before = after.find((x) => x.meta.reason === "回滚前");
    assert(before, "回滚前没留现场，退错了就回不去了");
    assert(before.meta.mods === 1, `「回滚前」应该记 1 个 mod，实际 ${before.meta.mods}`);
    console.log(`       退回前的那一刻存在 ${before.id}（${before.meta.mods} 个 mod）`);  });
  await scenario("再退回到「回滚前」：刚才那个 mod 又回来（连着退两次）", async () => {    const list = await snapshots3();
    const before = list.find((x) => x.meta.reason === "回滚前");
    assert(before, "找不到「回滚前」那份");
    const res = await s3.invoke("rollback_snapshot", { id: before.id });
    assert(res?.ok === true, "第二次回滚失败：" + JSON.stringify(res?.error));
    assert(res.data.mods === 1, `应该回到 1 个 mod，实际 ${res.data.mods}`);
    assert(existsSync(pakPath), "回到 1 个 mod 之后合并包应该又有了");
    const junk = readdirSync(PAKS).filter((n) => n.endsWith(".tmp") || n.endsWith(".prev"));
    assert(junk.length === 0, `Paks 里留了临时文件：${JSON.stringify(junk)}`);  });
  await scenario("退回之后游戏目录能正常用（记账与合并包对得上）", async () => {    const installed = await s3.invoke("list_installed");
    assert(installed?.ok === true, "读已装列表失败");
    const names = (installed.data ?? []).map((m) => m.name);
    assert(names.length === 1, `已装列表应该是 1 条，实际 ${names.length}`);
    assert(existsSync(pakPath), "合并包不在");
    const size = statSync(pakPath).size;
    assert(size > 1_000_000, `合并包太小，像是空的：${size} 字节`);
    console.log(`       已装 ${names.join(" / ")}，合并包 ${size} 字节`);  });  s3.close();
  killApp();
  await sleep(600);}
// ---------- 第三组续：游戏本体更新检测（v0.1.2 M3-3） ----------
//
// 合并包是拿「这一版」游戏的主 pak 合成出来的，游戏一更新，旧包和旧快照就成了毒。
// 这里用「把主 pak 的修改时间往后拨 5 秒」来假装游戏更新过，看提示会不会出来、撤销会不会按灰。
{
const gsDir = freshDataDir("gstate");
  const mainPak = join(PAKS, "FSD-WindowsNoEditor.pak");
  const before = statSync(mainPak);
  const app = await startApp({ ROCK_SPROUT_DATA: gsDir });
  const s = await connect(app.wsUrl);
  await waitReady(s);
  phase("游戏更新检测");
  await scenario("装一次之后会记下主 pak 的大小和时间", async () => {    const res = await s.invoke("import_local", { path: FIXTURE });
    assert(res?.ok === true, "导入失败：" + JSON.stringify(res?.error));
    const stateFile = join(gsDir, "game-state.json");
    assert(existsSync(stateFile), "game-state.json 没生成");
    const saved = JSON.parse(readFileSync(stateFile, "utf8"));
    assert(saved.main_pak_size === before.size, `记的大小不对：${saved.main_pak_size}`);
    assert(typeof saved.main_pak_mtime === "number" && saved.main_pak_mtime > 0, "没记修改时间");
    const check = await s.invoke("game_status");
    assert(check?.ok === true, "读游戏状态失败");
    assert(check.data.changed === false, `刚装完不该说游戏变了：${JSON.stringify(check.data)}`);
    console.log(`       记下 ${saved.main_pak_size} 字节 / ${saved.main_pak_mtime}`);  });
  s.close();
  killApp();
  await sleep(600);  
// 假装游戏更新：把主 pak 的修改时间往后拨 5 秒
const later = new Date(before.mtimeMs + 5_000);  utimesSync(mainPak, later, later);
  const app2 = await startApp({ ROCK_SPROUT_DATA: gsDir });
  const s2 = await connect(app2.wsUrl);
  await waitReady(s2);
  await scenario("主 pak 的时间一变，读出来的状态就说「更新过」", async () => {    const check = await s2.invoke("game_status");
    assert(check?.ok === true, "读游戏状态失败");
    assert(check.data.changed === true, `应该报游戏更新过：${JSON.stringify(check.data)}`);
    assert(check.data.summary.includes("更新"), `结论里没提更新：${check.data.summary}`);
    console.log(`       结论：${check.data.summary}`);  });
  await scenario("界面顶部出现「游戏更新过」的提示条", async () => {    const bannerText = await s2.evalJs(      `(() => { const b = document.querySelector(".banner.warn"); return b ? b.innerText : null; })()`,    );
    assert(bannerText, "顶部没有提示条");
    assert(bannerText.includes("更新"), `提示条内容不对：${bannerText}`);
    console.log(`       提示条：${bannerText.replace(/\n/g, " ")}`);  });
  await scenario("设置页里「退回到这里」按灰（旧备份不能用了）", async () => {    await s2.clickText(".nav-item, button", "设置");
    let disabled = null;
    for (let i = 0; i < 25; i++) {
      disabled = await s2.evalJs(
        `(() => {
          const all = [...document.querySelectorAll("button")]
            .filter((e) => e.innerText.includes("退回到这里"));
          if (all.length === 0) return null;
          return all.every((e) => e.disabled);
        })()`,
      );
      if (disabled !== null) break;
      await sleep(120);
    }
    assert(disabled === true, `回滚按钮应该全部按灰，实际 ${JSON.stringify(disabled)}`);
    const hint = await s2.text();
    assert(hint.includes("旧备份不能用了"), "没有告诉用户为什么不能退");  });  
// 把主 pak 的时间改回去，别把用户机器搞成「天天提示游戏更新过」
  utimesSync(mainPak, before.atime, before.mtime);
  await scenario("时间改回去之后就不提示了（说明判断真的按指纹来）", async () => {    const check = await s2.invoke("game_status");
    assert(check?.ok === true, "读游戏状态失败");
    assert(check.data.changed === false, `改回去了还说变了：${JSON.stringify(check.data)}`);  });  s2.close();
  killApp();
  await sleep(600);}
// ---------- 第三组续：游戏在跑就拦下来（v0.1.2 M3-4） ----------
//
// 游戏开着时它攥着 mods_P.pak 不放，替换文件只会拿到 os error 32。
// 这里没法真开游戏（Steam 那份 8 G，而且会把用户踢下线），
// 所以把一个 powershell.exe 复制成 FSD-Win64-Shipping.exe 跑起来冒充——
// 进程名就是文件名，tasklist 看到的名字和真游戏一样。
{
const runDir = freshDataDir("running");
  const fakeDir = join(TOOLS, "_fakeproc");
  mkdirSync(fakeDir, { recursive: true });
  const fakeExe = join(fakeDir, "FSD-Win64-Shipping.exe");
  const ps = join(process.env.SystemRoot ?? "C:/Windows", "System32/WindowsPowerShell/v1.0/powershell.exe");  copyFileSync(ps, fakeExe);
  const app = await startApp({ ROCK_SPROUT_DATA: runDir });
  const s = await connect(app.wsUrl);
  await waitReady(s);
  phase("游戏在跑时拦下来");
  await scenario("没跑游戏的时候一切照常（对照组）", async () => {    const res = await s.invoke("precheck_install", { modIds: [FIXTURE] });
    assert(res?.ok === true, `预检失败：${JSON.stringify(res?.error)}`);  });  
// 把冒充的「游戏」跑起来，撑 12 秒
const fake = spawn(fakeExe, ["-NoProfile", "-Command", "Start-Sleep -Seconds 12"], {    stdio: "ignore",    windowsHide: true,  });
  await sleep(1200);
  await scenario("装的时候拦住，话里说清「先关游戏」", async () => {    const res = await s.invoke("import_local", { path: FIXTURE });
    assert(res?.ok === false, "游戏在跑居然没拦");
    assert(res.error.code === "game_running", `错误码不对：${res.error.code}`);
    assert(res.error.message.includes("正在运行"), `话不对：${res.error.message}`);
    console.log(`       提示：${res.error.message}`);  });
  await scenario("卸载和回滚也一样被拦住", async () => {    const un = await s.invoke("uninstall_mod", { id: 1 });
    assert(un?.ok === false && un.error.code === "game_running", `卸载没拦：${JSON.stringify(un?.error)}`);
    const rb = await s.invoke("rollback_snapshot", { id: "nope" });
    assert(rb?.ok === false && rb.error.code === "game_running", `回滚没拦：${JSON.stringify(rb?.error)}`);  });
  await scenario("下载这一步也被拦住（批量装时不会先下一半）", async () => {    const dl = await s.invoke("download_mod", { id: 1 });
    assert(dl?.ok === false && dl.error.code === "game_running", `下载没拦：${JSON.stringify(dl?.error)}`);  });  
// 关掉冒充的游戏，别留着占进程
try {    fake.kill();  } catch {    /* 已经退了 */
  }  spawnSync("taskkill", ["/F", "/IM", "FSD-Win64-Shipping.exe"]);
  await sleep(800);
  await scenario("游戏关掉之后又能装了", async () => {    const res = await s.invoke("import_local", { path: FIXTURE });
    assert(res?.ok === true, `关掉游戏后还是装不了：${JSON.stringify(res?.error)}`);  });
  s.close();
  killApp();
  await sleep(600);  
// 冒充用的 exe 删掉，别在 _tools 里留 1 MB 的假游戏
rmSync(fakeDir, { recursive: true, force: true });}
// ---------- 第三组续：体检能查出来、能处理掉（v0.1.2 M3-5） ----------
//
// 三条最常见的「不对劲」：记录还在文件没了、上次合并留了半截文件、下载目录里有没人认领的文件。
// 注意残留那条：应用启动时自己会清 .tmp/.prev，所以要等应用起来之后再往 Paks 里放。
{
const auditDir = freshDataDir("audit");
  const app = await startApp({ ROCK_SPROUT_DATA: auditDir });
  const s = await connect(app.wsUrl);
  await waitReady(s);
  phase("体检");  
// 先装一个，再把它下载到的文件删掉——制造「记录在、文件没了」
  const imported = await s.invoke("import_local", { path: FIXTURE });
  assert(imported?.ok === true, `导入失败：${JSON.stringify(imported?.error)}`);
  const recFile = join(auditDir, "mods/installed", `${imported.data.id}.json`);
  assert(existsSync(recFile), "记账文件不在");
  const rec = JSON.parse(readFileSync(recFile, "utf8"));
  rmSync(rec.path, { force: true });  
// 记录里的 missing 是启动迁移时算的，这里手动标成「文件不在了」，  
// 走的就是用户「自己删了下载目录」那条路
rec.missing = true;
  writeFileSync(recFile, JSON.stringify(rec, null, 2));
  await scenario("记录还在、文件没了——体检说得出来", async () => {    const res = await s.invoke("audit_game");
    assert(res?.ok === true, `体检失败：${JSON.stringify(res?.error)}`);
    const hit = res.data.issues.find((i) => i.kind === "record_missing");
    assert(hit, `没报「文件没了」：${JSON.stringify(res.data.issues)}`);
    assert(hit.fixable === true && hit.action === "drop_record", "这条应该能处理");
    assert(hit.target === String(imported.data.id), `目标不对：${hit.target}`);
    console.log(`       ${hit.title}`);  });
  await scenario("点一下就把空记录清掉", async () => {    const res = await s.invoke("fix_audit_issue", { action: "drop_record", target: String(imported.data.id) });
    assert(res?.ok === true, `处理失败：${JSON.stringify(res?.error)}`);
    assert(!existsSync(recFile), "记账文件还在");
    const again = await s.invoke("audit_game");
    assert(      !again.data.issues.some((i) => i.kind === "record_missing"),      "处理完还说文件没了",    );  });
  await scenario("往 Paks 里丢一个半截文件，体检能看见并能清掉", async () => {    const tmp = join(PAKS, "mods_P.pak.tmp");    writeFileSync(tmp, "半截");
    const res = await s.invoke("audit_game");
    const hit = res.data.issues.find((i) => i.kind === "residue");
    assert(hit, `没报残留：${JSON.stringify(res.data.issues)}`);
    assert(hit.detail.includes("mods_P.pak.tmp"), `没写清是哪个文件：${hit.detail}`);
    const fixed = await s.invoke("fix_audit_issue", { action: "clean_residue", target: "" });
    assert(fixed?.ok === true, `清理失败：${JSON.stringify(fixed?.error)}`);
    assert(!existsSync(tmp), "残留文件还在");  });
  await scenario("没人认领的文件会被挪进备份目录（不是删掉）", async () => {    const dir = join(auditDir, "mods");
    mkdirSync(dir, { recursive: true });
    const stray = join(dir, "谁丢的.zip");    writeFileSync(stray, "x");
    const res = await s.invoke("audit_game");
    const hit = res.data.issues.find((i) => i.kind === "orphan");
    assert(hit, `没报孤儿文件：${JSON.stringify(res.data.issues)}`);
    assert(hit.detail.includes("谁丢的.zip"), `没写清哪个文件：${hit.detail}`);
    const fixed = await s.invoke("fix_audit_issue", { action: "clean_orphans", target: "" });
    assert(fixed?.ok === true, `处理失败：${JSON.stringify(fixed?.error)}`);
    assert(!existsSync(stray), "原位置应该清空");
    const kept = readdirSync(join(auditDir, "game-backup"), { recursive: true })      .map((p) => String(p))      .some((p) => p.includes("谁丢的.zip"));
    assert(kept, "备份目录里没找到它——说好的是挪走不是删掉");  });
  await scenario("体检结论里把「几个 mod、合并包在不在」说清楚", async () => {    const res = await s.invoke("audit_game");
    assert(res.data.summary.includes("个 mod"), `结论不对：${res.data.summary}`);
    console.log(`       ${res.data.summary}`);  });
  await scenario("设置页里那块「体检」看得见结论", async () => {    await s.clickText(".nav-item, button", "设置");
    const text = await s.text();
    assert(text.includes("体检"), "设置页没有体检那块");
    assert(text.includes("个 mod"), "体检结论没显示出来");  });
  s.close();
  killApp();
  await sleep(600);}
// ---------- 第三组续：游戏目录里的陌生产物（v0.1.2 M3-6） ----------
//
// 这里只验「认不认得出来」，**不点「处理掉」**——那样会把别家工具留下的包挪走，
// 那可是用户自己的东西。认出来之后，处理动作和孤儿文件是同一套挪动逻辑（上一组验过）。
{
const strayDir = freshDataDir("stray");
  const app = await startApp({ ROCK_SPROUT_DATA: strayDir });
  const s = await connect(app.wsUrl);
  await waitReady(s);
  phase("陌生产物");
  const mine = join(PAKS, "zz-m36-dummy.pak");  writeFileSync(mine, "not really a pak");
  await scenario("游戏目录里多出来的 pak 会被点名", async () => {    const res = await s.invoke("audit_game");
    assert(res?.ok === true, `体检失败：${JSON.stringify(res?.error)}`);
    const hit = res.data.issues.find((i) => i.kind === "stray");
    assert(hit, `没报陌生产物：${JSON.stringify(res.data.issues.map((i) => i.kind))}`);
    assert(hit.detail.includes("zz-m36-dummy.pak"), `没点名：${hit.detail}`);
    console.log(`       ${hit.title}`);  });  
// 顺手确认：这台机器上 MintCat 留下的那个包也在名单里（在的话才算数）
  const mintcat = join(PAKS, "FSD-WindowsNoEditor_Mods.pak");
  if (existsSync(mintcat)) {    await scenario("别家工具留下的 150 MB 包也被认出来（不处理，只点名）", async () => {      const res = await s.invoke("audit_game");
      const hit = res.data.issues.find((i) => i.kind === "stray");
      assert(hit && hit.detail.includes("FSD-WindowsNoEditor_Mods.pak"), `没认出 MintCat 那个包：${hit?.detail}`);
      assert(existsSync(mintcat), "只报不挪，文件应该还在");    });  }  
// 自己造的假包自己清掉
rmSync(mine, { force: true });
  await scenario("清掉之后就不再提它了", async () => {    const res = await s.invoke("audit_game");
    const hit = res.data.issues.find((i) => i.kind === "stray");
    assert(!hit || !hit.detail.includes("zz-m36-dummy.pak"), "假包删了还在报");  });
  s.close();
  killApp();
  await sleep(600);}
// ---------- 第三组续：数据目录落哪儿（v0.1.3 M2） ----------
//
// 绿色程序的数据本该在 exe 旁边；但装在 Program Files 里就写不进去了，得退到用户目录。
// 这里不开真权限，用「同名文件占住 data 这个名字」来模拟写不了：create_dir_all 会失败，
// 和只读目录是同一个效果，还不影响别的测试。
{
const base = join(TESTDATA, "datadir");
  rmSync(base, { recursive: true, force: true });
  const portableDir = join(base, "portable");
  const blockedDir = join(base, "blocked");
  const fakeLocal = join(base, "localappdata");
  mkdirSync(portableDir, { recursive: true });
  mkdirSync(blockedDir, { recursive: true });
  mkdirSync(fakeLocal, { recursive: true });
  const exe1 = join(portableDir, "rock-sprout.exe");
  const exe2 = join(blockedDir, "rock-sprout.exe");  copyFileSync(EXE, exe1);  copyFileSync(EXE, exe2);  
// 占住 data 这个名字，让程序建不出目录
writeFileSync(join(blockedDir, "data"), "not a dir");
  phase("数据目录（v0.1.3）");  
// 1. 正常情况：数据就在 exe 旁边
{
  const app = await startApp({ ROCK_SPROUT_DATA: "" }, { exe: exe1, cwd: portableDir });
    const s = await connect(app.wsUrl);
    await waitReady(s);
    await scenario("exe 放能写的地方：数据就在旁边，报 portable", async () => {      const res = await s.invoke("data_location");
      assert(res?.ok === true, `查数据目录失败：${JSON.stringify(res?.error)}`);
      assert(res.data.mode === "portable", `模式不对：${res.data.mode}`);
      assert(res.data.portable === true && res.data.fell_back === false, "标志位不对");
      assert(        res.data.path.toLowerCase().startsWith(portableDir.toLowerCase()),        `路径不在 exe 旁边：${res.data.path}`,      );
      assert(existsSync(join(portableDir, "data", "logs")), "data\\logs 没建出来");    });
    s.close();
    killApp();
    await sleep(600);  }  
// 2. 程序目录写不了：退到用户目录，并把这个事实报告出来
{
  const app = await startApp(      { ROCK_SPROUT_DATA: "", LOCALAPPDATA: fakeLocal },      { exe: exe2, cwd: blockedDir },    );
    const s = await connect(app.wsUrl);
    await waitReady(s);
    await scenario("程序目录写不了：退到用户目录，界面能知道", async () => {      const res = await s.invoke("data_location");
      assert(res?.ok === true, `查数据目录失败：${JSON.stringify(res?.error)}`);
      assert(res.data.mode === "fallback", `应该退到用户目录，实际：${res.data.mode}`);
      assert(res.data.fell_back === true, "没有标成退让");
      assert(        res.data.path.toLowerCase().startsWith(fakeLocal.toLowerCase()),        `没落在 LOCALAPPDATA 下：${res.data.path}`,      );
      assert(!existsSync(join(blockedDir, "data", "logs")), "居然写进了程序目录");    });
    await scenario("诊断信息里写明数据模式", async () => {      const res = await s.invoke("diagnostics");
      assert(res?.ok === true, `取诊断失败：${JSON.stringify(res?.error)}`);
      assert(res.data.includes("数据模式：fallback"), "诊断里没有数据模式这一行");
      assert(!res.data.includes("ghp_"), "诊断里混进了令牌");    });
    s.close();
    killApp();
    await sleep(600);  }
  rmSync(base, { recursive: true, force: true });}
// ---------- 第三组续：旧版本留下的数据（v0.1.3 M2b） ----------
//
// 造一个「老版本留下的数据目录」放在 LOCALAPPDATA 下，把 LOCALAPPDATA 指到一个临时目录，
// 让程序扫到它、问一次、沿用；沿用之后再启动就不该再问。
{
const base = join(TESTDATA, "legacy");
  rmSync(base, { recursive: true, force: true });
  const fakeLocal = join(base, "localappdata");
  const oldDir = join(fakeLocal, "RockSprout", "data");
  const curDir = join(base, "current");
  mkdirSync(join(oldDir, "mods", "installed"), { recursive: true });
  mkdirSync(join(oldDir, "snapshots", "a"), { recursive: true });
  mkdirSync(curDir, { recursive: true });  
// 老数据：两条记账 + 一份快照 + 令牌
for (const id of ["1", "2"]) {    writeFileSync(join(oldDir, "mods", "installed", `${id}.json`), JSON.stringify({ id: Number(id) }));  }  writeFileSync(join(oldDir, "modio-token.txt"), "old-token\n");
  phase("旧数据（v0.1.3）");  {    const app = await startApp({ ROCK_SPROUT_DATA: curDir, LOCALAPPDATA: fakeLocal });
    const s = await connect(app.wsUrl);
    await waitReady(s);
    await scenario("扫到旧数据，并说清有几个 mod、几份快照", async () => {      const res = await s.invoke("legacy_data");
      assert(res?.ok === true, `扫描失败：${JSON.stringify(res?.error)}`);
      assert(res.data, "没扫到旧数据");
      assert(res.data.mods === 2, `mod 数不对：${res.data.mods}`);
      assert(res.data.snapshots === 1, `快照数不对：${res.data.snapshots}`);
      assert(res.data.has_token === true, "没认出令牌");    });
    await scenario("设置页里那块「发现旧版本的数据」看得见", async () => {      await s.clickText(".rail button", "设置");
      await sleep(500);
      const text = await s.text();
      assert(text.includes("发现旧版本的数据"), "设置页没有这块卡片");
      assert(text.includes("沿用"), "没有沿用按钮");    });
    await scenario("点沿用：旧数据被复制过来，旧目录一个字节没动", async () => {      const res = await s.invoke("resolve_legacy", { path: oldDir, adopt: true });
      assert(res?.ok === true, `沿用失败：${JSON.stringify(res?.error)}`);
      assert(existsSync(join(curDir, "modio-token.txt")), "令牌没复制过来");
      assert(        readdirSync(join(curDir, "mods", "installed")).length === 2,        "记账没复制全",      );
      assert(existsSync(join(oldDir, "modio-token.txt")), "旧目录被动过了");
      assert(existsSync(join(curDir, "data-adopted.txt")), "没写记号文件");    });
    await scenario("沿用之后再问就没有了", async () => {      const res = await s.invoke("legacy_data");
      assert(res?.ok === true, `扫描失败：${JSON.stringify(res?.error)}`);
      assert(res.data === null, `还在提示：${JSON.stringify(res.data)}`);    });
    s.close();
    killApp();
    await sleep(600);  }  
// 选「不用」的路径：换个干净目录，选不用之后同样不再提示
{
  const cur2 = join(base, "current2");
    mkdirSync(cur2, { recursive: true });
    const app = await startApp({ ROCK_SPROUT_DATA: cur2, LOCALAPPDATA: fakeLocal });
    const s = await connect(app.wsUrl);
    await waitReady(s);
    await scenario("点不用：写 declined 记号，之后也不再提示", async () => {      const found = await s.invoke("legacy_data");
      assert(found?.ok === true && found.data, "第二次没扫到旧数据（记号被误写？）");
      const res = await s.invoke("resolve_legacy", { path: oldDir, adopt: false });
      assert(res?.ok === true, `选不用失败：${JSON.stringify(res?.error)}`);
      const text = readFileSync(join(cur2, "data-adopted.txt"), "utf8");
      assert(text.startsWith("declined"), `记号不对：${text}`);
      const again = await s.invoke("legacy_data");
      assert(again?.ok === true && again.data === null, "选不用之后还在提示");    });
    s.close();
    killApp();
    await sleep(600);  }  
// 报告问题：只放行 http/https（别的协议一律拒绝，省得这个命令变成开任意程序的口子）
{
  const reportDir = join(base, "report");
    mkdirSync(reportDir, { recursive: true });
    const app = await startApp({ ROCK_SPROUT_DATA: reportDir });
    const s = await connect(app.wsUrl);
    await waitReady(s);
    phase("报告问题（v0.1.3）");
    await scenario("open_url 只认 http/https：本地路径和别的协议都被拒", async () => {      const bad = await s.invoke("open_url", { url: "file:///C:/Windows/System32/calc.exe" });
      assert(bad?.ok === false, "本地文件居然被放行");
      assert(bad.error?.code === "bad_url", `错误码不对：${JSON.stringify(bad.error)}`);
      const js = await s.invoke("open_url", { url: "javascript:alert(1)" });
      assert(js?.ok === false && js.error?.code === "bad_url", "javascript: 没被拒");    });
    await scenario("设置页里看得见「报告问题」按钮", async () => {      await s.clickText(".rail button", "设置");
      await sleep(500);
      const text = await s.text();
      assert(text.includes("报告问题"), "设置页没有报告问题按钮");    });
    await scenario("「怎么拿令牌」只有一个网址，且点了交给浏览器（不是 <a>）", async () => {
      // v0.1.8 起这里不再是 <a href>：在窗口里点 <a> 会把界面导航走，所以换成按钮走 open_url。
      const openers = await s.evalJs(
        `document.querySelectorAll('.steps .link-btn[data-url="https://mod.io/me/access"]').length`,
      );
      assert(openers === 1, `「怎么拿令牌」里应该只有一个可点的入口，实际 ${openers} 个`);
      const anchors = await s.evalJs(`document.querySelectorAll('.steps a[href^="http"]').length`);
      assert(anchors === 0, `「怎么拿令牌」里不该再有 <a href>，实际 ${anchors} 个`);
      const raw = await s.text();
      const hits = raw.split("https://mod.io/me/access").length - 1;
      assert(hits === 1, `网址在设置页出现了 ${hits} 次，重复了`);
    });
    await scenario("「当前状态」验不过时走行内错误提示，不把原始报错当正文", async () => {
      const card = await s.evalJs(`(() => {
        const title = [...document.querySelectorAll('.card-title')].find((n) => n.textContent.includes('当前状态'));
        if (!title) return null;
        const box = title.parentElement;
        return {
          hint: !!box.querySelector('.hint-err'),
          note: !!box.querySelector('.note'),
          text: box.innerText,
        };
      })()`);
      assert(card, "设置页没有「当前状态」卡");
      // 有令牌但连不上时：卡片里应该是红点提示，正文那句「连不上…先确认网是通的」而不是一整段原始报错
      if (card.hint) {
        assert(!card.note, "既有行内提示又有一整段正文，重复了");
        assert(/连不上/.test(card.text), `提示里没说到连不上：${card.text.replace(/\n/g, " | ")}`);
        assert(/先确认网是通的/.test(card.text), "提示里没给建议");
      } else {
        assert(/还没填|读状态/.test(card.text), `没令牌时该说「还没填」，实际：${card.text.replace(/\n/g, " | ")}`);
      }
    });
    await scenario("「关于」弹层：版本号读得到、图标加载了、仓库链接在", async () => {      await s.clickText(".rail button", "关于");
      await sleep(700);
      const info = await s.evalJs(`(() => {        const card = document.querySelector(".modal-card");
        if (!card) return null;
        const img = card.querySelector(".about-icon");        return {          text: card.innerText,          icon: img            ? { src: img.getAttribute("src"), loaded: img.complete && img.naturalWidth > 0, w: img.naturalWidth }            : null,        };      })()`);
      assert(info, "点了「关于」没出现弹层");
      assert(        /版本 \d+\.\d+\.\d+/.test(info.text),        `版本号没读到：${info.text.replace(/\n/g, " | ")}`,      );
      assert(        info.icon?.src === "/icon-128.png" && info.icon.loaded === true && info.icon.w === 128,        `图标没加载：${JSON.stringify(info.icon)}`,      );
      assert(info.text.includes("打开项目主页"), "弹层里没有项目主页按钮");
      assert(info.text.includes("GPL-3.0"), "弹层里没有许可那一行");
      await s.clickText(".modal-card button", "知道了");
      await sleep(400);
      const closed = await s.evalJs(`!document.querySelector(".modal-card")`);
      assert(closed === true, "点了「知道了」弹层没关掉");    });    
// 放行 https 这一侧也得验（会真的开一个浏览器标签，所以只验一条）
    await scenario("open_url 放行 https（报告问题就是靠它打开的）", async () => {      const res = await s.invoke("open_url", { url: "https://github.com/Yisun229/rock-sprout" });
      assert(res?.ok === true, `https 被拒了：${JSON.stringify(res?.error)}`);    });    
// 点链接不许把界面导航走。open_url 打桩，免得每次回归都真弹一个浏览器窗口。
    await scenario("点「怎么拿令牌」的链接：窗口地址不变，底部出一条提示", async () => {
      await s.evalJs(`(() => {
        const real = window.__TAURI_INTERNALS__.invoke;
        window.__rs_restore = () => { window.__TAURI_INTERNALS__.invoke = real; };
        window.__TAURI_INTERNALS__.invoke = (cmd, args) =>
          cmd === "open_url" ? Promise.resolve({ ok: true, data: "ok" }) : real(cmd, args);
      })()`);
      const before = await s.evalJs("location.href");
      await s.evalJs(`(() => {
        const btn = document.querySelector('.steps .link-btn');
        btn.scrollIntoView({ block: 'center' });
        btn.click();
      })()`);
      await sleep(400);
      const after = await s.evalJs("location.href");
      const toast = await s.evalJs(`document.querySelector('[data-block="toast"]')?.textContent ?? null`);
      assert(after === before, `点完窗口地址变了：${before} → ${after}`);
      assert(toast && toast.length > 0, "点完底部没出提示");
    });
    await scenario("底部那条提示 2 秒后自己消失", async () => {
      await sleep(2200);
      const toast = await s.evalJs(`document.querySelector('[data-block="toast"]')?.textContent ?? null`);
      assert(toast === null, `2.3 秒后提示还在：${toast}`);
      await s.evalJs(`window.__rs_restore?.()`);
    });
    await scenario("UE4SS 卡里的链接也是按钮，卡内没有 <a href>", async () => {
      const info = await s.evalJs(`(() => {
        const card = [...document.querySelectorAll('.card')].find((c) => c.textContent.includes('UE4SS（脚本 mod 的前置）'));
        if (!card) return null;
        return {
          buttons: card.querySelectorAll('.link-btn[data-url^="http"]').length,
          anchors: card.querySelectorAll('a[href^="http"]').length,
        };
      })()`);
      assert(info, "设置页没有 UE4SS 卡");
      assert(info.buttons === 1, `UE4SS 卡里可点入口应该 1 个，实际 ${info.buttons}`);
      assert(info.anchors === 0, `UE4SS 卡里还有 <a href>：${info.anchors} 个`);
    });
// WebView2 自检的间接证据：注册表里有时，日志第二行会写「就绪」
    await scenario("WebView2 自检：注册表里有，日志里写「就绪」", async () => {      const logDir = join(reportDir, "logs");
      assert(existsSync(logDir), "没有日志目录");
      const text = readdirSync(logDir)        .map((f) => readFileSync(join(logDir, f), "utf8"))        .join("\n");
      assert(/WebView2 就绪 version=/.test(text), "日志里没有「WebView2 就绪 version=」这一行");    });
    s.close();
    killApp();
    await sleep(600);  }  
// 便携包：解压就能跑，数据落在 exe 旁边的 data\（需要先跑过 _tools/make-portable.sh）
{
  const relDir = join(TOOLS, "release-renamed");
    const zip = existsSync(relDir)      ? readdirSync(relDir)          .filter((f) => /^RockSprout_.+_portable_win64\.zip$/.test(f))          .sort()          .pop()      : null;
    if (!zip) {      console.log("  跳过便携包场景：_tools/release-renamed 里还没有 portable zip");    } else {      const dir = join(base, "portable");
      mkdirSync(dir, { recursive: true });
      const ps = spawnSync(        "powershell",        [          "-NoProfile",          "-Command",          `Expand-Archive -LiteralPath '${join(relDir, zip)}' -DestinationPath '${dir}' -Force`,        ],        { encoding: "utf8" },      );
      assert(ps.status === 0, `解压便携包失败：${ps.stderr || ps.stdout}`);
      const exe = join(dir, "RockSprout.exe");
      assert(existsSync(exe), "解压后没有 RockSprout.exe");
      assert(existsSync(join(dir, "使用说明.txt")), "解压后没有使用说明.txt");      
// 故意不传 ROCK_SPROUT_DATA：默认行为就该是「数据落在 exe 旁边」
      const app = await startApp({}, { exe, cwd: dir });
      const s = await connect(app.wsUrl);
      await waitReady(s);
      phase("便携包（v0.1.3）");
      await scenario("便携包解压就能跑，数据落在 exe 旁边的 data\\", async () => {        const loc = await s.invoke("data_location");
        assert(loc?.ok === true, `data_location 失败：${JSON.stringify(loc?.error)}`);
        assert(loc.data.mode === "portable", `不是便携模式：${loc.data.mode}`);
        assert(          loc.data.path.toLowerCase().startsWith(dir.toLowerCase()),          `数据目录不在解压位置：${loc.data.path}`,        );
        assert(existsSync(join(dir, "data", "logs")), "data\\logs 没建出来");      });
      s.close();
      killApp();
      await sleep(600);    }  }
  rmSync(base, { recursive: true, force: true });}
// ---------- 第二组：需要网（--online 才跑） ----------
if (ONLINE) {
if (!existsSync(TOKEN)) {    phase("在线（跳过：没有 _tools/modio-token.txt）");  } else {    const onlineDir = freshDataDir("online");    copyFileSync(TOKEN, join(onlineDir, "modio-token.txt"));
    const app = await startApp({ ROCK_SPROUT_DATA: onlineDir });
    const s = await connect(app.wsUrl);
    phase("在线（mod.io 必须可达）");
    await scenario("搜索真接口能拿到结果并落缓存", async () => {      const res = await s.invoke("search_mods", { query: "brighter", page: 1 });
      assert(res?.ok === true, `搜索失败：${JSON.stringify(res?.error)}`);
      assert(res.data.items.length > 0, "一条都没搜到");
      const cacheDir = join(onlineDir, "cache/search");
      assert(existsSync(cacheDir) && readdirSync(cacheDir).length > 0, "缓存目录里没东西");    });
    let installedId = 0;
    await scenario("装一个真 mod（下载 + 合并）", async () => {      const search = await s.invoke("search_mods", { query: "brighter", page: 1 });
      const mod = search.data.items[0];      installedId = mod.id;
      const res = await s.invoke("install_mod", { id: mod.id });
      assert(res?.ok === true, `安装失败：${JSON.stringify(res?.error)}`);
      assert(res.data.pak?.endsWith("mods_P.pak"), "没写出合并包");
      assert(existsSync(res.data.pak), `合并包不在磁盘上：${res.data.pak}`);    });
    await scenario("卸载真 mod 后合并包消失", async () => {      const res = await s.invoke("uninstall_mod", { id: installedId });
      assert(res?.ok === true, `卸载失败：${JSON.stringify(res?.error)}`);
      assert(res.data.pak === null, `应该清干净，实际还写着 ${res.data.pak}`);    });
    await scenario("翻译：第一次发请求、第二次直接命中缓存", async () => {
      const first = await s.invoke("translate_text", {
        text: "Hello miners, the drill is ready.",
        target: "",
      });
      assert(first?.ok === true, "翻译失败：" + JSON.stringify(first?.error));
      assert(String(first.data).length > 0, "翻出来是空的");
      const detail1 = await s.invoke("translate_detail", { modId: 1179441, target: "" });
      assert(detail1?.ok === true, "详情翻译失败：" + JSON.stringify(detail1?.error));
      if (detail1.data.cached === true) {
        const again = await s.invoke("translate_detail", { modId: 1179441, target: "" });
        assert(again?.ok === true && again.data.cached === true, "命中缓存后第二次仍不是缓存");
        return;
      }
      const detail2 = await s.invoke("translate_detail", { modId: 1179441, target: "" });
      assert(detail2?.ok === true, "第二次翻译失败：" + JSON.stringify(detail2?.error));
      assert(detail2.data.cached === true, "第二次又去发了请求，缓存没生效");
      assert(detail2.data.text === detail1.data.text, "两次译文不一样");
    });
    s.close();
    killApp();
    await sleep(600);    
// 同一个数据目录，换成断网再起一次：应该能回放上次的搜索结果
  const app2 = await startApp({      ROCK_SPROUT_DATA: onlineDir,      HTTPS_PROXY: "http://127.0.0.1:9",      ALL_PROXY: "http://127.0.0.1:9",    });
    const s2 = await connect(app2.wsUrl);
    const listRes = await s.invoke("list_mods", {
      keyword: null,
      tags: [],
      version: null,
      sort: "downloads",
      page: 1,
    });
    const qolRes = await s.invoke("list_mods", {
      keyword: null,
      tags: ["QoL"],
      version: null,
      sort: "downloads",
      page: 1,
    });
    const twoRes = await s.invoke("list_mods", {
      keyword: null,
      tags: ["QoL", "Framework"],
      version: null,
      sort: "downloads",
      page: 1,
    });
    const latestRes = await s.invoke("list_mods", {
      keyword: null,
      tags: [],
      version: null,
      sort: "latest",
      page: 1,
    });
    const ids = (r) => (r?.ok ? r.data.items.map((m) => m.id) : []);

    await scenario("筛选结果确实变了：勾上标签换一批 id，排序换了顺序，下载量单调", async () => {
      assert(listRes?.ok && qolRes?.ok && twoRes?.ok && latestRes?.ok, "有一条接口调用失败");
      const base = ids(listRes);
      const qol = ids(qolRes);
      const two = ids(twoRes);
      assert(base.length > 0 && qol.length > 0, "拿到的是空列表");
      assert(
        JSON.stringify(base) !== JSON.stringify(qol),
        "带标签和不带标签返回的 id 列表一模一样——说明筛选参数没生效",
      );
      assert(
        qolRes.data.total < listRes.data.total,
        "带标签的条数（" + qolRes.data.total + "）不该比不带标签（" + listRes.data.total + "）多",
      );
      assert(
        twoRes.data.total < qolRes.data.total,
        "勾两个（" + twoRes.data.total + "）应该比勾一个（" + qolRes.data.total + "）更少——多个类型是「同时满足」",
      );
      assert(two.length > 0, "勾两个类型后一条都没有，没法继续断言");
      assert(
        JSON.stringify(two) !== JSON.stringify(qol),
        "勾两个和勾一个返回的完全一样",
      );
      assert(
        JSON.stringify(ids(latestRes)) !== JSON.stringify(base),
        "换成按最新发布排序，顺序没变——排序参数没生效",
      );
      const dl = listRes.data.items.map((m) => m.downloads);
      for (let k = 1; k < dl.length; k++) {
        assert(dl[k - 1] >= dl[k], "按下载量排序后第 " + k + " 条比下一条还小");
      }
    });
    await scenario("断网时回放上次的搜索结果（标成过期）", async () => {      const res = await s2.invoke("search_mods", { query: "brighter", page: 1 });
      assert(res?.ok === true, `没回放出来：${JSON.stringify(res?.error)}`);
      assert(res.data.stale === true, "没标成过期数据");
      assert(res.data.items.length > 0, "回放出来是空的");    });    s2.close();
    killApp();
    await sleep(600);  }}
// ---------- 今天页（v0.1.5）----------
// 这一组自己起窗口：有的场景要重启一次才能验证「重启后还在」。
const DEAD_PROXY = { HTTPS_PROXY: "http://127.0.0.1:9", ALL_PROXY: "http://127.0.0.1:9" };
/** 写一份「今天」页的缓存，模拟上次取到的东西。 */
function writeTodayCache(dir, { fetchedAt, title }) {
  mkdirSync(join(dir, "cache"), { recursive: true });
  const digest = {
    fetched_at: fetchedAt,
    from_cache: false,
    news: [{ gid: "probe-1", title, url: "https://example.com/news/1", date: 1_790_000_000, contents: "一段抓下来的正文。" }],
    mod_changes: [],
    updates: [],
    installed_count: 0,
    checked_count: 0,
    token_missing: true,
    degraded: [],
  };
  writeFileSync(join(dir, "cache", "today.json"), JSON.stringify(digest, null, 2));
}
/** 等到「今天」页把骨架换成正式内容（断网时也会很快退化成一行说明）。 */
async function waitTodaySettled(s) {
  for (let i = 0; i < 100; i++) {
    const loading = await s.evalJs("!!document.querySelector('[data-page=\"today\"] .skeleton')");
    const head = await s.evalJs("!!document.querySelector('[data-page=\"today\"] .today-head')");
    if (head && !loading) return;
    await sleep(150);
  }
}
/** 写一份假的 mod 详情缓存：断网时后端会拿它兜底（见 ipc/mods.rs 的 detail_with_cache）。 */
function writeDetailCache(dir, id, patch = {}) {
  mkdirSync(join(dir, "cache", "detail"), { recursive: true });
  const value = {
    id,
    name: "缓存测试 mod 1",
    summary: "",
    description: "",
    description_plaintext: "",
    downloads: 50000,
    version: "1.0",
    logo_320: "",
    logo_640: "",
    creator: "",
    date_updated: 0,
    tags: [],
    stale: false,
    cached_at: null,
    ...patch,
  };
  writeFileSync(
    join(dir, "cache", "detail", `${id}.json`),
    JSON.stringify({ cached_at: Date.now(), value }, null, 2),
  );
}

const todayText = (s) => s.evalJs(`document.querySelector('[data-page="today"]')?.innerText ?? ""`);

killApp();
await sleep(400);
phase("今天页：断网、没缓存、没令牌");
{
  const dir = freshDataDir("today-notoken");
  const app = await startApp({ ROCK_SPROUT_DATA: dir, ...DEAD_PROXY });
  const s = await connect(app.wsUrl);
  await waitReady(s);
  await waitTodaySettled(s);

  await scenario("断网又没缓存：页头写「当前离线」，三块各给一行说明，不弹框", async () => {
    const body = await todayText(s);
    assert(body.includes("当前离线"), `页头没写「当前离线」：${body.slice(0, 200)}`);
    assert(body.includes("这块没取到。"), "三块没写「这块没取到。」");
    const noName = await s.evalJs(`document.body.innerText.includes("官方公告这块没取到")`);
    assert(noName === false, "失败说明里又重复了一遍块名");
    const dialogs = await s.evalJs(`document.querySelectorAll('.modal, [role="dialog"]').length`);
    assert(dialogs === 0, "弹了窗口，应该只在页面上写一行");
  });

  await scenario("没令牌时「更新」只是页尾一行，不占整张卡排在最上面", async () => {
    const tag = await s.evalJs(`document.querySelector('[data-block="updates"]')?.tagName ?? null`);
    assert(tag === "P", `「更新」应该是一行（p），实际是 ${tag}`);
    const isCard = await s.evalJs(`!!document.querySelector('section.card[data-block="updates"]')`);
    assert(isCard === false, "没更新时还留着一张卡");
    const body = await todayText(s);
    assert(body.includes("粘个令牌，才能看出你装的 mod 有没有新版"), `页尾那行写得不对：${body.slice(-120)}`);
  });

  await scenario("没令牌时「新冒出来的 mod」是引导卡，能点去设置", async () => {
    const body = await todayText(s);
    assert(body.includes("粘个令牌，才能看到 mod 动态"), "引导卡没出现");
    const btns = await s.evalJs(
      `[...document.querySelectorAll('[data-page="today"] button')].filter((b) => b.getClientRects().length > 0).map((b) => b.innerText.trim())`,
    );
    assert(btns.includes("去设置"), `没找到「去设置」按钮，现有：${JSON.stringify(btns)}`);
    assert(btns.includes("先不看"), "没找到「先不看」按钮");
  });

  await s.evalJs(`(() => {
    const b = [...document.querySelectorAll('[data-page="today"] button')].find((e) => e.innerText.trim() === "先不看");
    if (b) b.click();
    return !!b;
  })()`);
  // 等它真把状态写盘再关（写回是 400ms 防抖，机器忙时 900ms 不一定够，别跟它赛跑）
  const uiFile = join(dir, "ui-state.json");
  for (let i = 0; i < 30; i++) {
    if (existsSync(uiFile) && readFileSync(uiFile, "utf8").includes('"token_hint_collapsed":true')) break;
    await sleep(100);
  }
  s.close();
  killApp();
  await sleep(700);

  const again = await startApp({ ROCK_SPROUT_DATA: dir, ...DEAD_PROXY });
  const s2 = await connect(again.wsUrl);
  await waitReady(s2);
  await waitTodaySettled(s2);
  await scenario("点过「先不看」之后重启：引导卡还是一行小字", async () => {
    const body = await todayText(s2);
    assert(!body.includes("粘个令牌，才能看到 mod 动态"), "重启后整张引导卡又回来了");
    assert(body.includes("粘个令牌") && body.includes("怎么弄"), `收起后的那一行不对：${body.slice(0, 200)}`);
  });
  s2.close();
  killApp();
  await sleep(700);
}

phase("今天页：缓存、离线、重启记忆");
{
  // 有一天前的旧缓存：不该拿来糊弄人（Q6）
  const staleDir = freshDataDir("today-stale");
  writeTodayCache(staleDir, { fetchedAt: Date.now() - 2 * 24 * 3600 * 1000, title: "前天存的旧公告" });
  const app = await startApp({ ROCK_SPROUT_DATA: staleDir, ...DEAD_PROXY });
  const s = await connect(app.wsUrl);
  await waitReady(s);
  await waitTodaySettled(s);
  await scenario("缓存是前天的：不拿来当今天的用，会重新去取（Q6）", async () => {
    const body = await todayText(s);
    assert(!body.includes("前天存的旧公告"), "把前天的缓存当成今天的显示了");
    assert(!body.includes("今天早些时候存的"), "前天的数据不该说成「今天早些时候」");
    assert(body.includes("当前离线"), `重取没成时该说清是断网：${body.slice(0, 160)}`);
  });
  s.close();
  killApp();
  await sleep(700);

  // 今天已经取过一次：断网时显示存下的内容，并说明来源
  const cacheDir = freshDataDir("today-cache");
  writeTodayCache(cacheDir, { fetchedAt: Date.now() - 5 * 60 * 1000, title: "今天早些时候真取到的公告" });
  const app2 = await startApp({ ROCK_SPROUT_DATA: cacheDir, ...DEAD_PROXY });
  const s2 = await connect(app2.wsUrl);
  await waitReady(s2);
  await waitTodaySettled(s2);
  await scenario("断网但今天取过一次：显示存下的内容，并写明是什么时候取到的", async () => {
    const body = await todayText(s2);
    assert(body.includes("今天早些时候真取到的公告"), `没显示存下的内容：${body.slice(0, 200)}`);
    assert(body.includes("今天早些时候存的"), "没说明这批数据是早些时候取到的");
    assert(!body.includes("当前离线"), "有内容可显示，不该再说「什么都没取到」");
  });
  s2.close();
  killApp();
  await sleep(700);

  // 停在矿脉再重启：应该还落在矿脉（Q1）
  const memDir = freshDataDir("today-memory");
  const app3 = await startApp({ ROCK_SPROUT_DATA: memDir, ...DEAD_PROXY });
  const s3 = await connect(app3.wsUrl);
  await waitReady(s3);
  await gotoMine(s3);
  await sleep(900);
  s3.close();
  killApp();
  await sleep(700);
  const app4 = await startApp({ ROCK_SPROUT_DATA: memDir, ...DEAD_PROXY });
  const s4 = await connect(app4.wsUrl);
  await waitReady(s4);
  await sleep(800);
  await scenario("上次停在矿脉，重启还落在矿脉（Q1）", async () => {
    const visible = await s4.evalJs(
      `[...document.querySelectorAll('[data-page]')].filter((e) => e.getClientRects().length > 0).map((e) => e.dataset.page).join(",")`,
    );
    assert(visible.includes("mine"), `重启后显示的是「${visible}」，不是矿脉`);
  });
  s4.close();
  killApp();
  await sleep(700);
}

phase("今天页：一块坏了不影响另外两块");
{
  // 网络通、令牌是假的：公告照常，mod 两块各一行说明（至少要能连上 Steam）
  // 用 curl 探路：这台机器上 Node 自带的 fetch 不认系统的证书链，会报 UNABLE_TO_GET_ISSUER_CERT_LOCALLY，
  // 而应用里的请求（reqwest）是通的，所以不能用 Node 的探测结果去关掉这条场景。
  let netUp = false;
  try {
    const probe = spawnSync(
      "curl.exe",
      ["-s", "-o", "NUL", "-w", "%{http_code}", "--noproxy", "*", "-m", "8", "https://store.steampowered.com/feeds/news/app/548430/"],
      { encoding: "utf8" },
    );
    netUp = (probe.stdout ?? "").trim() === "200";
  } catch {
    netUp = false;
  }
  if (!netUp) {
    console.log("  跳过「一块坏了不影响另外两块」：这台机器连不上 Steam feed");
  } else {
    const dir = freshDataDir("today-badtoken");
    writeFileSync(join(dir, "modio-token.txt"), "0000000000000000000000000000dead\n");
    const app = await startApp({ ROCK_SPROUT_DATA: dir });
    const s = await connect(app.wsUrl);
    await waitReady(s);
    await waitTodaySettled(s);
    await scenario("令牌失效时：公告照常显示，两块 mod 各写一行「这块没取到」", async () => {
      const body = await todayText(s);
      const newsCard = await s.evalJs(
        `document.querySelector('[data-block="news"]')?.innerText ?? ""`,
      );
      assert(newsCard.includes("公告") && !newsCard.includes("这块没取到。"), `公告块被带坏了：${newsCard.slice(0, 120)}`);
      const failed = await s.evalJs(
        `[...document.querySelectorAll('[data-page="today"] .card')].filter((c) => c.innerText.includes("这块没取到。")).length`,
      );
      assert(failed >= 1, "两块 mod 一块都没标失败");
      const body2 = await todayText(s);
      assert(body2.includes("官方公告"), "页面结构没了");
    });

    await scenario("公告项是按钮不是链接，点它走同一条白名单", async () => {
      const info = await s.evalJs(`(() => {
        const items = [...document.querySelectorAll('[data-page="today"] .news-item')];
        return {
          count: items.length,
          tags: items.map((li) => li.querySelector('button, a')?.tagName ?? null),
          urls: items.map((li) => li.querySelector('button, a')?.getAttribute('href') ?? null),
        };
      })()`);
      assert(info.count > 0, "一条公告都没有");
      assert(info.tags.every((t) => t === "BUTTON"), `公告项应该是 button，实际：${JSON.stringify(info.tags)}`);
      assert(info.urls.every((u) => u === null), `公告项不该自带 href：${JSON.stringify(info.urls)}`);
      const bad = await s.invoke("open_url", { url: "javascript:alert(1)" });
      assert(bad?.ok === false, "open_url 放过了 javascript:");
    });
    s.close();
    killApp();
    await sleep(600);
  }
}



// ---------- 矿脉：抽屉与筛选（M6 新增）----------
// 断网 + 假缓存 + 真令牌：列表能出来，详情与翻译必然失败——正好验「失败怎么说人话」。
killApp();
await sleep(400);
phase("矿脉：抽屉与筛选（M6 新增）");
{
  const dir = freshDataDir("m6-mine");
  copyFileSync(TOKEN, join(dir, "modio-token.txt"));
  writeSearchCache(dir, "性能");
  writeDetailCache(dir, 800000);
  const app = await startApp({ ROCK_SPROUT_DATA: dir, ...DEAD_PROXY });
  const s = await connect(app.wsUrl);
  await waitReady(s);
  const gotoMine = async () => {
    await s.evalJs(
      `(() => { const b = [...document.querySelectorAll('.nav-item')].find((x) => x.textContent.includes('矿脉')); b?.click(); return !!b; })()`,
    );
    await sleep(300);
  };
  const search = async () => {
    await s.evalJs(`(() => {
      const input = document.querySelector('.searchbar input');
      if (input) {
        const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
        setter.call(input, '性能');
        input.dispatchEvent(new Event('input', { bubbles: true }));
      }
      const b = document.querySelector('.searchbar .btn');
      b?.click();
      return !!b;
    })()`);
    for (let i = 0; i < 60; i++) {
      if ((await s.evalJs("document.querySelectorAll('.vrow .row').length")) > 0) return;
      await sleep(200);
    }
    throw new Error("搜了半天没出结果");
  };
  const rowCount = () => s.evalJs("document.querySelectorAll('.vrow .row').length");
  const hasPreview = () => s.evalJs("!!document.querySelector('.mod-preview')");
  const previewText = () => s.evalJs("document.querySelector('.mod-preview')?.innerText ?? ''");
  /** 等抽屉从骨架变成「有内容」或「说没取到」，最多 8 秒。 */
  const waitPreviewSettled = async () => {
    for (let i = 0; i < 40; i++) {
      const st = await s.evalJs(`(() => {
        const box = document.querySelector('.mod-preview');
        if (!box) return 'gone';
        if (box.querySelector('.preview-name')) return 'name';
        if (box.innerText.includes('没取到')) return 'failed';
        return 'pending';
      })()`);
      if (st === "name" || st === "failed") return st;
      await sleep(200);
    }
    return "timeout";
  };
  const pressEsc = async () => {
    await s.send("Input.dispatchKeyEvent", {
      type: "rawKeyDown",
      windowsVirtualKeyCode: 27,
      nativeVirtualKeyCode: 27,
      key: "Escape",
      code: "Escape",
    });
    await s.send("Input.dispatchKeyEvent", {
      type: "keyUp",
      windowsVirtualKeyCode: 27,
      nativeVirtualKeyCode: 27,
      key: "Escape",
      code: "Escape",
    });
    await sleep(300);
  };
  await gotoMine();
  await search();

  await scenario("点一行打开抽屉：先出骨架，再落定（断网时落成「没取到」）", async () => {
    assert((await rowCount()) > 0, "一条结果都没有");
    await s.evalJs("document.querySelector('.vrow .row')?.click()");
    await sleep(120);
    assert(await hasPreview(), "点了行没出抽屉");
    const st = await waitPreviewSettled();
    assert(st !== "timeout" && st !== "gone", "抽屉一直停在骨架上");
    const body = await previewText();
    assert(body.length > 2, `抽屉是空的：${JSON.stringify(body)}`);
  });

  await scenario("再点同一行：抽屉收起来", async () => {
    await s.evalJs("document.querySelector('.vrow .row')?.click()");
    await sleep(300);
    assert(!(await hasPreview()), "再点同一行没关掉");
  });

  await scenario("点开以后按 Esc：抽屉收起来", async () => {
    await s.evalJs("document.querySelector('.vrow .row')?.click()");
    await sleep(300);
    assert(await hasPreview(), "没打开");
    await pressEsc();
    assert(!(await hasPreview()), "按 Esc 没关掉");
  });

  await scenario("勾选框只勾不打开抽屉", async () => {
    await s.evalJs("document.querySelector('.vrow .row .pick')?.click()");
    await sleep(300);
    const picked = await s.evalJs(
      "document.querySelector('.vrow .row .pick')?.checked === true",
    );
    assert(picked, "勾选框没勾上");
    assert(!(await hasPreview()), "勾选把抽屉打开了");
    await s.evalJs("document.querySelector('.vrow .row .pick')?.click()");
    await sleep(200);
  });

  await scenario("断网时抽屉说「详情没取到」，并给重试", async () => {
    // 第一行的详情在缓存里有（上面那条空简介场景要用），这里挑没有缓存的一行
    await s.evalJs(
      "document.querySelectorAll('.vrow .row .row-main')[1]?.click()",
    );
    assert(await hasPreview(), "没打开抽屉");
    const st = await waitPreviewSettled();
    assert(st === "failed", `断网时抽屉状态是「${st}」，不是失败提示`);
    const retry = await s.evalJs(
      "[...document.querySelectorAll('.mod-preview button')].some((b) => b.innerText.trim() === '重试')",
    );
    assert(retry, "没给重试按钮");
    await pressEsc();
  });


  await scenario("简介为空的 mod：写「作者还没有写简介」，不给翻译按钮", async () => {
    if (await hasPreview()) await pressEsc();
    await s.evalJs("document.querySelector('.vrow .row')?.click()");
    assert(await hasPreview(), "没打开抽屉");
    const st = await waitPreviewSettled();
    assert(st === "name", `抽屉没落到有内容的状态（${st}）`);
    const info = await s.evalJs(`(() => {
      const box = document.querySelector('.mod-preview');
      return {
        text: box.innerText,
        translateButtons: [...box.querySelectorAll('button')].filter((b) => b.innerText.trim() === '翻译').length,
        tagRows: box.querySelectorAll('.preview-tags').length,
      };
    })()`);
    assert(info.text.includes("作者还没有写简介"), `面板里没写那句：${info.text.slice(0, 80)}`);
    assert(info.translateButtons === 0, "没有简介却给了翻译按钮");
    assert(info.tagRows === 0, "没有标签却留了一块空的标签区");
    await pressEsc();
  });

  await scenario("断网时「译」按钮变灰，鼠标停上去能看到原因", async () => {
    assert((await rowCount()) > 0, "列表空了，没法验翻译失败");
    await s.evalJs(
      `(() => { const b = document.querySelector('.vrow .row .btn.translate'); b?.click(); return !!b; })()`,
    );
    let info = null;
    for (let i = 0; i < 120; i++) {
      info = await s.evalJs(`(() => {
        const b = document.querySelector('.vrow .row .btn.translate');
        return b ? { failed: b.classList.contains('failed'), title: b.title } : null;
      })()`);
      if (info?.failed) break;
      await sleep(250);
    }
    assert(info, "行里没有「译」按钮");
    assert(info.failed, "翻译失败后按钮没变灰");
    assert((info.title ?? "").length > 0, "悬停没有说明原因");
    assert((await rowCount()) > 0, "翻译失败把列表也带没了");
  });

  await scenario("类型下拉里就是那 9 个真实标签，底部写着「勾选多个 = 同时满足」", async () => {
    await s.evalJs(
      `(() => { const b = [...document.querySelectorAll('.filters .btn')].find((x) => x.innerText.includes('类型')); b?.click(); return !!b; })()`,
    );
    await sleep(300);
    const info = await s.evalJs(`(() => {
      const panel = document.querySelector('.filter-panel');
      if (!panel) return null;
      return {
        items: [...panel.querySelectorAll('.filter-item')].map((e) => e.innerText.trim()),
        foot: panel.querySelector('.filter-foot')?.innerText ?? '',
      };
    })()`);
    assert(info, "类型面板没打开");
    const expect = ["体验优化", "前置框架", "视觉", "外观", "玩法", "音效", "工具", "动画", "官方认证"];
    for (const name of expect) {
      assert(info.items.includes(name), `少了「${name}」，实际：${info.items.join("/")}`);
    }
    assert(info.items.length === expect.length, `多出别的项了：${info.items.join("/")}`);
    assert(info.foot.includes("勾选多个 = 同时满足"), `底部没写那句：${info.foot}`);
  });

  await scenario("勾两个类型：按钮写成「已选 2 个」", async () => {
    await s.evalJs(`(() => {
      const items = [...document.querySelectorAll('.filter-panel .filter-item')];
      items[0]?.click();
      items[1]?.click();
      return true;
    })()`);
    await sleep(800);
    const label = await s.evalJs(
      `[...document.querySelectorAll('.filters .btn')].find((x) => x.innerText.includes('类型'))?.innerText ?? ''`,
    );
    assert(label.includes("已选 2 个"), `按钮上写的是「${label}」`);
  });

  s.close();
  killApp();
  await sleep(600);
}

// ---- 纯原版往返（默认不跑：它会挪动用户机器上别的启动器装的东西）----
// 用法：node _tools/check-all.mjs --with-stock
// 为什么单独做：§0 第 39 条说新用户的起点是「纯原版」——没有 dwmapi.dll、没有 ue4ss\。
// 这个场景要把现有的 dwmapi.dll 和 ue4ss\ 先移出去，装一遍、卸一遍，确认目录回到纯原版，
// 再把人家那套原样放回并逐字节核对。默认不跑是因为它会动用户已经装好的东西。
const WITH_STOCK = process.argv.includes("--with-stock");

function walkTree(dir, base = "") {
  const out = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const rel = base ? `${base}/${entry.name}` : entry.name;
    const full = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...walkTree(full, rel));
    else out.push([rel, statSync(full).size, createHash("sha256").update(readFileSync(full)).digest("hex")]);
  }
  return out;
}

function win64Tree() {
  const win64 = join(GAME, "Binaries/Win64");
  return new Map(walkTree(win64).map(([rel, size, hash]) => [rel, `${size}:${hash}`]));
}

function treeDiff(a, b) {
  const diffs = [];
  for (const [k, v] of a) if (b.get(k) !== v) diffs.push(`变了或没了：${k}`);
  for (const k of b.keys()) if (!a.has(k)) diffs.push(`多出来：${k}`);
  return diffs;
}

/** 等应用起来、调一段命令、再关掉。 */
async function withGameApp(dataDir, fn) {
  const app = await startApp({ ROCK_SPROUT_DATA: dataDir });
  const s = await connect(app.wsUrl);
  await waitReady(s);
  try {
    return await fn(s);
  } finally {
    s.close();
    killApp();
    await sleep(600);
  }
}

if (WITH_STOCK) {
  await scenario("纯原版起点：装一遍、卸一遍，目录回到原样", async () => {
    const win64 = join(GAME, "Binaries/Win64");
    const keep = join(TESTDATA, "_stock-gate");
    const moved = join(keep, "moved");
    rmSync(moved, { recursive: true, force: true });
    mkdirSync(moved, { recursive: true });

    const before = win64Tree();
    let restored = false;
    try {
      // ① 把别人的那套挪出去（跨盘不能用 rename，先复制验大小再删）
      for (const rel of ["dwmapi.dll", "ue4ss"]) {
        const src = join(win64, rel);
        if (!existsSync(src)) continue;
        const dst = join(moved, rel);
        cpSync(src, dst, { recursive: true });
        const sizeOf = (p) => (statSync(p).isDirectory() ? walkTree(p).reduce((n, f) => n + f[1], 0) : statSync(p).size);
        assert(sizeOf(src) === sizeOf(dst), `留档后大小对不上：${rel}`);
        rmSync(src, { recursive: true, force: true });
      }
      const pure = win64Tree();

      // ② 备好数据目录与安装包，装进去
      const data = freshDataDir("stock");
      mkdirSync(join(data, "downloads"), { recursive: true });
      copyFileSync(join(TOOLS, "ue4ss-test/UE4SS_v3.0.1.zip"), join(data, "downloads", "UE4SS_v3.0.1.zip"));

      await withGameApp(data, async (s) => {
        const start = await s.invoke("ue4ss_game_status");
        assert(start.data.kind === "none", `纯原版起点应该是 none，实际是 ${start.data.kind}`);

        const prep = await s.invoke("ue4ss_prepare");
        assert(prep.ok, `准备安装包失败：${prep.error?.message ?? "未知"}`);

        const ins = await s.invoke("ue4ss_install", { confirmReplace: false });
        assert(ins.ok, `装失败：${ins.error?.message ?? "未知"}`);
        for (const rel of ["UE4SS.dll", "dwmapi.dll", "UE4SS-settings.ini", "Mods"]) {
          assert(existsSync(join(win64, rel)), `装完应该有 ${rel}`);
        }
        const mid = await s.invoke("ue4ss_game_status");
        assert(mid.data.kind === "ours", `装完状态应该是 ours，实际是 ${mid.data.kind}`);

        const un = await s.invoke("ue4ss_uninstall");
        assert(un.ok, `卸失败：${un.error?.message ?? "未知"}`);
        for (const rel of ["UE4SS.dll", "dwmapi.dll", "UE4SS-settings.ini", "Mods", "ue4ss", "UE4SS.log"]) {
          assert(!existsSync(join(win64, rel)), `卸完不该有 ${rel}`);
        }
        const end = await s.invoke("ue4ss_game_status");
        assert(end.data.kind === "none", `卸完状态应该回到 none，实际是 ${end.data.kind}`);
      });

      // ③ 目录必须回到纯原版
      const diffs = treeDiff(pure, win64Tree());
      assert(diffs.length === 0, `卸完和纯原版不一致：${diffs.slice(0, 3).join("；")}`);
    } finally {
      // ④ 不管成没成，都把人家的那套放回去
      for (const rel of ["dwmapi.dll", "ue4ss"]) {
        const src = join(moved, rel);
        if (!existsSync(src)) continue;
        const dst = join(win64, rel);
        rmSync(dst, { recursive: true, force: true });
        cpSync(src, dst, { recursive: true });
      }
      restored = true;
      const back = treeDiff(before, win64Tree());
      assert(restored && back.length === 0, `放回后和移出前不一致：${back.slice(0, 3).join("；")}`);
    }
  });
}
// ---- 没装游戏时导入（y 级补丁 v0.1.8.1 的防复发场景）----
// 干净机上暴露过的两个毛病：① 错误块把「找不到游戏」说成「游戏正开着」；
// ② 导入其实成功了，页面却同时弹红块 + 说「还没有装任何 mod」。
// 做法：把 Steam 的 appmanifest 临时藏起来，让 mint_lib 找不到游戏；跑完一定放回去。
await scenario("没装游戏时导入：行还在、不弹红块、不说空、也不提「游戏正开着」", async () => {
  const acf = "D:/Steam/steamapps/appmanifest_548430.acf";
  const hidden = acf + ".rs-test-hidden";
  const hadAcf = existsSync(acf);
  assert(hadAcf, "这台机器上没有 appmanifest_548430.acf，没法造「没装游戏」");

  const data = freshDataDir("nogame");
  renameSync(acf, hidden);
  try {
    const app = await startApp({ ROCK_SPROUT_DATA: data });
    const s = await connect(app.wsUrl);
    await waitReady(s);
    try {
      const res = await s.invoke("import_local", {
        path: join(TOOLS, "fixtures/small-mod.zip"),
        replace: false,
        dirName: null,
      });
      assert(res.ok, `导入本身不该失败：${res.error?.message ?? "未知"}`);
      assert(res.data.installed === false, "没装游戏时不该说自己装进游戏了");
      assert(res.data.reason === "no_game", `reason 应该是 no_game，实际是 ${res.data.reason}`);

      // 切到背包页，让它读一次记账
      await s.evalJs(`(() => {
        const nav = [...document.querySelectorAll('.nav-item')].find((b) => b.innerText.trim() === '背包');
        nav?.click();
        return true;
      })()`);
      await sleep(1200);

      const view = await s.evalJs(`(() => {
        const page = document.querySelector('[data-page="bag"]');
        // 「没找到《深岩银河》」这句在页头横幅里，不在背包那一栏里，所以整页一起看
        const whole = document.body.innerText;
        return {
          rows: page ? page.querySelectorAll('.list .row').length : 0,
          redBlock: !!document.querySelector('[data-block="error-state"], .card.fail'),
          empty: !!document.querySelector('[data-page="bag"] .empty'),
          hasNoGame: whole.includes('没找到'),
          saysRunning: whole.includes('游戏正开着') || whole.includes('文件被占用'),
        };
      })()`);

      assert(view.rows >= 1, `背包里应该有这一行，实际 ${view.rows} 行`);
      assert(!view.redBlock, "不该再弹红块（页头已经说过游戏没找到）");
      assert(!view.empty, "记账里明明有一条，不该显示空态");
      assert(view.hasNoGame, "文案里应该写「没找到」");
      assert(!view.saysRunning, "文案里不该出现「游戏正开着 / 文件被占用」");
    } finally {
      s.close();
      killApp();
      await sleep(600);
    }
  } finally {
    if (existsSync(hidden)) renameSync(hidden, acf);
    assert(existsSync(acf), "appmanifest 没能放回去");
  }
});

phase("我的数据：读存档、换档、三种读不通");
{
  const SAVE_A = join(TOOLS, "save-samples/76561199027062409_Player.sav"); // 199 局、21,216 金币
  const SAVE_B = join(TOOLS, "save-samples/76561199218191195_Player.sav"); // 141 局、120,205 金币
  const dir = freshDataDir("mydata");
  const saves = join(dir, "saves");
  mkdirSync(saves, { recursive: true });

  // 两份候选：把 B 的时间拨到最新，默认就该选 B
  copyFileSync(SAVE_A, join(saves, basename(SAVE_A)));
  copyFileSync(SAVE_B, join(saves, basename(SAVE_B)));
  utimesSync(join(saves, basename(SAVE_A)), new Date("2026-11-01T10:00:00"), new Date("2026-11-01T10:00:00"));
  utimesSync(join(saves, basename(SAVE_B)), new Date("2026-11-02T10:00:00"), new Date("2026-11-02T10:00:00"));

  const env = { ROCK_SPROUT_DATA: dir, ROCK_SPROUT_SAVE_DIR: saves, ...DEAD_PROXY };

  /** 等「我的数据」页把数字打出来。 */
  const waitNumbers = async (s) => {
    for (let i = 0; i < 60; i++) {
      const n = await s.evalJs(`document.querySelectorAll('[data-page="data"] .data-num').length`);
      if (n >= 4) return;
      await sleep(200);
    }
    throw new Error("等不到「我的数据」的四个数字");
  };
  const dataText = (s) => s.evalJs(`document.querySelector('[data-page="data"]')?.innerText ?? ""`);

  const open = async () => {
    const app = await startApp(env);
    const s = await connect(app.wsUrl);
    await waitReady(s);
    await s.clickText(".rail button", "我的数据");
    return s;
  };

  let s = await open();

  await scenario("默认选最新那份：四个数字与存档里的值一致", async () => {
    await waitNumbers(s);
    const text = await dataText(s);
    assert(text.includes("120,205"), `金币不对：${text.slice(0, 200)}`);
    assert(text.includes("69 小时 15 分"), "总时长不对");
    assert(text.includes("141"), "局数不对");
    assert(text.includes("5 个槽"), "角色槽不对");
  });

  await scenario("矿物显示名字（不是 #编号）", async () => {
    const text = await dataText(s);
    const named = ["Croppa", "Umanite", "Jadiz", "Bismor", "Magnite"].filter((n) => text.includes(n));
    assert(named.length >= 2, `矿物名字没出来，现有：${text.slice(0, 300)}`);
    assert(!/#[0-9a-f]{8}/.test(text), "表面上有认不出来的编号");
  });

  await scenario("角色块：五个槽都在，有进度的显等级", async () => {
    const text = await dataText(s);
    for (const i of [1, 2, 3, 4, 5]) assert(text.includes(`角色 ${i}`), `缺角色 ${i}：${text.slice(0, 300)}`);
    assert(/等级 \d+\/25/.test(text), "没有等级行（等级 N/25）");
    assert(text.includes("这个槽还没进度"), "空槽没标出来");
  });

  await scenario("里程碑块默认收起，摘要写「已领取 N 条」", async () => {
    const info = await s.evalJs(`(() => {
      const d = document.querySelector('[data-page="data"] details.dwarf-details');
      if (!d) return null;
      return { open: d.open, text: (d.querySelector('summary')?.innerText ?? '') };
    })()`);
    assert(info, "没有里程碑块");
    assert(info.open === false, "里程碑默认应该是收起的");
    assert(/已领取 \d+\/\d+ 条/.test(info.text), `摘要不对：${info.text}`);
  });

  await scenario("展开里程碑：名字填好、没有 {value} 占位符", async () => {
    await s.evalJs(`(() => {
      const d = document.querySelector('[data-page="data"] details.dwarf-details');
      if (d) d.open = true;
      return !!d;
    })()`);
    await sleep(400);
    const rows = await s.evalJs(`document.querySelectorAll('[data-page="data"] details.dwarf-details li').length`);
    assert(rows >= 50, `里程碑条目太少：${rows}`);
    const text = await dataText(s);
    assert(!text.includes("{value}"), "还有 {value} 没填");
    assert(!text.includes("{character}"), "还有 {character} 没填");
  });

  await scenario("里程碑：条数、总数与每行档位都对得上表", async () => {
    // 固定用 062409 这份（本档领过 Scout 的升级里程碑，tier=4），断言才不随「最新存档」漂移
    // pick() 的 selected 是**文件名**，不是路径（传路径会被当成没匹配上而退回最新那份）
    const fixed = (await s.invoke("my_dwarf_view", { selected: "76561199027062409_Player.sav" })).data;
    assert(fixed && fixed.state === "ok", `里程碑视图读不出来：${JSON.stringify(fixed)?.slice(0, 200)}`);
    const rows = fixed.milestones ?? [];
    assert(
      fixed.milestones_total >= rows.length,
      `总数 ${fixed.milestones_total} 不该小于列表条数 ${rows.length}（分母是名字表 57 条）`,
    );
    const claimed = rows.filter((m) => m.tier > 0).length;
    assert(claimed === fixed.milestones_claimed, `已领取写 ${fixed.milestones_claimed}，按 tier 数是 ${claimed}`);
    const unnamed = rows.filter((m) => !m.name);
    assert(unnamed.length === 0, `有 ${unnamed.length} 条查不到名字：${unnamed.map((m) => m.id.slice(0, 8)).join(",")}`);
    // 表里缺级会在这里立刻露馅：旧表把 LevelUp 压成一级时，这一行是「第 4/1 级」
    const over = rows.filter((m) => m.max_tier < Math.max(1, m.tier));
    assert(over.length === 0, `档位超出表里的级数：${over.map((m) => `${m.id.slice(0, 8)} 第 ${m.tier}/${m.max_tier} 级`).join("；")}`);
    const levelups = rows.filter((m) => /levels as the/.test(m.name ?? ""));
    assert(levelups.length === 1, `这份存档里的职业升级里程碑应只有 Scout 一条，实际 ${levelups.length}`);
    assert(levelups[0].max_tier === 4, `职业升级里程碑应有四级，表里是 ${levelups[0].max_tier}`);

    // 界面上的分母用的是同一份数据：条数、已领取、摘要三者必须一致
    const cur = (await s.invoke("my_dwarf_view", {})).data;
    const ui = await s.evalJs(`(() => {
      const d = document.querySelector('[data-page="data"] details.dwarf-details');
      return d ? { text: d.querySelector('summary')?.innerText ?? '', rows: d.querySelectorAll('li').length } : null;
    })()`);
    assert(ui, "界面上的里程碑块不见了");
    assert(ui.rows === cur.milestones.length, `界面上 ${ui.rows} 行，接口给了 ${cur.milestones.length} 行`);
    assert(ui.text.includes(`已领取 ${cur.milestones_claimed}/${cur.milestones_total} 条`), `摘要与接口对不上：${ui.text}`);
  });

  await scenario("换回另一份存档：角色块的等级跟着变", async () => {
    const switched = await s.evalJs(`(() => {
      const sel = document.querySelector('[data-page="data"] .save-pick select');
      if (!sel) return false;
      const opt = [...sel.options].find((o) => /062409|199 局/.test(o.textContent));
      if (!opt) return false;
      sel.value = opt.value;
      sel.dispatchEvent(new Event('change', { bubbles: true }));
      return true;
    })()`);
    assert(switched, "换档下拉没找到目标存档");
    await sleep(1200);
    const text = await dataText(s);
    assert(/等级 25\/25/.test(text), `换档后没看到等级 25/25：${text.slice(0, 300)}`);
  });

  await scenario("换一份存档：数字跟着变", async () => {
    const switched = await s.evalJs(`(() => {
      const sel = document.querySelector('[data-page="data"] .save-pick select');
      if (!sel) return false;
      const opt = [...sel.options].find((o) => /062409|199 局/.test(o.textContent));
      if (!opt) return false;
      sel.value = opt.value;
      sel.dispatchEvent(new Event("change", { bubbles: true }));
      return true;
    })()`);
    assert(switched, "存档下拉里找不到另一份");
    for (let i = 0; i < 40; i++) {
      const text = await dataText(s);
      if (text.includes("21,216")) break;
      await sleep(200);
    }
    const text = await dataText(s);
    assert(text.includes("21,216"), `换档后金币没变：${text.slice(0, 200)}`);
    assert(text.includes("199"), "换档后局数没变");
    assert(!text.includes("120,205"), "换档后还留着上一份的数");
  });

  s.close();
  killApp();
  await sleep(800);

  s = await open();
  await scenario("重启后还记着我选的那份", async () => {
    await waitNumbers(s);
    const text = await dataText(s);
    assert(text.includes("21,216"), `重启后没记住选择：${text.slice(0, 200)}`);
    const saved = JSON.parse(readFileSync(join(dir, "ui-state.json"), "utf8"));
    assert(String(saved.save_file ?? "").includes("062409"), `ui-state 里没记住：${JSON.stringify(saved)}`);
  });

  await scenario("对照表里没有的编号显示成 #编号", async () => {
    // 单独起一份：把这份档里所有认识的资源编号改成不认识的，再让它读
    const dir2 = freshDataDir("mydata-unknown-id");
    const saves2 = join(dir2, "saves");
    mkdirSync(saves2, { recursive: true });
    const target = join(saves2, basename(SAVE_A));
    const buf = readFileSync(SAVE_A);
    const names = JSON.parse(readFileSync(join(ROOT, "app/src/data/names.json"), "utf8"));
    let patched = 0;
    for (const hexId of Object.keys(names.resources)) {
      const bytes = Buffer.from(hexId, "hex");
      let at = buf.indexOf(bytes);
      while (at >= 0) {
        buf[at] ^= 0xff; // 改完就搜不到了，一个位置只改一次
        patched++;
        at = buf.indexOf(bytes, at + 16);
      }
    }
    assert(patched >= 14, `只改了 ${patched} 处资源编号`);
    writeFileSync(target, buf);

    const app = await startApp({ ROCK_SPROUT_DATA: dir2, ROCK_SPROUT_SAVE_DIR: saves2, ...DEAD_PROXY });
    const s2 = await connect(app.wsUrl);
    await waitReady(s2);
    await s2.clickText(".rail button", "我的数据");
    const text = await (async () => {
      for (let i = 0; i < 40; i++) {
        const t = await s2.evalJs(`document.querySelector('[data-page="data"]')?.innerText ?? ""`);
        if (/#[0-9a-f]{8}/.test(t)) return t;
        await sleep(200);
      }
      return s2.evalJs(`document.querySelector('[data-page="data"]')?.innerText ?? ""`);
    })();
    assert(/#[0-9a-f]{8}/.test(text), `没显示 #编号：${text.slice(0, 300)}`);
    assert(text.includes("对照表里还没有"), "带 # 的那行没解释");
    s2.close();
    killApp();
    await sleep(600);
  });

  s.close();
  killApp();
  await sleep(700);

  await scenario("一份存档都没有：说「没找到」", async () => {
    const emptyDir = freshDataDir("mydata-empty");
    const emptySaves = join(emptyDir, "saves");
    mkdirSync(emptySaves, { recursive: true });
    const app = await startApp({ ROCK_SPROUT_DATA: emptyDir, ROCK_SPROUT_SAVE_DIR: emptySaves, ...DEAD_PROXY });
    const s2 = await connect(app.wsUrl);
    await waitReady(s2);
    await s2.clickText(".rail button", "我的数据");
    await sleep(1500);
    const text = await dataText(s2);
    assert(text.includes("没找到存档"), `空目录该说没找到：${text.slice(0, 200)}`);
    assert(!text.includes("损坏"), "不该说存档坏了");
    s2.close();
    killApp();
    await sleep(600);
  });

  await scenario("版本不认识：说的是「版本不认识」，不是「存档损坏」", async () => {
    const dir2 = freshDataDir("mydata-version");
    const saves2 = join(dir2, "saves");
    mkdirSync(saves2, { recursive: true });
    const bad = join(saves2, basename(SAVE_A));
    const buf = readFileSync(SAVE_A);
    buf.writeInt32LE(99, 4); // saveGameFileVersion 改成 99
    writeFileSync(bad, buf);
    const app = await startApp({ ROCK_SPROUT_DATA: dir2, ROCK_SPROUT_SAVE_DIR: saves2, ...DEAD_PROXY });
    const s2 = await connect(app.wsUrl);
    await waitReady(s2);
    await s2.clickText(".rail button", "我的数据");
    await sleep(1800);
    const text = await dataText(s2);
    assert(text.includes("存档版本不认识"), `版本那话说得不对：${text.slice(0, 240)}`);
    assert(!text.includes("损坏"), "把版本问题说成了存档损坏");
    s2.close();
    killApp();
    await sleep(600);
  });

  await scenario("结构读不通：说「结构读不通」，不是「版本不认识」", async () => {
    const dir3 = freshDataDir("mydata-broken");
    const saves3 = join(dir3, "saves");
    mkdirSync(saves3, { recursive: true });
    const buf = readFileSync(SAVE_A).subarray(0, 4096); // 截断到一半，头还在、树断了
    writeFileSync(join(saves3, basename(SAVE_A)), buf);
    const app = await startApp({ ROCK_SPROUT_DATA: dir3, ROCK_SPROUT_SAVE_DIR: saves3, ...DEAD_PROXY });
    const s2 = await connect(app.wsUrl);
    await waitReady(s2);
    await s2.clickText(".rail button", "我的数据");
    await sleep(1800);
    const text = await dataText(s2);
    assert(text.includes("结构读不通") || text.includes("读不通"), `截断的档那话说得不对：${text.slice(0, 240)}`);
    assert(!text.includes("版本不认识"), "把截断说成了版本问题");
    s2.close();
    killApp();
    await sleep(600);
  });
}


phase("装备页：五块角色、八格固定、空格子、抽屉、改装清单");
{
  const dir = freshDataDir("equip");
  const saves = join(dir, "saves");
  mkdirSync(saves, { recursive: true });
  copyFileSync(join(TOOLS, "save-samples/76561199027062409_Player.sav"), join(saves, "76561199027062409_Player.sav"));
  const env = { ROCK_SPROUT_DATA: dir, ROCK_SPROUT_SAVE_DIR: saves, ...DEAD_PROXY };
  let app;
  let s;
  try {
    app = await startApp(env);
    s = await connect(app.wsUrl);
    await waitReady(s);
    await s.clickText(".rail button", "装备");
    await sleep(1500);

    const equipText = () => s.evalJs('document.querySelector(\'[data-page="equip"]\')?.innerText ?? ""');

    await scenario("装备页有 5 块，标题是四个职业加一个认不出的", async () => {
      // 只看角色卡：页底还有「皮肤」「十字镐」两块同名卡，不是角色
      const titles = await s.evalJs(`[...document.querySelectorAll('[data-page="equip"] .card[data-char] .card-title')].map((e) => e.innerText)`);
      assert(titles.length === 5, `角色块应该是 5 个，实际 ${titles.length}`);
      const joined = titles.join("|");
      for (const cl of ["钻机手", "工程师", "枪手", "侦察兵"]) {
        assert(joined.includes(cl), `标题里缺 ${cl}：${joined}`);
      }
      assert(joined.includes("角色 5"), `第 5 块应该写「角色 5」：${joined}`);
      const pageText = await equipText();
      assert(pageText.includes("认不出"), "第 5 块要写明认不出职业");
    });

    await scenario("每套配装都是同一套八格，顺序固定", async () => {
      const sets = await s.evalJs('document.querySelectorAll(\'[data-page="equip"] .equip-grid\').length');
      assert(sets >= 5, `配装套数太少：${sets}`);
      const first = await s.evalJs('[...document.querySelectorAll(\'[data-page="equip"] .equip-grid\')[0].children].map((c) => c.getAttribute("data-slot"))');
      assert(JSON.stringify(first) === JSON.stringify(["PrimaryWeapon", "SecondaryWeapon", "TraversalTool", "ClassTool", "Armor", "Flare", "MiningTool", "Grenade"]), `八格顺序不对：${first.join(",")}`);
    });

    await scenario("空着的格子写「没装」", async () => {
      const info = await s.evalJs('(() => { const cells = [...document.querySelectorAll(\'[data-page="equip"] .equip-cell.empty\')]; return { total: cells.length, withText: cells.filter((c) => (c.innerText || "").includes("没装")).length }; })()');
      assert(info.total > 0, "这份存档应该有空格子");
      assert(info.withText === info.total, `有空格子没写「没装」：${info.withText}/${info.total}`);
    });

    await scenario("点一格开抽屉，Esc 关掉", async () => {
      const opened = await s.evalJs('(() => { const c = document.querySelector(\'[data-page="equip"] .equip-cell:not(.empty)\'); if (!c) return null; c.click(); return c.querySelector(".equip-name")?.innerText ?? ""; })()');
      assert(opened !== null, "没点到装了东西的格子");
      await sleep(600);
      const has = await s.evalJs('!!document.querySelector(".equip-drawer")');
      assert(has, "点了格子没有抽屉");
      await s.evalJs('(() => { window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })); return true; })()');
      await sleep(400);
      assert(!(await s.evalJs('!!document.querySelector(".equip-drawer")')), "Esc 之后抽屉还在");
    });

    await scenario("抽屉里有改装清单，并写明「解锁与否是整份存档一份清单」", async () => {
      await s.evalJs('(() => { const c = document.querySelector(\'[data-page="equip"] .equip-cell:not(.empty)\'); if (c) c.click(); return !!c; })()');
      await sleep(800);
      const info = await s.evalJs(`(() => {
        const box = document.querySelector('.equip-drawer .equip-upgrades');
        if (!box) return null;
        return {
          total: box.querySelectorAll('.equip-upg').length,
          owned: box.querySelectorAll('.equip-upg.owned').length,
          note: box.innerText,
        };
      })()`);
      assert(info, "抽屉里没有改装清单");
      assert(info.total > 0, "改装清单是空的");
      assert(info.note.includes("不分武器"), `脚注不对：${info.note.slice(0, 120)}`);
      await s.evalJs('(() => { window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })); return true; })()');
      await sleep(300);
    });

    phase("装备页：超频、皮肤分组、十字镐、存档备份");

    // 这一版新接的三段都在同一张页上：超频块在抽屉里，皮肤与十字镐在页底，
    // 存档备份在设置页。都按界面上真正看得见的东西断言，不看后端字段。
    const readSkins = () =>
      s.evalJs(`(() => {
        const card = document.querySelector('[data-section="皮肤"]');
        if (!card) return null;
        const cells = [...card.querySelectorAll('.name-cell')];
        const diesel = cells.filter((c) => c.innerText.trim() === '柴油动力');
        return {
          cells: cells.length,
          owned: cells.filter((c) => c.getAttribute('data-owned') === '1').length,
          off: cells.filter((c) => c.classList.contains('name-cell-off')).length,
          label: card.querySelector('.card-title-row .dim')?.innerText.trim() ?? '',
          titles: [...card.querySelectorAll('.name-group-title')].map((e) => e.innerText.trim()),
          diesel: diesel.length,
          dieselGroups: new Set(diesel.map((c) => c.closest('.name-group')?.getAttribute('data-group'))).size,
        };
      })()`);

    const clickInSection = (title, label) =>
      s.evalJs(`(() => {
        const card = document.querySelector('[data-section="${title}"]');
        if (!card) return 'no-card';
        const b = [...card.querySelectorAll('button')].find((x) => x.innerText.trim() === '${label}');
        if (!b) return 'no-button';
        b.click();
        return 'ok';
      })()`);

    await scenario("超频块：装着的那条带稀有度，并写清一共几条可换", async () => {
      const found = await s.evalJs(`(async () => {
        const cells = [...document.querySelectorAll('[data-page="equip"] .equip-cell:not(.empty)')];
        for (let i = 0; i < cells.length; i += 1) {
          cells[i].click();
          await new Promise((r) => setTimeout(r, 150));
          const box = document.querySelector('.equip-overclock');
          const rarity = box?.querySelector('.equip-oc-rarity');
          if (rarity) {
            const info = {
              total: Number(box.getAttribute('data-oc-total')),
              name: box.querySelector('.equip-oc-name')?.innerText.trim() ?? '',
              rarity: rarity.innerText.trim(),
              text: box.innerText,
            };
            window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
            return info;
          }
          window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
          await new Promise((r) => setTimeout(r, 40));
        }
        return null;
      })()`);
      assert(found, "翻遍配装没找到装着超频的武器");
      assert(found.total > 0, `超频块应带可换条数，实际 ${found.total}`);
      assert(found.name.length > 0, "超频块里名字是空的");
      assert(["无瑕", "均衡", "不稳定"].includes(found.rarity), `稀有度不在三档里：${found.rarity}`);
      assert(/共有 \d+ 条可换/.test(found.text), `没写「共有 N 条可换」：${found.text.slice(0, 120)}`);
      await sleep(250);
    });

    await scenario("没有超频的格子不渲染超频块", async () => {
      const info = await s.evalJs(`(async () => {
        const cell = document.querySelector('[data-page="equip"] .equip-cell[data-slot="Armor"]:not(.empty)');
        if (!cell) return null;
        cell.click();
        await new Promise((r) => setTimeout(r, 300));
        const out = {
          drawer: !!document.querySelector('.equip-drawer'),
          overclock: !!document.querySelector('.equip-overclock'),
        };
        window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
        return out;
      })()`);
      assert(info && info.drawer, "护甲格子没打开抽屉，判不了");
      assert(!info.overclock, "护甲上没有超频，不该画超频块");
      await sleep(250);
    });

    await scenario("皮肤：首屏只列已拥有的，点「显示全部」才放全量", async () => {
      const first = await readSkins();
      assert(first, "页面上没有「皮肤」这一块");
      assert(first.cells > 0, "皮肤一块是空的");
      assert(first.cells === first.owned, `首屏必须全是已拥有：${first.cells} 条里 ${first.owned} 条有`);
      assert(first.off === 0, `首屏不该有灰掉的条目，实际 ${first.off} 条`);
      assert(first.label.includes(`已拥有 ${first.owned} 条`), `标题旁的数字不对：${first.label}`);
      assert((await clickInSection("皮肤", "显示全部")) === "ok", "皮肤块里找不到「显示全部」");
      await sleep(500);
      const all = await readSkins();
      assert(all.cells === 654, `全量应该是 654 条，实际 ${all.cells}`);
      assert(all.owned === first.owned, `展开后已拥有数不该变：${all.owned} ≠ ${first.owned}`);
      assert(all.off === all.cells - all.owned, "没拥有的条目应该灰掉");
      assert((await clickInSection("皮肤", "只看有的")) === "ok", "展开后按钮应该变成「只看有的」");
      await sleep(500);
      assert((await readSkins()).cells === first.owned, "点「只看有的」没收回去");
    });

    await scenario("皮肤分组没有空标题，柴油动力各回各的武器", async () => {
      assert((await clickInSection("皮肤", "显示全部")) === "ok", "找不到「显示全部」");
      await sleep(500);
      const all = await readSkins();
      const blank = all.titles.filter((t) => t.length === 0 || t === "未标注归属");
      assert(blank.length === 0, `还有空标题的分组：${JSON.stringify(blank)}`);
      assert(all.titles.length >= 25, `分组太少：${all.titles.length}`);
      assert(all.diesel === 25, `「柴油动力」应该是 25 条，实际 ${all.diesel}`);
      assert(all.dieselGroups === 25, `25 条柴油动力应该分在 25 个组里，实际 ${all.dieselGroups} 组`);
      await clickInSection("皮肤", "只看有的");
      await sleep(400);
    });

    await scenario("十字镐：只列已解锁的，按部位分组", async () => {
      const info = await s.evalJs(`(() => {
        const card = document.querySelector('[data-section="十字镐"]');
        if (!card) return null;
        const cells = [...card.querySelectorAll('.name-cell')];
        return {
          cells: cells.length,
          owned: cells.filter((c) => c.getAttribute('data-owned') === '1').length,
          titles: [...card.querySelectorAll('.name-group-title')].map((e) => e.innerText.trim()),
          text: card.innerText,
        };
      })()`);
      assert(info, "页面上没有「十字镐」这一块");
      assert(info.cells === 19, `首屏应该是 19 件已解锁，实际 ${info.cells}`);
      assert(info.cells === info.owned, "首屏不该出现没解锁的部件");
      assert(info.titles.length >= 2, `应该按部位分组，实际只有 ${info.titles.length} 组`);
      assert(info.titles.every((t) => t.length > 0), `有分组标题是空的：${JSON.stringify(info.titles)}`);
      assert(!info.text.includes("当前装配"), "十字镐不该说「当前装配」");
      assert((await clickInSection("十字镐", "显示全部")) === "ok", "十字镐块里找不到「显示全部」");
      await sleep(400);
      const all = await s.evalJs(`document.querySelectorAll('[data-section="十字镐"] .name-cell').length`);
      assert(all === 160, `全量应该是 160 件，实际 ${all}`);
      await clickInSection("十字镐", "只看有的");
      await sleep(300);
    });

    await scenario("三条任务工具不出现在装备页", async () => {
      const text = await s.evalJs(`document.querySelector('[data-page="equip"]').innerText`);
      for (const word of ["PipelineBuilder", "RockCracker", "ResonanceScanner", "管线建造器", "碎石机建造器", "共振扫描器"]) {
        assert(!text.includes(word), `界面上不该出现「${word}」`);
      }
    });

    await scenario("设置页存档备份：点一次出一份，连点两次出两份", async () => {
      await s.clickText(".rail button", "设置");
      await sleep(1000);
      const src = join(saves, "76561199027062409_Player.sav");
      const srcBytes = statSync(src).size;
      const backupsDir = join(dir, "backups");
      const listBackups = () =>
        existsSync(backupsDir)
          ? readdirSync(backupsDir).filter((n) => n.toLowerCase().endsWith(".sav"))
          : [];
      const backupOnce = async () => {
        const clicked = await s.evalJs(`(() => {
          const card = [...document.querySelectorAll('.card')].find((c) => c.querySelector('.card-title')?.innerText.trim() === '存档备份');
          if (!card) return 'no-card';
          const b = [...card.querySelectorAll('button')].find((x) => /备份一次|再备份一次/.test(x.innerText));
          if (!b) return 'no-button';
          b.click();
          return 'ok';
        })()`);
        assert(clicked === "ok", `找不到存档备份卡上的按钮：${clicked}`);
        for (let i = 0; i < 40; i += 1) {
          const busy = await s.evalJs(`(() => {
            const card = [...document.querySelectorAll('.card')].find((c) => c.querySelector('.card-title')?.innerText.trim() === '存档备份');
            const b = card ? [...card.querySelectorAll('button')].find((x) => /备份一次|再备份一次/.test(x.innerText)) : null;
            return b ? b.disabled : false;
          })()`);
          if (!busy) break;
          await sleep(250);
        }
        await sleep(300);
      };

      await backupOnce();
      const first = listBackups();
      assert(first.length === 1, `点一次应该只有一份，实际 ${first.length}`);
      assert(statSync(join(backupsDir, first[0])).size === srcBytes, "备份大小和原档对不上");
      const text1 = await s.evalJs(`(() => {
        const card = [...document.querySelectorAll('.card')].find((c) => c.querySelector('.card-title')?.innerText.trim() === '存档备份');
        return card ? card.innerText : '';
      })()`);
      assert(/最近一次备份：\d+月\d+日\d{2}:\d{2}|\d+ 月 \d+ 日 \d{2}:\d{2}/.test(text1.replace(/\s+/g, " ")), `没显示最近一次备份：${text1.slice(0, 160)}`);
      assert(/一共 1 份/.test(text1), "份数没更新成 1");

      await sleep(1200);
      await backupOnce();
      const second = listBackups();
      assert(second.length === 2, `点两次应该有两份，实际 ${second.length}`);
      assert(new Set(second).size === 2, `两份名字一样，时间戳没起作用：${JSON.stringify(second)}`);
      assert(statSync(join(backupsDir, second[0])).size === srcBytes, "第二份大小和原档对不上");
      const text2 = await s.evalJs(`(() => {
        const card = [...document.querySelectorAll('.card')].find((c) => c.querySelector('.card-title')?.innerText.trim() === '存档备份');
        return card ? card.innerText : '';
      })()`);
      assert(/一共 2 份/.test(text2), "份数没更新成 2");
      await s.clickText(".rail button", "装备");
      await sleep(600);
    });

  } finally {
    if (s) s.close();
    killApp();
    await sleep(600);
  }
}

phase("天赋页：三个点数、24 条清单、未买灰显、角色配装");
{
  const dir = freshDataDir("perks");
  const saves = join(dir, "saves");
  mkdirSync(saves, { recursive: true });
  copyFileSync(join(TOOLS, "save-samples/76561199027062409_Player.sav"), join(saves, "76561199027062409_Player.sav"));
  const env = { ROCK_SPROUT_DATA: dir, ROCK_SPROUT_SAVE_DIR: saves, ...DEAD_PROXY };
  let s;
  try {
    const app = await startApp(env);
    s = await connect(app.wsUrl);
    await waitReady(s);

    scenario("侧栏里「天赋」排在我的数据和装备之间", async () => {
      const order = await s.evalJs(`[...document.querySelectorAll('.rail .nav-item')].map((b) => b.textContent.trim())`);
      const iData = order.indexOf("我的数据");
      const iPerks = order.indexOf("天赋");
      const iEquip = order.indexOf("装备");
      assert(iData >= 0 && iPerks >= 0 && iEquip >= 0, `侧栏少了页：${JSON.stringify(order)}`);
      assert(iData < iPerks && iPerks < iEquip, `天赋的位置不对：${JSON.stringify(order)}`);
    });

    await s.clickText(".rail button", "天赋");
    await sleep(1000);

    scenario("天赋页：24 条、21 条已买、未买排在后面且灰显", async () => {
      const v = await s.evalJs(`(() => {
        const page = document.querySelector('[data-page="perks"]');
        const rows = [...page.querySelectorAll('.perk-row')];
        const owned = rows.filter((r) => r.dataset.owned === '1');
        const off = rows.filter((r) => r.dataset.owned === '0');
        return {
          rows: rows.length,
          owned: owned.length,
          off: off.length,
          lastOwned: rows.map((r) => r.dataset.owned).lastIndexOf('1'),
          firstOff: rows.map((r) => r.dataset.owned).indexOf('0'),
          offGrey: off.every((r) => r.classList.contains('perk-row-off')),
          offClickable: off.filter((r) => r.querySelector('button, a')).length,
        };
      })()`);
      assert(v.rows === 24, `应该 24 条，实际 ${v.rows}`);
      assert(v.owned === 21, `应该 21 条已买，实际 ${v.owned}`);
      assert(v.off === 3, `应该 3 条未买，实际 ${v.off}`);
      assert(v.lastOwned < v.firstOff, "已买没有全部排在未买前面");
      assert(v.offGrey, "未买的条目没有灰显");
      assert(v.offClickable === 0, `未买的条目上不该有点击控件，实际 ${v.offClickable} 个`);
    });

    scenario("天赋页：三个点数对得上存档（346 − 311 = 35）", async () => {
      const v = await s.evalJs(`(() => {
        const page = document.querySelector('[data-page="perks"]');
        const card = page.querySelector('[data-points]');
        const nums = [...card.querySelectorAll('.data-value')].map((e) => Number(e.textContent.replace(/[^0-9-]/g, '')));
        return { state: card.dataset.points, nums };
      })()`);
      assert(v.state === "ok", `点数区状态是 ${v.state}，应该 ok`);
      assert(v.nums.length === 3, `应该三个数，实际 ${v.nums.length} 个：${JSON.stringify(v.nums)}`);
      const [earned, spent, left] = v.nums;
      assert(earned === 346 && spent === 311 && left === 35, `三个数不对：${JSON.stringify(v.nums)}`);
      assert(earned - spent === left, "已赚 − 已花 与还剩对不上");
    });

    scenario("天赋页：条目都有中文名，级别与花费写全", async () => {
      const v = await s.evalJs(`(() => {
        const page = document.querySelector('[data-page="perks"]');
        const rows = [...page.querySelectorAll('.perk-row')];
        const names = rows.map((r) => r.querySelector('.perk-name').textContent.trim());
        const metas = rows.map((r) => r.querySelector('.perk-meta')?.textContent.trim() ?? '');
        return {
          names,
          rawGuid: names.filter((n) => !n || /^[0-9a-f]{8}$/.test(n)),
          rankTexts: metas.filter((m) => /^\d+ \/ \d+ 级$/.test(m)).length,
          noRanks: metas.filter((m) => m === '没有分级').length,
          costTexts: metas.filter((m) => /^共 \d+ 点$/.test(m)).length,
        };
      })()`);
      assert(v.rawGuid.length === 0, `还有没换成名字的编号：${JSON.stringify(v.rawGuid)}`);
      assert(v.rankTexts === 23, `应该有 23 条写「x / y 级」，实际 ${v.rankTexts}`);
      assert(v.noRanks === 1, `应该只有 1 条「没有分级」，实际 ${v.noRanks}`);
      assert(v.costTexts === 23, `应该有 23 条写总花费，实际 ${v.costTexts}`);
    });

    scenario("天赋页：页底四个角色各带一套，名字都认得出来", async () => {
      const v = await s.evalJs(`(() => {
        const page = document.querySelector('[data-page="perks"]');
        const cards = [...page.querySelectorAll('.perk-loadout')];
        return {
          n: cards.length,
          heads: cards.map((c) => c.querySelector('.perk-loadout-head').textContent.trim()),
          counts: cards.map((c) => c.querySelector('.perk-loadout-perks').textContent.trim().split('、').length),
          bodies: cards.map((c) => c.querySelector('.perk-loadout-perks').textContent),
        };
      })()`);
      assert(v.n === 4, `应该有 4 个角色，实际 ${v.n}`);
      assert(v.heads.every((h, i) => h === `角色 ${i + 1}`), `角色标题不对：${JSON.stringify(v.heads)}`);
      assert(v.counts.every((n) => n >= 4), `每个角色至少 4 个天赋：${JSON.stringify(v.counts)}`);
      assert(v.bodies.every((t) => !/[0-9a-f]{8}/.test(t)), `配装里还有编号没换成名字：${JSON.stringify(v.bodies)}`);
    });
  } finally {
    if (s) s.close();
    killApp();
    await sleep(600);
  }
}

phase("天赋页：点数对不上时的降级（构造档）");
{
  // 造一份「已赚 − 已花 ≠ 存档 PerkPoints」的档：把副本里的 PerkPoints 从 35 改成 42，
  // 两个来源立刻对不上。改的是副本，原档一个字节都没动。
  const dir = freshDataDir("perks-bad");
  const saves = join(dir, "saves");
  mkdirSync(saves, { recursive: true });
  const src = join(TOOLS, "save-samples/76561199027062409_Player.sav");
  const buf = readFileSync(src);
  const name = Buffer.from("PerkPoints\0", "latin1");
  const at = buf.indexOf(name);
  assert(at > 0, "存档里找不到 PerkPoints 字段");
  let j = at + name.length;
  const typeLen = buf.readInt32LE(j);
  assert(typeLen > 0 && typeLen < 64, `PerkPoints 的类型长度不对劲：${typeLen}`);
  j += 4 + typeLen;
  const size = buf.readInt32LE(j);
  assert(size === 4, `PerkPoints 的值不是 4 字节：${size}`);
  j += 4 + 4 + 1; // 大小 + 数组下标 + 有没有属性 GUID 的那个标志位
  const old = buf.readInt32LE(j);
  assert(old === 35, `原档 PerkPoints 应该是 35，实际 ${old}`);
  buf.writeInt32LE(42, j);
  writeFileSync(join(saves, "76561199027062409_Player.sav"), buf);
  const env = { ROCK_SPROUT_DATA: dir, ROCK_SPROUT_SAVE_DIR: saves, ...DEAD_PROXY };
  let s;
  try {
    const app = await startApp(env);
    s = await connect(app.wsUrl);
    await waitReady(s);
    await s.clickText(".rail button", "天赋");
    await sleep(1000);

    scenario("天赋页：对不上时只说「先不显示」，清单照旧 24 条", async () => {
      const v = await s.evalJs(`(() => {
        const page = document.querySelector('[data-page="perks"]');
        const card = page.querySelector('[data-points]');
        return {
          state: card?.dataset.points ?? '',
          text: page.innerText.replace(/\\s+/g, ' '),
          values: card ? card.querySelectorAll('.data-value').length : 0,
          rows: page.querySelectorAll('.perk-row').length,
          owned: [...page.querySelectorAll('.perk-row')].filter((r) => r.dataset.owned === '1').length,
        };
      })()`);
      assert(v.state === "off", `点数区状态应该是 off，实际「${v.state}」`);
      assert(v.text.includes("点数对不上，先不显示"), `没写「点数对不上，先不显示」：${v.text.slice(0, 200)}`);
      assert(v.values === 0, `对不上时不该显示任何点数，实际 ${v.values} 个`);
      assert(v.rows === 24 && v.owned === 21, `清单应该照旧 24 条 / 21 条已买，实际 ${v.rows} / ${v.owned}`);
    });
  } finally {
    if (s) s.close();
    killApp();
    await sleep(600);
  }
}

phase("本周页：赛季、深潜、战役三块与刷新时刻");
{
  const dir = freshDataDir("week");
  const saves = join(dir, "saves");
  mkdirSync(saves, { recursive: true });
  copyFileSync(join(TOOLS, "save-samples/76561199027062409_Player.sav"), join(saves, "76561199027062409_Player.sav"));
  const env = { ROCK_SPROUT_DATA: dir, ROCK_SPROUT_SAVE_DIR: saves, ...DEAD_PROXY };
  let s;
  try {
    const app = await startApp(env);
    s = await connect(app.wsUrl);
    await waitReady(s);

    await scenario("侧栏里「本周」在背包之后、设置之前", async () => {
      const order = await s.evalJs(`[...document.querySelectorAll('.rail .nav-item')].map((b) => b.textContent.trim())`);
      const iBag = order.indexOf("背包");
      const iWeek = order.indexOf("本周");
      const iSet = order.indexOf("设置");
      assert(iBag >= 0 && iWeek >= 0 && iSet >= 0, `侧栏少了页：${JSON.stringify(order)}`);
      assert(iBag < iWeek && iWeek < iSet, `本周的位置不对：${JSON.stringify(order)}`);
    });

    await s.clickText(".rail button", "本周");
    await sleep(1200);

    await scenario("赛季块：两季的名字、XP 与已领条数都对得上存档", async () => {
      const v = await s.evalJs(`(() => {
        const page = document.querySelector('[data-page="week"]');
        const sec = [...page.querySelectorAll('.week-section')][0];
        const blocks = [...page.querySelectorAll('.week-season')].map((x) => ({
          name: x.querySelector('.week-season-name')?.textContent.trim() ?? '',
          stat: x.querySelector('.week-season-stat')?.textContent.replace(/\\s+/g, ' ').trim() ?? '',
          challenges: x.querySelectorAll('.week-challenge').length,
        }));
        return { blocks, text: sec.innerText };
      })()`);
      assert(v.blocks.length === 2, `应该有 2 个赛季，实际 ${v.blocks.length}`);
      assert(v.blocks[0].name === "生物危害", `第一季是「${v.blocks[0].name}」`);
      assert(v.blocks[1].name === "腐化临界", `第二季是「${v.blocks[1].name}」`);
      assert(/24\.7 万/.test(v.blocks[0].stat) && /70 \/ 100/.test(v.blocks[0].stat), `第一季的数字不对：${v.blocks[0].stat}`);
      assert(/7\.5 万/.test(v.blocks[1].stat) && /30 \/ 100/.test(v.blocks[1].stat), `第二季的数字不对：${v.blocks[1].stat}`);
      assert(v.blocks[0].challenges + v.blocks[1].challenges === 5, `挑战条数不对：${JSON.stringify(v.blocks.map((b) => b.challenges))}`);
      assert(!/[{}]/.test(v.text), "赛季块里有没填的模板占位符");
      assert(!v.text.includes("下次刷新"), "赛季挑战那一段不该写刷新时刻");
    });

    await scenario("深潜块默认收起，展开后两档状态与种子说明都在", async () => {
      const closed = await s.evalJs(`(() => {
        const sec = [...document.querySelectorAll('[data-page="week"] .week-section')][1];
        return { open: sec.dataset.open };
      })()`);
      assert(closed.open === "0", `深潜默认应该是收起的，实际 data-open=${closed.open}`);
      await s.clickText('[data-page="week"] .week-section .week-head', "深潜");
      await sleep(500);
      const v = await s.evalJs(`(() => {
        const sec = [...document.querySelectorAll('[data-page="week"] .week-section')][1];
        return { text: sec.innerText.replace(/\\s+/g, ' '), reset: sec.querySelector('.week-reset')?.textContent.trim() ?? '' };
      })()`);
      assert(v.text.includes("完成过"), `没写「完成过」：${v.text.slice(0, 160)}`);
      assert(v.text.includes("打到第 2 关"), `没写「打到第 2 关」：${v.text.slice(0, 160)}`);
      assert(v.text.includes("读不到关卡名"), "缺少种子说明");
      assert(/下次刷新：\d+ 月 \d+ 日（周四）11:00 UTC/.test(v.reset), `刷新行不对：${v.reset}`);
    });

    await scenario("战役块：13 条已知 + 2 条未知，未知只给可复制的短编号", async () => {
      await s.clickText('[data-page="week"] .week-section .week-head', "战役");
      await sleep(500);
      const v = await s.evalJs(`(() => {
        const sec = [...document.querySelectorAll('[data-page="week"] .week-section')][2];
        const rows = [...sec.querySelectorAll('.week-campaign')];
        const unknown = [...sec.querySelectorAll('.week-unknown-id')];
        return {
          rows: rows.length,
          retired: rows.filter((r) => r.innerText.includes("已停办的活动")).length,
          removedTitle: sec.querySelector('.week-removed-title')?.textContent.trim() ?? '',
          unknown: unknown.map((b) => ({ text: b.textContent.trim(), title: b.getAttribute('title') ?? '' })),
          reset: sec.querySelector('.week-reset')?.textContent.trim() ?? '',
        };
      })()`);
      assert(v.rows === 15, `战役行应该 15 条（13 + 2），实际 ${v.rows}`);
      assert(v.retired === 2, `已停办应该有 2 条，实际 ${v.retired}`);
      assert(v.removedTitle === "已从游戏里移除的活动（2 个）", `标题是「${v.removedTitle}」`);
      assert(v.unknown.length === 2, `未知编号应该 2 条，实际 ${v.unknown.length}`);
      assert(v.unknown.every((u) => /^[0-9a-f]{8}$/.test(u.text) && u.title === "点一下复制这个编号"), `未知编号的样式不对：${JSON.stringify(v.unknown)}`);
      assert(/下次刷新：\d+ 月 \d+ 日（周四）11:00 UTC/.test(v.reset), `刷新行不对：${v.reset}`);
    });
  } finally {
    if (s) s.close();
    killApp();
    await sleep(600);
  }
}

phase("本周页：多份存档时能换档，换完数字跟着变");
{
  const dir = freshDataDir("week-pick");
  const saves = join(dir, "saves");
  mkdirSync(saves, { recursive: true });
  copyFileSync(join(TOOLS, "save-samples/76561199027062409_Player.sav"), join(saves, "76561199027062409_Player.sav"));
  copyFileSync(join(TOOLS, "save-samples/76561199218191195_Player.sav"), join(saves, "76561199218191195_Player.sav"));
  // 让 218191195 成为「最新」那份：默认选中它，切换目标才是另一份
  const nowSec = Date.now() / 1000;
  utimesSync(join(saves, "76561199027062409_Player.sav"), nowSec - 600, nowSec - 600);
  utimesSync(join(saves, "76561199218191195_Player.sav"), nowSec, nowSec);
  const env = { ROCK_SPROUT_DATA: dir, ROCK_SPROUT_SAVE_DIR: saves, ...DEAD_PROXY };
  let s;
  try {
    const app = await startApp(env);
    s = await connect(app.wsUrl);
    await waitReady(s);
    await s.clickText(".rail button", "本周");
    await sleep(1500);

    await scenario("本周页：两份存档给下拉框，换档后 XP 与已领条数都跟着变", async () => {
      const before = await s.evalJs(`(() => {
        const p = document.querySelector('[data-page="week"]');
        const sel = p.querySelector('.save-pick select');
        return {
          hasSelect: !!sel,
          options: sel ? [...sel.options].map((o) => o.textContent.trim()) : [],
          stats: [...p.querySelectorAll('.week-season-stat')].map((e) => e.textContent.replace(/\\s+/g, ' ').trim()),
        };
      })()`);
      assert(before.hasSelect, "两份存档时应该给下拉框");
      assert(before.options.length === 2, `下拉框应该有 2 项，实际 ${before.options.length}`);
      assert(before.stats.some((t) => /22\.9 万/.test(t)), `默认那份应是 22.9 万，实际 ${JSON.stringify(before.stats)}`);

      const target = await s.evalJs(`(() => {
        const sel = document.querySelector('[data-page="week"] .save-pick select');
        return [...sel.options].find((o) => o.textContent.includes('062409'))?.value ?? null;
      })()`);
      assert(target, "下拉框里找不到 76561199027062409");
      await s.evalJs(`(() => {
        const sel = document.querySelector('[data-page="week"] .save-pick select');
        sel.value = ${JSON.stringify(target)};
        sel.dispatchEvent(new Event('change', { bubbles: true }));
        return true;
      })()`);
      await sleep(2200);

      const after = await s.evalJs(`(() => {
        const p = document.querySelector('[data-page="week"]');
        const sel = p.querySelector('.save-pick select');
        return {
          value: sel?.value ?? '',
          stats: [...p.querySelectorAll('.week-season-stat')].map((e) => e.textContent.replace(/\\s+/g, ' ').trim()),
        };
      })()`);
      assert(after.value === target, `换档后下拉框应该停在目标那份，实际 ${after.value}`);
      assert(after.stats.some((t) => /24\.7 万/.test(t)), `换档后应显示 24.7 万，实际 ${JSON.stringify(after.stats)}`);
      assert(after.stats.some((t) => /70 \/ 100/.test(t)), `换档后应显示 70 / 100，实际 ${JSON.stringify(after.stats)}`);
    });
  } finally {
    if (s) s.close();
    killApp();
    await sleep(600);
  }
}

// v0.2.6 的补充：商店两态、酒吧三层、八条补充、PAGES 恢复装备页。
async function waitBlock(s, name) {
  for (let i = 0; i < 240; i++) {
    const has = await s.evalJs(`!!document.querySelector('[data-block="${name}"]')`);
    if (has) return true;
    await sleep(250);
  }
  return false;
}

/** 改存档里一个 int 属性（顶层的 IntProperty，不带 GUID） */
function patchSaveInt(src, dst, prop, value) {
  const buf = readFileSync(src);
  const needle = Buffer.from(prop + "\0", "utf8");
  const at = buf.indexOf(needle);
  assert(at > 0, `存档里找不到 ${prop}`);
  assert(buf.indexOf(needle, at + 1) === -1, `${prop} 在存档里不止一处，别乱改`);
  let p = at + needle.length;
  const typeLen = buf.readInt32LE(p);
  p += 4 + typeLen + 4 + 4; // 类型名 / size / 数组下标
  const hasGuid = buf.readUInt8(p);
  p += 1;
  assert(hasGuid === 0, `${prop} 带了 GUID 标志，改法不对`);
  const old = buf.readInt32LE(p);
  buf.writeInt32LE(value, p);
  writeFileSync(dst, buf);
  return old;
}

phase("v0.2.6：商店、酒吧、八条补充");

// ---- 商店：有折扣的缓存回包 + DLC 一句话
{
  const dir = freshDataDir("v026store");
  const saves = join(dir, "saves");
  mkdirSync(saves, { recursive: true });
  copyFileSync(join(TOOLS, "save-samples/76561199027062409_Player.sav"), join(saves, "76561199027062409_Player.sav"));
  mkdirSync(join(dir, "cache"), { recursive: true });
  writeFileSync(
    join(dir, "cache/steam.json"),
    JSON.stringify({
      fetchedAt: Math.floor(Date.now() / 1000),
      cc: "CN",
      appid: 548430,
      price: { currency: "CNY", initial: 21600, final: 10800, discountPercent: 50, available: true },
      players: 4321,
    }),
  );
  let app;
  let s;
  try {
    app = await startApp({ ROCK_SPROUT_DATA: dir, ROCK_SPROUT_SAVE_DIR: saves, ...DEAD_PROXY });
    s = await connect(app.wsUrl);
    await waitReady(s);
    await waitBlock(s, "store");
    await scenario("商店：现价、原价、折扣、在线人数、DLC 一句话都在", async () => {
      const t = await s.evalJs(`(() => {
        const el = document.querySelector('[data-block="store"]');
        return el ? el.innerText.replace(/\\s+/g, ' ').trim() : '';
      })()`);
      assert(/¥108\.00/.test(t), `应该显示现价 ¥108.00，实际：${t}`);
      assert(/¥216\.00/.test(t), `应该保留原价 ¥216.00，实际：${t}`);
      assert(/-50%/.test(t), `应该有 -50%，实际：${t}`);
      assert(/4,321 人在线/.test(t), `应该显示 4,321 人在线，实际：${t}`);
      assert(/另有 13 个皮肤包，¥33–¥58/.test(t), `DLC 应该只写一句，实际：${t}`);
      assert(/拿到的/.test(t), `缓存回包应标出拿到的时刻，实际：${t}`);
    });
  } finally {
    if (s) s.close();
    killApp();
    await sleep(600);
  }
}

// ---- 商店：这个区没有价格
{
  const dir = freshDataDir("v026store2");
  const saves = join(dir, "saves");
  mkdirSync(saves, { recursive: true });
  copyFileSync(join(TOOLS, "save-samples/76561199027062409_Player.sav"), join(saves, "76561199027062409_Player.sav"));
  mkdirSync(join(dir, "cache"), { recursive: true });
  writeFileSync(
    join(dir, "cache/steam.json"),
    JSON.stringify({
      fetchedAt: Math.floor(Date.now() / 1000),
      cc: "CN",
      appid: 548430,
      price: { currency: "", initial: 0, final: 0, discountPercent: 0, available: false },
      players: null,
    }),
  );
  let app;
  let s;
  try {
    app = await startApp({ ROCK_SPROUT_DATA: dir, ROCK_SPROUT_SAVE_DIR: saves, ...DEAD_PROXY });
    s = await connect(app.wsUrl);
    await waitReady(s);
    await waitBlock(s, "store");
    await scenario("商店：这个区看不到价格时说的是人话，不是报错", async () => {
      const t = await s.evalJs(`(() => {
        const el = document.querySelector('[data-block="store"]');
        return el ? el.innerText.replace(/\\s+/g, ' ').trim() : '';
      })()`);
      assert(/这个区看不到价格/.test(t), `应该写「这个区看不到价格」，实际：${t}`);
      assert(/人数没取到/.test(t), `人数没取到也要写清，实际：${t}`);
    });
  } finally {
    if (s) s.close();
    killApp();
    await sleep(600);
  }
}

// ---- 酒吧：三层判定（正常档），并顺手把八条里的经验条一起验了
{
  const dir = freshDataDir("v026bar");
  const saves = join(dir, "saves");
  mkdirSync(saves, { recursive: true });
  copyFileSync(join(TOOLS, "save-samples/76561199027062409_Player.sav"), join(saves, "76561199027062409_Player.sav"));
  let app;
  let s;
  try {
    app = await startApp({ ROCK_SPROUT_DATA: dir, ROCK_SPROUT_SAVE_DIR: saves, ...DEAD_PROXY });
    s = await connect(app.wsUrl);
    await waitReady(s);
    await waitBlock(s, "bar");
    await scenario("酒吧：32 款、能点 25、缺执照 7，组内能点的排在前面", async () => {
      const d = await s.evalJs(`(() => {
        const el = document.querySelector('[data-block="bar"]');
        const rows = [...el.querySelectorAll('.bar-row')];
        const seq = rows.map((r) => r.dataset.state);
        const groupSeq = (g) => rows.filter((r) => r.dataset.group === g).map((r) => r.dataset.state);
        return {
          rows: rows.length,
          ok: rows.filter((r) => r.dataset.state === 'ok').length,
          license: rows.filter((r) => r.dataset.state === 'license').length,
          short: rows.filter((r) => r.dataset.state === 'short').length,
          stock: [...el.querySelectorAll('.bar-stock-item')].length,
          crafted: groupSeq('crafted'),
        };
      })()`);
      assert(d.rows === 32, `应该有 32 款酒，实际 ${d.rows}`);
      assert(d.ok === 25, `能点的应该 25 款，实际 ${d.ok}`);
      assert(d.license === 7, `缺执照的应该 7 款，实际 ${d.license}`);
      assert(d.short === 0, `这份存档不该有缺料的，实际 ${d.short}`);
      assert(d.stock === 4, `顶部应有 4 种原料，实际 ${d.stock}`);
      const firstLicense = d.crafted.indexOf("license");
      if (firstLicense !== -1) {
        assert(
          d.crafted.slice(firstLicense).every((x) => x === "license"),
          `组内排序错了：缺执照的没排在最后，实际 ${d.crafted.join(",")}`,
        );
      }
    });

    await s.clickText(".rail button", "我的数据");
    for (let i = 0; i < 120; i++) {
      if (await s.evalJs("!!document.querySelector('.xp-bar')")) break;
      await sleep(250);
    }
    await scenario("经验条：有下一级的才有条，满级和空槽不画", async () => {
      const d = await s.evalJs(`(() => {
        const ul = [...document.querySelectorAll('.dwarf-list')].find((u) => u.querySelector('.dwarf-slot'));
        const lis = [...ul.querySelectorAll(':scope > li')];
        return lis.map((li) => ({
          slot: li.querySelector('.dwarf-slot')?.innerText ?? '',
          text: li.innerText.replace(/\\s+/g, ' ').trim(),
          bar: li.querySelector('.xp-bar i')?.style.width ?? null,
        }));
      })()`);
      const withBar = d.filter((x) => x.bar !== null);
      assert(withBar.length === 2, `应该有 2 条经验条（两个 3 级角色），实际 ${withBar.length}`);
      assert(withBar.every((x) => /\d+ \/ [\d,]+ XP/.test(x.text)), `经验条旁边要写 当前/下一级 XP：${JSON.stringify(withBar)}`);
      assert(
        d.some((x) => /等级 25\/25/.test(x.text) && x.bar === null),
        `满级角色不该画条：${JSON.stringify(d.map((x) => [x.text, x.bar]))}`,
      );
      assert(d.some((x) => /还没进度/.test(x.text) && x.bar === null), "空槽不该画条");
    });

    await s.clickText(".rail button", "装备");
    for (let i = 0; i < 160; i++) {
      if (await s.evalJs("!!document.querySelector('[data-summary]')")) break;
      await sleep(250);
    }
    await scenario("装备页汇总：模组/皮肤/无人机/十字镐/外观等级/时装，百分比只在标题行", async () => {
      const t = await s.evalJs(`document.querySelector('[data-summary]').parentElement.innerText.replace(/\\s+/g, ' ')`);
      assert(/模组 10 \/ 164（6%）/.test(t), `模组那一项不对：${t}`);
      assert(/皮肤 30 \/ 654（5%）/.test(t), `皮肤那一项不对：${t}`);
      assert(/无人机皮肤 4 \/ 40（10%）/.test(t), `无人机那一项不对：${t}`);
      assert(/十字镐 19 \/ 160（12%）/.test(t), `十字镐那一项不对：${t}`);
      assert(/外观等级 9 级/.test(t), `外观等级不对：${t}`);
      assert(/时装 已拥有 6 件/.test(t), `时装件数不对：${t}`);
      assert(/分母含活动限定与已停用项/.test(t), `分母说明没写：${t}`);
    });

    await s.clickText(".rail button", "本周");
    for (let i = 0; i < 160; i++) {
      if (await s.evalJs("!!document.querySelector('.week-section')")) break;
      await sleep(250);
    }
    await s.evalJs(`(() => {
      const sec = [...document.querySelectorAll('.week-section')].find((x) =>
        (x.querySelector('.card-title')?.innerText || '').includes('深潜'),
      );
      sec?.querySelector('.week-toggle')?.click();
      return true;
    })()`);
    await sleep(600);
    await scenario("本周：深潜段的刷新行带「还有 N 天」，赛季段没有", async () => {
      const d = await s.evalJs(`(() => {
        const page = document.querySelector('[data-page="week"]');
        const resets = [...page.querySelectorAll('.week-reset')].map((p) => p.innerText.trim());
        const season = page.querySelector('.week-section');
        return { resets, seasonHasReset: !!(season && season.querySelector('.week-reset')) };
      })()`);
      assert(d.resets.length >= 1, `深潜段应该有刷新行，实际 ${JSON.stringify(d.resets)}`);
      assert(
        d.resets.every((x) => /下次刷新：\d+ 月 \d+ 日（周四）11:00 UTC · 还有 \d+ 天/.test(x)),
        `刷新行格式不对：${JSON.stringify(d.resets)}`,
      );
      assert(!d.seasonHasReset, "赛季挑战不属于周四刷新，赛季段不该有刷新行");
    });
  } finally {
    if (s) s.close();
    killApp();
    await sleep(600);
  }
}

// ---- 酒吧：信用点不够时的「还差 N 信用点」
{
  const dir = freshDataDir("v026bar2");
  const saves = join(dir, "saves");
  mkdirSync(saves, { recursive: true });
  const src = join(TOOLS, "save-samples/76561199027062409_Player.sav");
  const dst = join(saves, "76561199027062409_Player.sav");
  const oldCredits = patchSaveInt(src, dst, "Credits", 0);
  let app;
  let s;
  try {
    app = await startApp({ ROCK_SPROUT_DATA: dir, ROCK_SPROUT_SAVE_DIR: saves, ...DEAD_PROXY });
    s = await connect(app.wsUrl);
    await waitReady(s);
    await waitBlock(s, "bar");
    await scenario(`酒吧：信用点归零后三层里的「差料」就出现了（原值 ${oldCredits} 改成 0）`, async () => {
      const d = await s.evalJs(`(() => {
        const el = document.querySelector('[data-block="bar"]');
        const rows = [...el.querySelectorAll('.bar-row')];
        const short = rows.filter((r) => r.dataset.state === 'short');
        return {
          short: short.length,
          text: short.slice(0, 3).map((r) => r.innerText.replace(/\\s+/g, ' ').trim()),
          hasShortText: short.slice(0, 3).every((r) => /还差 \\d+ 信用点/.test(r.innerText)),
        };
      })()`);
      assert(d.short >= 2, `信用点归零后应该至少 2 款差信用点，实际 ${d.short}`);
      assert(d.hasShortText, `差料那行应写「还差 N 信用点」，实际 ${JSON.stringify(d.text)}`);
    });
  } finally {
    if (s) s.close();
    killApp();
    await sleep(600);
  }
}

// ---- 上次停在装备页：重启后还在装备页（PAGES 补 equip 的回归）
{
  const dir = freshDataDir("v026pages");
  const saves = join(dir, "saves");
  mkdirSync(saves, { recursive: true });
  copyFileSync(join(TOOLS, "save-samples/76561199027062409_Player.sav"), join(saves, "76561199027062409_Player.sav"));
  writeFileSync(
    join(dir, "ui-state.json"),
    JSON.stringify({ page: "equip", token_hint_collapsed: false, save_file: "76561199027062409_Player.sav" }),
  );
  let app;
  let s;
  try {
    app = await startApp({ ROCK_SPROUT_DATA: dir, ROCK_SPROUT_SAVE_DIR: saves, ...DEAD_PROXY });
    s = await connect(app.wsUrl);
    await waitReady(s);
    await sleep(800);
    await scenario("上次停在装备页：重启后回到装备页，不是被当成不认识的页", async () => {
      const d = await s.evalJs(`(() => {
        const on = [...document.querySelectorAll('.rail .nav-item.on')].map((b) => b.innerText.trim());
        const page = document.querySelector('[data-page="equip"]');
        return { on, hidden: page ? page.hasAttribute('hidden') : null };
      })()`);
      assert(d.on.length === 1 && d.on[0] === "装备", `侧栏应停在装备，实际 ${JSON.stringify(d.on)}`);
      assert(d.hidden === false, `装备页应该是显示状态，实际 hidden=${d.hidden}`);
    });
  } finally {
    if (s) s.close();
    killApp();
    await sleep(600);
  }
}

phase("v0.2.7：好友页的四态、排序与未公开折叠");
{
  const dir = freshDataDir("friends");
  mkdirSync(join(dir, "saves"), { recursive: true });
  copyFileSync(join(TOOLS, "save-samples/76561199027062409_Player.sav"),
    join(dir, "saves/76561199027062409_Player.sav"));
  mkdirSync(join(dir, "cache"), { recursive: true });
  const env = { ROCK_SPROUT_DATA: dir, ROCK_SPROUT_SAVE_DIR: join(dir, "saves"), ...DEAD_PROXY };
  let s;
  try {
    const app = await startApp(env);
    s = await connect(app.wsUrl);
    await waitReady(s);

    await scenario("侧栏里「好友」在本周之后、设置之前", async () => {
      const order = await s.evalJs(`[...document.querySelectorAll('.rail .nav-item')].map((b) => b.textContent.trim())`);
      const iWeek = order.indexOf("本周"), iFriends = order.indexOf("好友"), iSet = order.indexOf("设置");
      assert(iWeek >= 0 && iFriends >= 0 && iSet >= 0, `侧栏少了页：${JSON.stringify(order)}`);
      assert(iWeek < iFriends && iFriends < iSet, `好友的位置不对：${JSON.stringify(order)}`);
    });

    await s.clickText(".rail button", "好友");
    await sleep(1200);

    await scenario("没有 key：整页给引导卡，不是空列表", async () => {
      const v = await s.evalJs(`(() => {
        const page = document.querySelector('[data-page="friends"]');
        return { state: page.querySelector('[data-state]')?.dataset.state ?? '', text: page.innerText };
      })()`);
      assert(v.state === "no_key", `应该是 no_key，实际 ${JSON.stringify(v.state)}`);
      assert(v.text.includes("去申请 key"), "缺少申请 key 的入口");
      assert(v.text.includes("把 key 粘在这里") || v.text.includes("粘在下面"), "缺少粘贴框说明");
    });

    // 造一份「看得见/看不见」混在一起的好友缓存：甲 12600 分钟、乙 0 分钟、丙/丁 资料不公开
    const seedFriends = () => JSON.stringify({
      steamId: "76561199034598912",
      fetchedAt: Math.floor(Date.now() / 1000),
      friends: [
        { steamId: "76561198000000001", name: "甲矿工", avatar: "", visibility: 3, ownsGame: true, playtimeMin: 12600 },
        { steamId: "76561198000000002", name: "乙矿工", avatar: "", visibility: 3, ownsGame: false, playtimeMin: 0 },
        { steamId: "76561198000000003", name: "丙矿工", avatar: "", visibility: 1, ownsGame: null, playtimeMin: null },
        { steamId: "76561198000000004", name: "丁矿工", avatar: "", visibility: 2, ownsGame: null, playtimeMin: null },
      ],
    });
    const KEY = "0123456789abcdef0123456789abcdef";
    await s.invoke("set_steam", { key: KEY, id: null });
    writeFileSync(join(dir, "cache/steam-friends.json"), seedFriends());

    await s.clickText(".rail button", "今天");
    await sleep(400);
    await s.clickText(".rail button", "好友");
    await sleep(1500);

    await scenario("有缓存：列表按时长降序，没玩过的写「没玩过」，资料不公开的进折叠区", async () => {
      const v = await s.evalJs(`(() => {
        const page = document.querySelector('[data-page="friends"]');
        const visibleRows = [...(page.querySelector('.friend-list')?.querySelectorAll('.friend-row') ?? [])];
        const names = visibleRows.map((x) => x.querySelector('.friend-name').textContent.trim());
        const times = visibleRows.map((x) => x.querySelector('.friend-time').textContent.replace(/\\s+/g, ' ').trim());
        const hidden = page.querySelector('.friend-hidden');
        return {
          names, times,
          hiddenTitle: hidden?.querySelector("summary")?.textContent.replace(/\\s+/g, ' ').trim() ?? '',
          hiddenNames: [...(hidden?.querySelectorAll('.friend-name') ?? [])].map((x) => x.textContent.trim()),
          text: page.innerText,
        };
      })()`);
      assert(v.names.length === 2, `公开好友应该是 2 个，实际 ${JSON.stringify(v.names)}`);
      assert(v.names[0] === "甲矿工" && v.names[1] === "乙矿工", `排序不对：${JSON.stringify(v.names)}`);
      assert(/210 小时/.test(v.times[0]), `甲的时长不对：${v.times[0]}`);
      assert(v.times[1].includes("没玩过"), `乙应该写「没玩过」：${v.times[1]}`);
      assert(v.hiddenTitle.includes("未公开（2 人）"), `折叠区标题不对：${v.hiddenTitle}`);
      assert(v.hiddenNames.includes("丙矿工") && v.hiddenNames.includes("丁矿工"), `折叠区里缺人：${JSON.stringify(v.hiddenNames)}`);
      assert(v.text.includes("其中 2 位公开"), "顶部没写「其中 2 位公开」");
      assert(!v.text.includes(KEY), "页面上出现了完整的 key");
      assert(!v.text.includes("api.steampowered.com"), "页面上出现了接口地址（可能带 key）");
    });

    await scenario("缓存过期又连不上：退回上次结果，并说清是上次的", async () => {
      const stale = JSON.parse(seedFriends());
      stale.fetchedAt = Math.floor(Date.now() / 1000) - 3600;
      writeFileSync(join(dir, "cache/steam-friends.json"), JSON.stringify(stale));
      await s.invoke("set_steam", { key: KEY, id: null });
      const view = await s.invoke("get_friends", { force: true });
      const last = view.error ?? view.data;
      assert(last.state === "offline", `应该是 offline，实际 ${JSON.stringify(last.state)}`);
      assert(/上次刷新/.test(last.note ?? ""), `没说明是上次的结果：${JSON.stringify(last.note)}`);
      assert((last.visible ?? []).length === 2, "离线时应该仍然给出上次的列表");
    });

    await scenario("假 key：Steam 不接受时整页说「不被 Steam 接受」，不是空白", async () => {
      const online = await s.evalJs(`fetch("https://api.steampowered.com/ISteamUser/ResolveVanityURL/v1/?vanityurl=gaben")
        .then((r) => true).catch(() => false)`);
      writeFileSync(join(dir, "cache/steam-friends.json"), JSON.stringify({ steamId: "76561199034598912", fetchedAt: 0, friends: [] }));
      const view = await s.invoke("get_friends", { force: true });
      const last = view.error ?? view.data;
      if (online) {
        assert(last.state === "bad_key", `有网时假 key 应该是 bad_key，实际 ${JSON.stringify(last.state)}`);
      } else {
        assert(last.state === "offline" || last.state === "bad_key", `没网时应该退成 offline，实际 ${JSON.stringify(last.state)}`);
      }
      await s.clickText(".rail button", "今天");
      await sleep(400);
      await s.clickText(".rail button", "好友");
      await sleep(1200);
      const text = await s.evalJs(`document.querySelector('[data-page="friends"]').innerText`);
      assert(text.includes("Steam") && !text.includes(KEY), "好友页文字不对");
    });
  } finally {
    if (s) s.close();
    killApp();
    await sleep(600);
  }
}

const cleanup = clearGameDir();
console.log("\n== 汇总 ==");
for (const r of results) {  console.log(`  ${r.ok ? "PASS" : "FAIL"}  ${r.name}  ${r.ms}ms`);}
const failed = results.filter((r) => !r.ok);
console.log(`\n  ${results.length} 个场景，${results.length - failed.length} 通过，${failed.length} 失败`);
if (cleanup) console.log(`  收尾：${cleanup}`);
console.log(`  测试数据留在：${TESTDATA}`);
if (failed.length > 0) {  console.log("\n失败清单：");
  for (const f of failed) console.log(`  - [${f.phase}] ${f.name}\n    ${f.msg}`);
  process.exit(1);}
process.exit(0);