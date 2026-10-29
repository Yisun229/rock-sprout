#!/usr/bin/env bash
# 一次性：在 GitHub 上建 v0.1.1 的 Release，附安装包与 SHA256SUMS，再从 GitHub 下载回来核对哈希。
# 令牌从系统凭据库取（git credential fill），不打印、不写盘。
set -u

REPO="Yisun229/rock-sprout"
TAG="v0.1.1"
BUNDLE="_tools/target/release/bundle/nsis"
EXE="RockSprout_0.1.1_x64-setup.exe"
SUMS="SHA256SUMS.txt"
PAYLOAD="_tools/_tmp/release-v0.1.1.json"

token=$(printf 'protocol=https\nhost=github.com\n\n' | git credential fill | sed -n 's/^password=//p')
if [ -z "$token" ]; then
  echo "拿不到令牌，停" >&2
  exit 1
fi

echo "== 建 Release =="
resp=$(curl -sS -X POST \
  -H "Authorization: Bearer $token" \
  -H "Accept: application/vnd.github+json" \
  -H "Content-Type: application/json" \
  --data-binary "@$PAYLOAD" \
  "https://api.github.com/repos/$REPO/releases")

rel_id=$(printf '%s' "$resp" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const j=JSON.parse(s);console.log(j.id??"");})')
html=$(printf '%s' "$resp" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const j=JSON.parse(s);console.log(j.html_url??"");})')
if [ -z "$rel_id" ]; then
  echo "建 Release 失败，返回：" >&2
  printf '%s\n' "$resp" | head -20 >&2
  exit 1
fi
echo "release id=$rel_id  $html"

upload() {
  local file="$1"
  local name
  name=$(basename "$file")
  echo "== 上传 $name =="
  local out
  out=$(curl -sS -X POST \
    -H "Authorization: Bearer $token" \
    -H "Accept: application/vnd.github+json" \
    -H "Content-Type: application/octet-stream" \
    --data-binary "@$file" \
    "https://uploads.github.com/repos/$REPO/releases/$rel_id/assets?name=$name")
  printf '%s' "$out" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const j=JSON.parse(s);console.log(j.state?("ok size="+j.size):("FAILED: "+s.slice(0,300)));})'
}

upload "$BUNDLE/$EXE"
upload "$BUNDLE/$SUMS"

echo "== 从 GitHub 下载回来核对哈希 =="
mkdir -p _tools/_tmp/roundtrip
curl -sSL -o "_tools/_tmp/roundtrip/$EXE" \
  "https://github.com/$REPO/releases/download/$TAG/$EXE"
local_sum=$(sha256sum "$BUNDLE/$EXE" | awk '{print $1}')
remote_sum=$(sha256sum "_tools/_tmp/roundtrip/$EXE" | awk '{print $1}')
echo "本地 $local_sum"
echo "下载 $remote_sum"
if [ "$local_sum" = "$remote_sum" ]; then
  echo "哈希一致 ✔"
else
  echo "哈希不一致 ✘" >&2
  exit 1
fi

printf '%s\n' "$html" > _tools/_tmp/release-url.txt
