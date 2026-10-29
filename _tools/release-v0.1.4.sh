#!/usr/bin/env bash
# 发 v0.1.4 的 Release：正文取 CHANGELOG.md 里 0.1.4 那一节 → 建发布 → 传安装包/便携包/SHA256SUMS → 逐个下载回来重算 sha256。
# 令牌从 git 的凭据库取（不打印、不落盘）。这台机器走代理不通，curl 一律 --noproxy '*'。
set -euo pipefail
cd "$(dirname "$0")/.."

OWNER=Yisun229
REPO=rock-sprout
TAG=v0.1.4
EXE="_tools/target/release/bundle/nsis/RockSprout_0.1.4_x64-setup.exe"
ZIP="_tools/release-renamed/RockSprout_0.1.4_portable_win64.zip"
SUMS="_tools/release-renamed/SHA256SUMS.txt"
TMP="_tools/_tmp"
mkdir -p "$TMP"

TOKEN=$(printf 'protocol=https\nhost=github.com\n\n' | git credential fill | sed -n 's/^password=//p')
if [ -z "$TOKEN" ]; then
  echo "没拿到令牌，先在自己终端里 git push 一次完成授权" >&2
  exit 1
fi

# 从 CHANGELOG.md 抽出 0.1.4 那一节当正文（去掉它自己的小节标题，标题走 name 字段）
node -e '
const fs = require("fs");
const lines = fs.readFileSync("CHANGELOG.md", "utf8").split(/\r?\n/);
const start = lines.findIndex((l) => l.startsWith("## [0.1.4]"));
if (start < 0) { console.error("CHANGELOG.md 里找不到 ## [0.1.4] 这一节"); process.exit(1); }
let end = lines.findIndex((l, i) => i > start && l.startsWith("## ["));
if (end < 0) end = lines.length;
let body = lines.slice(start + 1, end).join("\n").trim();
body = body.replace(/\n---\s*$/, "").trim();   // 去掉小节之间的分隔线
fs.writeFileSync(process.argv[1], JSON.stringify({
  tag_name: process.argv[2],
  name: "v0.1.4 好看",
  body,
  draft: false,
  prerelease: false,
}));
console.log("正文长度:", body.length);
' "$TMP/rel.json" "$TAG"

echo "== 建 Release =="
RESP=$(curl -sS --noproxy '*' -X POST \
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
  curl -sS --noproxy '*' -X POST \
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

echo "== 下载回来重算 sha256 =="
curl -sS --noproxy '*' -H "Authorization: Bearer $TOKEN" -H "Accept: application/vnd.github+json" \
  "https://api.github.com/repos/$OWNER/$REPO/releases/$RID/assets?per_page=20" > "$TMP/assets.json"

node -e '
const fs = require("fs");
const list = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
fs.writeFileSync(process.argv[2], list.map((a) => `${a.name}\t${a.browser_download_url}`).join("\n") + "\n");
' "$TMP/assets.json" "$TMP/urls.tsv"

DL="$TMP/dl"
rm -rf "$DL"; mkdir -p "$DL"
BAD=0
while IFS=$'\t' read -r name url; do
  [ -z "$name" ] && continue
  echo "-- 下 $name"
  curl -sS -L --noproxy '*' --max-time 1800 -o "$DL/$name" "$url"
  got=$(sha256sum "$DL/$name" | cut -d' ' -f1)
  case "$name" in
    *setup.exe) want=$(sha256sum "$EXE" | cut -d' ' -f1) ;;
    *portable_win64.zip) want=$(sha256sum "$ZIP" | cut -d' ' -f1) ;;
    *) want=$(sha256sum "$SUMS" | cut -d' ' -f1) ;;
  esac
  if [ "$got" = "$want" ]; then
    echo "   一致 ✔  $name  $(stat -c%s "$DL/$name") 字节  $got"
  else
    echo "   不一致 ✘  $name  本地 $want  下载 $got"
    BAD=1
  fi
done < "$TMP/urls.tsv"

rm -rf "$DL" "$TMP"
if [ "$BAD" -ne 0 ]; then echo "核对失败，别往下走" >&2; exit 1; fi
echo "完成：https://github.com/$OWNER/$REPO/releases/tag/$TAG"
