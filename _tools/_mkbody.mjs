import fs from 'node:fs';

const [tag, mdPath, out] = process.argv.slice(2);
if (!tag || !mdPath || !out) {
  console.error('用法: node _mkbody.mjs <tag> <发布说明.md> <输出.json>');
  process.exit(1);
}

const payload = JSON.stringify({
  tag_name: tag,
  name: `岩笋 · RockSprout ${tag}`,
  body: fs.readFileSync(mdPath, 'utf8'),
  draft: process.env.RELEASE_DRAFT === '1',
  prerelease: false,
});

fs.writeFileSync(out, payload, 'utf8');
console.log(`写出 ${out}：${Buffer.byteLength(payload, 'utf8')} 字节`);
