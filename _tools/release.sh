#!/usr/bin/env bash
# 出包脚本：先跑门禁，再打包。门禁红了就 exit 1，包根本不会出来。
#
#   bash _tools/release.sh            # 只出包（门禁 + 构建 + 便携包 + 校验和）
#   bash _tools/release.sh --publish  # 出完包接着发 Release（需要 github 令牌）
#
# 门禁为什么放最前面：纯原版往返这条只有在“游戏目录里没有别人的东西”时才跑得起来，
# 所以它不能是默认回归路径；但它又必须每个版本都跑一次——放这里，跑不跑不靠人记得。
set -euo pipefail
cd "$(dirname "$0")/.."

PUBLISH=0
[ "${1:-}" = "--publish" ] && PUBLISH=1

VER=$(node -p "require('./app/package.json').version")
echo "== 版本 $VER =="

# ---- 1. 门禁：回归 + 纯原版往返 ----
echo "== 门禁：回归（含 --with-stock 纯原版往返）=="
node _tools/check-all.mjs --with-stock || { echo "门禁没过，不出包" >&2; exit 1; }

# ---- 2. 构建 ----
echo "== 构建（release）=="
taskkill //IM rock-sprout.exe //F >/dev/null 2>&1 || true
( cd app && node node_modules/@tauri-apps/cli/tauri.js build )

SETUP="app/src-tauri/target/release/bundle/nsis/RockSprout_${VER}_x64-setup.exe"
[ -f "$SETUP" ] || SETUP="_tools/target/release/bundle/nsis/RockSprout_${VER}_x64-setup.exe"
[ -f "$SETUP" ] || { echo "找不到安装包（$VER）" >&2; exit 1; }

# ---- 3. 便携包 ----
echo "== 便携包 =="
bash _tools/make-portable.sh

OUT="_tools/release-renamed"
ZIP="$OUT/RockSprout_${VER}_portable_win64.zip"
EXE="$OUT/RockSprout_${VER}_x64-setup.exe"
mkdir -p "$OUT"
cp -f "$SETUP" "$EXE"

# ---- 4. 校验和（每版只留这两行）----
echo "== 校验和 =="
( cd "$OUT" && sha256sum "RockSprout_${VER}_x64-setup.exe" "RockSprout_${VER}_portable_win64.zip" > SHA256SUMS.txt )
cat "$OUT/SHA256SUMS.txt"
ls -l "$EXE" "$ZIP" "$OUT/SHA256SUMS.txt" | awk '{print $5, $9}'

# ---- 5. 发布（可选）----
if [ "$PUBLISH" = "1" ]; then
  TOKEN=$(printf 'protocol=https\nhost=github.com\n\n' | git credential fill | sed -n 's/^password=//p')
  [ -n "$TOKEN" ] || { echo "拿不到 github 令牌" >&2; exit 1; }
  REPO="Yisun229/rock-sprout"
  TAG="v$VER"
  echo "== 建 Release $TAG =="
  BODY=$(node -e 'const fs=require("fs");console.log(JSON.stringify({tag_name:process.argv[1],name:process.argv[1],body:fs.readFileSync(process.argv[2],"utf8"),draft:false,prerelease:false}))' "$TAG" "_tools/release-v$VER.md")
  curl --noproxy '*' -sS -X POST \
    -H "Authorization: Bearer $TOKEN" \
    -H "Accept: application/vnd.github+json" \
    -H "Content-Type: application/json" \
    -d "$BODY" "https://api.github.com/repos/$REPO/releases" > /tmp/rel.json
  ID=$(node -p 'JSON.parse(require("fs").readFileSync("/tmp/rel.json","utf8")).id')
  echo "release id: $ID"
  for f in "$EXE" "$ZIP" "$OUT/SHA256SUMS.txt"; do
    NAME=$(basename "$f")
    echo "上传 $NAME"
    curl --noproxy '*' -sS -X POST \
      -H "Authorization: Bearer $TOKEN" \
      -H "Content-Type: application/octet-stream" \
      --data-binary "@$f" \
      "https://uploads.github.com/repos/$REPO/releases/$ID/assets?name=$NAME" > /dev/null
  done
  echo "发布完成：https://github.com/$REPO/releases/tag/$TAG"
fi
