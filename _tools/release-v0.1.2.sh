#!/usr/bin/env bash
# 发 v0.1.2 的 Release：建发布 → 传安装包 → 传 SHA256SUMS → 下载回来核对 sha256。
# 令牌从 git 的凭据库取（不打印、不落盘）。
set -euo pipefail
cd "$(dirname "$0")/.."

OWNER=Yisun229
REPO=rock-sprout
TAG=v0.1.2
EXE="_tools/target/release/bundle/nsis/RockSprout_0.1.2_x64-setup.exe"
SUMS="_tools/target/release/bundle/nsis/SHA256SUMS.txt"
NOTES="_tools/release-v0.1.2.md"
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
  tag_name: "v0.1.2",
  name: "v0.1.2 · 敢用",
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

echo "== 传安装包 =="
curl -sS -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Accept: application/vnd.github+json" \
  -H "Content-Type: application/octet-stream" \
  --data-binary "@$EXE" \
  "https://uploads.github.com/repos/$OWNER/$REPO/releases/$RID/assets?name=$(basename "$EXE")" \
  | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const j=JSON.parse(s);console.log(j.state?`ok ${j.name} ${j.size} 字节`:`失败 ${JSON.stringify(j).slice(0,200)}`);});'

echo "== 传校验文件 =="
curl -sS -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Accept: application/vnd.github+json" \
  -H "Content-Type: application/octet-stream" \
  --data-binary "@$SUMS" \
  "https://uploads.github.com/repos/$OWNER/$REPO/releases/$RID/assets?name=$(basename "$SUMS")" \
  | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const j=JSON.parse(s);console.log(j.state?`ok ${j.name} ${j.size} 字节`:`失败 ${JSON.stringify(j).slice(0,200)}`);});'

echo "== 下载回来核对 =="
URL="https://github.com/$OWNER/$REPO/releases/download/$TAG/$(basename "$EXE")"
curl -sSL -o "$TMP/roundtrip.exe" "$URL"
LOCAL=$(sha256sum "$EXE" | cut -d' ' -f1)
REMOTE=$(sha256sum "$TMP/roundtrip.exe" | cut -d' ' -f1)
echo "本地  $LOCAL"
echo "下载  $REMOTE"
[ "$LOCAL" = "$REMOTE" ] && echo "一致 ✔" || { echo "不一致 ✘" >&2; exit 1; }

rm -rf "$TMP"
echo "完成：https://github.com/$OWNER/$REPO/releases/tag/$TAG"
