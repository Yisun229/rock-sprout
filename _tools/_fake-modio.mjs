// 本地假 mod.io：只够验「一键换新版」那条链路，不冒充真接口。
//
//   node _tools/_fake-modio.mjs                     起服务（默认 127.0.0.1:8899）
//   FAKE_PORT=9001 node _tools/_fake-modio.mjs      换端口
//   curl 'http://127.0.0.1:8899/control?mode=merge' 切换下一次下载的失败方式
//
// 为什么要有它：升级验的是我们自己的顺序（下载 → 校验 → 合并 → 做成了才写记录 → 删旧文件），
// 跟对面是不是真 mod.io 无关；真接口上「正好有个已装 mod 出了新版」不可控。
// 让应用连这里靠的是 modio.rs 里的 api_root()：只有开发构建（debug_assertions）
// 才读 ROCK_SPROUT_MODIO_BASE，release 包里那段代码不存在。
//
// 四种下载行为：
//   success   正常给完
//   truncate  给到一半掐断连接（模拟下到一半断）
//   size      声明的大小和实际字节数不一致（模拟校验不过）
//   merge     字节数对、内容是坏的（模拟解包/合并报错）
import { createServer } from "node:http";
import { readFileSync } from "node:fs";

const PORT = Number(process.env.FAKE_PORT ?? 8899);
const HOST = "127.0.0.1";
const PAYLOAD = process.env.FAKE_PAYLOAD ?? "_tools/fixtures/small-mod.zip";
const bytes = readFileSync(PAYLOAD);

const MOD_ID = 1034060;
const NEW_FILE_ID = 777_000_111;
const NEW_FILE_NAME = "fake-upgrade.zip";
const SIZE_LIE = 1234;

let mode = "success";

function modJson() {
  return {
    id: MOD_ID,
    name: "假服务器上的 mod",
    summary: "给一键升级用的假 mod。",
    description: "给一键升级用的假 mod。",
    description_plaintext: "给一键升级用的假 mod。",
    stats: { downloads_total: 42 },
    date_added: 1_700_000_000,
    date_updated: 1_700_100_000,
    logo: { thumb_320x180: "", thumb_640x360: "" },
    submitted_by: { username: "fake" },
    tags: [{ name: "QoL" }],
  };
}

function fileJson() {
  const declared =
    mode === "size" ? bytes.length + SIZE_LIE : mode === "merge" ? bytes.length : bytes.length;
  return {
    id: NEW_FILE_ID,
    filename: NEW_FILE_NAME,
    filesize: declared,
    filehash: { md5: "00000000000000000000000000000000" },
    download: { binary_url: `http://${HOST}:${PORT}/download/${NEW_FILE_ID}` },
  };
}

function envelope(data) {
  return JSON.stringify({ data, result_count: data.length, result_total: data.length });
}

function send(res, code, body, type = "application/json") {
  res.writeHead(code, { "content-type": type, "content-length": Buffer.byteLength(body) });
  res.end(body);
}

const server = createServer((req, res) => {
  const url = new URL(req.url ?? "/", `http://${HOST}:${PORT}`);
  const path = url.pathname;
  console.log(`[fake] ${req.method} ${req.url}`);

  if (path === "/control") {
    const next = url.searchParams.get("mode") ?? "success";
    if (!["success", "truncate", "size", "merge"].includes(next)) {
      return send(res, 400, JSON.stringify({ error: `不认识的方式：${next}` }));
    }
    mode = next;
    console.log(`[fake] 下载方式 → ${mode}`);
    return send(res, 200, JSON.stringify({ mode }));
  }

  if (path === "/v1/me") {
    return send(res, 200, JSON.stringify({ id: 1, username: "fake", username_id: 1 }));
  }

  if (path === `/v1/games/2475/mods`) {
    return send(res, 200, envelope([modJson()]));
  }

  if (path === `/v1/games/2475/mods/${MOD_ID}`) {
    return send(res, 200, JSON.stringify(modJson()));
  }

  if (path === `/v1/games/2475/mods/${MOD_ID}/files`) {
    return send(res, 200, envelope([fileJson()]));
  }

  if (path.startsWith("/download/")) {
    if (mode === "truncate") {
      const half = Math.floor(bytes.length / 2);
      res.writeHead(200, { "content-type": "application/zip", "content-length": bytes.length });
      res.write(bytes.subarray(0, half));
      console.log(`[fake] 下到一半掐断（给了 ${half}/${bytes.length} 字节）`);
      setTimeout(() => res.destroy(), 80);
      return;
    }
    if (mode === "merge") {
      const broken = Buffer.alloc(bytes.length, 0);
      broken.write("not a real zip, this is garbage padding", 0, "utf8");
      res.writeHead(200, { "content-type": "application/zip", "content-length": broken.length });
      res.end(broken);
      console.log(`[fake] 给了 ${broken.length} 字节坏内容`);
      return;
    }
    res.writeHead(200, { "content-type": "application/zip", "content-length": bytes.length });
    res.end(bytes);
    console.log(`[fake] 正常给完 ${bytes.length} 字节`);
    return;
  }

  send(res, 404, JSON.stringify({ error: `没这个路径：${path}` }));
});

server.listen(PORT, HOST, () => {
  console.log(`假 mod.io 起在 http://${HOST}:${PORT}（载荷 ${PAYLOAD}，${bytes.length} 字节）`);
});
