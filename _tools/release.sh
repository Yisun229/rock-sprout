#!/usr/bin/env bash
# 出包脚本：先跑门禁，再打包。门禁红了就 exit 1，包根本不会出来。
#
#   bash _tools/release.sh                # 只出包（门禁 + 构建 + 便携包 + 校验和）
#   bash _tools/release.sh --publish      # 出完包接着发 Release（需要 github 令牌）
#   bash _tools/release.sh --publish-only # 包已经出好了，只发 Release
#
# 门禁为什么放最前面：纯原版往返这条只有在“游戏目录里没有别人的东西”时才跑得起来，
# 所以它不能是默认回归路径；但它又必须每个版本都跑一次——放这里，跑不跑不靠人记得。
set -euo pipefail
cd "$(dirname "$0")/.."

PUBLISH=0
ONLY=0
case "${1:-}" in
  --publish) PUBLISH=1 ;;
  --publish-only) PUBLISH=1; ONLY=1 ;;
esac

VER=$(node -p "require('./app/package.json').version")
OUT="_tools/release-renamed"
EXE="$OUT/RockSprout_${VER}_x64-setup.exe"
ZIP="$OUT/RockSprout_${VER}_portable_win64.zip"
SUMS="$OUT/SHA256SUMS.txt"
echo "== 版本 $VER =="

if [ "$ONLY" = "1" ]; then
  echo "== 跳过门禁与构建（--publish-only）=="
  for f in "$EXE" "$ZIP" "$SUMS"; do
    [ -f "$f" ] || { echo "缺文件：$f（先不带参数跑一遍出包）" >&2; exit 1; }
  done
else
  # ---- 1. 门禁：回归 + 纯原版往返 ----
  echo "== 门禁：回归（含 --with-stock 纯原版往返）=="
  node _tools/check-all.mjs --with-stock || { echo "门禁没过，不出包" >&2; exit 1; }

  # ---- 2. 构建 ----
  echo "== 构建（release）=="
  taskkill //IM rock-sprout.exe //F >/dev/null 2>&1 || true
  ( cd app && node node_modules/@tauri-apps/cli/tauri.js build )

  SETUP="_tools/target/release/bundle/nsis/RockSprout_${VER}_x64-setup.exe"
  [ -f "$SETUP" ] || { echo "找不到安装包（$VER）" >&2; exit 1; }

  # ---- 3. 便携包 ----
  echo "== 便携包 =="
  bash _tools/make-portable.sh

  mkdir -p "$OUT"
  cp -f "$SETUP" "$EXE"

  # ---- 4. 校验和（每版只留这两行）----
  echo "== 校验和 =="
  ( cd "$OUT" && sha256sum "$(basename "$EXE")" "$(basename "$ZIP")" > SHA256SUMS.txt )
fi

cat "$SUMS"
ls -l "$EXE" "$ZIP" "$SUMS" | awk '{print $5, $9}'

# ---- 5. 发布（可选）----
if [ "$PUBLISH" = "1" ]; then
  TOKEN=$(printf 'protocol=https\nhost=github.com\n\n' | git credential fill | sed -n 's/^password=//p')
  [ -n "$TOKEN" ] || { echo "拿不到 github 令牌" >&2; exit 1; }
  REPO="Yisun229/rock-sprout"
  TAG="v$VER"
  echo "== 建 Release $TAG =="
  # 请求体必须先写文件再发：Git Bash 把中文参数传给原生 curl 时会按系统代码页转码，
  # 中文会被压成单字节，GitHub 收到非法 UTF-8 直接回 400 "Problems parsing JSON"。
  BODYFILE="_tools/.release-body.json"
  node _tools/_mkbody.mjs "$TAG" "_tools/release-v$VER.md" "$BODYFILE" || { echo "生成请求体失败" >&2; exit 1; }
  CODE=$(curl --noproxy '*' -sS -o /tmp/rel.json -w '%{http_code}' -X POST \
    -H "Authorization: Bearer $TOKEN" \
    -H "Accept: application/vnd.github+json" \
    -H "Content-Type: application/json" \
    --data-binary "@$BODYFILE" "https://api.github.com/repos/$REPO/releases")
  rm -f "$BODYFILE"
  if [ "$CODE" != "201" ]; then
    echo "建 Release 失败：HTTP $CODE" >&2
    head -c 400 /tmp/rel.json >&2; echo >&2
    exit 1
  fi
  ID=$(node -p 'JSON.parse(require("fs").readFileSync("/tmp/rel.json","utf8")).id' 2>/dev/null || true)
  case "$ID" in
    ''|*[!0-9]*) echo "建 Release 返回的 id 不正常：$ID" >&2; exit 1 ;;
  esac
  echo "release id: $ID"
  for f in "$EXE" "$ZIP" "$SUMS"; do
    NAME=$(basename "$f")
    echo "上传 $NAME"
    UCODE=$(curl --noproxy '*' -sS -o /dev/null -w '%{http_code}' -X POST \
      -H "Authorization: Bearer $TOKEN" \
      -H "Content-Type: application/octet-stream" \
      --data-binary "@$f" \
      "https://uploads.github.com/repos/$REPO/releases/$ID/assets?name=$NAME")
    if [ "$UCODE" != "201" ]; then
      echo "上传 $NAME 失败：HTTP $UCODE" >&2
      exit 1
    fi
  done
  N=$(curl --noproxy '*' -sS -H "Authorization: Bearer $TOKEN" \
        "https://api.github.com/repos/$REPO/releases/$ID" \
      | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const j=JSON.parse(s);console.log((j.assets||[]).length)})')
  if [ "$N" != "3" ]; then
    echo "附件数量不对：应为 3，实际 $N" >&2
    exit 1
  fi
  echo "发布完成（3 个附件已核对）：https://github.com/$REPO/releases/tag/$TAG"
fi
