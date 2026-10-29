#!/usr/bin/env bash
# 打便携包：把编译好的 exe 变成一个「解压就能用」的 zip。
#
#   bash _tools/make-portable.sh
#
# 做的事：取 _tools/target/release/rock-sprout.exe → 摆成一个干净的小文件夹
# （程序 + 使用说明 + 空的 data\）→ 压成 zip → 算 sha256 记进 SHA256SUMS.txt。
# 产物：_tools/release-renamed/RockSprout_<版本>_portable_win64.zip
set -euo pipefail
cd "$(dirname "$0")/.."

VER=$(node -p "require('./app/package.json').version")
SRC="_tools/target/release/rock-sprout.exe"
STAGE="_tools/_portable"
OUTDIR="_tools/release-renamed"
ZIP="$OUTDIR/RockSprout_${VER}_portable_win64.zip"

if [ ! -f "$SRC" ]; then
  echo "没有 $SRC。先编译：cd app && node node_modules/@tauri-apps/cli/tauri.js build --no-bundle" >&2
  exit 1
fi

rm -rf "$STAGE"
mkdir -p "$STAGE/data"
cp "$SRC" "$STAGE/RockSprout.exe"
: > "$STAGE/data/.keep"

cat > "$STAGE/使用说明.txt" <<'TXT'
岩笋 · Rock Sprout（便携版）

1. 解压到任意你能写的地方（比如 D:\游戏工具\岩笋），双击 RockSprout.exe 就能用，不用安装。
2. 配置、缓存、令牌都在这个文件夹里的 data\ 下；整个文件夹拷到 U 盘或另一台电脑，就是换机搬家。
3. 别放进 C:\Program Files 这类不让普通用户随便写的地方——那样数据会自动改存到你的用户目录。
4. 如果提示缺 WebView2，按弹窗里给的链接装一次就好，装完再双击本程序。
TXT

mkdir -p "$OUTDIR"
powershell -NoProfile -Command "Add-Type -A System.IO.Compression.FileSystem; [IO.Compression.ZipFile]::CreateFromDirectory('$(cygpath -w "$STAGE")','$(cygpath -w "$ZIP")')"

HASH=$(sha256sum "$ZIP" | cut -d' ' -f1)
SUMS="$OUTDIR/SHA256SUMS.txt"
if [ -f "$SUMS" ]; then
  grep -v " \*RockSprout_${VER}_portable_win64.zip$" "$SUMS" > "$SUMS.tmp" || true
  mv "$SUMS.tmp" "$SUMS"
fi
printf '%s *RockSprout_%s_portable_win64.zip\n' "$HASH" "$VER" >> "$SUMS"

SIZE=$(stat -c %s "$ZIP")
echo "打好了：$ZIP"
echo "  大小 $SIZE 字节（$(awk "BEGIN{printf \"%.2f\", $SIZE/1048576}") MiB）"
echo "  sha256 $HASH"
echo "  记进了 $SUMS"
