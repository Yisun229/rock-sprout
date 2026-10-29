// 测试小工具：往数据目录里写一份「搜索结果缓存」。
//
// 用途：断网时后端会回放缓存，这样不用连 mod.io 也能在界面里看到一屏 mod。
// 文件名规则必须跟 `app/src-tauri/src/cache.rs` 的 `search_file_name` 一致：
//   字母/数字/`-`/`_` 原样留，其它字符换 `_`，最多 32 个字符；哈希是 FNV-1a 32 位（8 位小写十六进制）。
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

function fnv1a32(text) {
  let hash = 0x811c9dc5;
  for (const byte of Buffer.from(text, "utf8")) {
    hash ^= byte;
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash.toString(16).padStart(8, "0");
}

function safePart(query) {
  const trimmed = query.trim();
  if (trimmed === "") return "热门";
  // 后端用的是 Rust 的 char::is_alphanumeric：汉字算字母，所以中文会原样留下
  const kept = Array.from(trimmed)
    .map((ch) => (/[\p{L}\p{N}_-]/u.test(ch) ? ch : "_"))
    .slice(0, 32)
    .join("");
  return kept === "" ? "热门" : kept;
}

/**
 * 写一份搜索缓存。
 * @param {string} dataDir 数据目录（跟 ROCK_SPROUT_DATA 一致）
 * @param {string} query 搜索关键词
 * @param {number} count 造几条
 * @returns {string} 写出的缓存文件路径
 */
export function writeSearchCache(dataDir, query, count = 30) {
  const items = Array.from({ length: count }, (_, i) => ({
    id: 800000 + i,
    name: `缓存测试 mod ${i + 1}`,
    summary: "这是一段说明文字，用来模拟真实的卡片内容",
    downloads: 50000 - i * 37,
  }));
  const dir = join(dataDir, "cache/search");
  mkdirSync(dir, { recursive: true });
  const path = join(dir, `${safePart(query)}-${fnv1a32(query.trim())}-p1.json`);
  writeFileSync(
    path,
    JSON.stringify({
      cached_at: Date.now(),
      value: { query, page: 1, total: count, items, stale: false, cached_at: null },
    }),
    "utf8",
  );
  return path;
}
