#!/usr/bin/env bash
# 发 v0.1.3 的 Release：建发布 → 传安装包 → 传便携包 → 传 SHA256SUMS → 下载回来核对 sha256。
# 令牌从 git 的凭据库取（不打印、不落盘）。
set -euo pipefail
cd "$(dirname "$0")/.."

OWNER=Yisun229
REPO=rock-sprout
TAG=v0.1.3
EXE="_tools/target/release/bundle/nsis/RockSprout_0.1.3_x64-setup.exe"
ZIP="_tools/release-renamed/RockSprout_0.1.3_portable_win64.zip"
SUMS="_tools/release-renamed/SHA256SUMS.txt"
NOTES="_tools/release-v0.1.3.md"
TMP="_tools/_tmp"
mkdir -p "$TMP"

TOKEN=$(printf 'protocol=https\nhost=github.com\n\n' | git credential fill | sed -n 's/^password=//p')
if [ -z "$TOKEN" ]; then
  echo "没拿到令牌，先在自己终端里 git push 一次完成授权" >&2
  exit 1
fi

node -e '
const fs = require("fs");
const body = fs.readFileSync(process.argv[1], "utf8");
fs.writeFileSync(process.argv[2], JSON.stringify({
  tag_name: "v0.1.3",
  name: "v0.1.3 · 轻便",
  body,
  draft: false,
  prerelease: false,
}));
' "$NOTES" "$TMP/rel.json"

echo "== 建 Release =="
RESP=$(curl -sS -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Accept: application/vnd.github+json" \
  -H "Content-Type: application/json" \
  --data-binary "@$TMP/rel.json" \
  "https://api.github.com/repos/$OWNER/$REPO/releases")
echo "$RESP" | node -e '
let s=""; process.stdin.on("data",d=>s+=d).on("end",()=>{
  const j=JSON.parse(s);
  if(j.id){ console.log("release id:", j.id); console.log("页面:", j.html_url); }
  else { console.error("失败:", JSON.stringify(j).slice(0,400)); process.exit(1); }
});' | tee "$TMP/rel.out"

RID=$(sed -n 's/^release id: //p' "$TMP/rel.out")

upload() {
  local file="$1"
  echo "== 传 $(basename "$file") =="
  curl -sS -X POST \
    -H "Authorization: Bearer $TOKEN" \
    -H "Accept: application/vnd.github+json" \
    -H "Content-Type: application/octet-stream" \
    --data-binary "@$file" \
    "https://uploads.github.com/repos/$OWNER/$REPO/releases/$RID/assets?name=$(basename "$file")" \
    | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const j=JSON.parse(s);console.log(j.state?`ok ${j.name} ${j.size} 字节 digest=${j.digest||"-"}`:`失败 ${JSON.stringify(j).slice(0,200)}`);});'
}

upload "$EXE"
upload "$ZIP"
upload "$SUMS"

echo "== 核对服务端 digest（这台机器下载整包很慢，改用 GitHub 算好的指纹比）=="
EXE_SUM=$(sha256sum "$EXE" | cut -d' ' -f1)
ZIP_SUM=$(sha256sum "$ZIP" | cut -d' ' -f1)
SUMS_SUM=$(sha256sum "$SUMS" | cut -d' ' -f1)
printf '{"%s":"%s","%s":"%s","%s":"%s"}\n' \
  "$(basename "$EXE")" "$EXE_SUM" \
  "$(basename "$ZIP")" "$ZIP_SUM" \
  "$(basename "$SUMS")" "$SUMS_SUM" > "$TMP/want.json"
curl -sS -H "Authorization: Bearer $TOKEN" -H "Accept: application/vnd.github+json" \
  "https://api.github.com/repos/$OWNER/$REPO/releases/$RID/assets?per_page=20" > "$TMP/assets.json"
node -e '
const fs = require("fs");
const list = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
const want = JSON.parse(fs.readFileSync(process.argv[2], "utf8")); // name -> sha256
let bad = 0;
for (const a of list) {
  const local = want[a.name];
  const digest = (a.digest || "").replace(/^sha256:/, "");
  const ok = local && local === digest;
  console.log(`${ok ? "一致 ✔" : "不一致 ✘"}  ${a.name}  ${a.size} 字节  ${digest || "(无 digest)"}`);
  if (!ok) bad = 1;
}
for (const name of Object.keys(want)) if (!list.some(a => a.name === name)) { console.log(`缺附件 ✘  ${name}`); bad = 1; }
process.exit(bad);
' "$TMP/assets.json" "$TMP/want.json"
SHA256SUMS_FILE_SUM=$(sha256sum "$SUMS" | cut -d' ' -f1)
echo "SHA256SUMS.txt 本地 sha256 = $SHA256SUMS_FILE_SUM"

rm -rf "$TMP"
echo "完成：https://github.com/$OWNER/$REPO/releases/tag/$TAG"
