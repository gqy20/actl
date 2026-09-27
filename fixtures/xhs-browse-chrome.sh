#!/usr/bin/env bash
# 场景实测(轨 2,不进 CI):Chrome × 小红书 —— pack "Chrome 同源可直接套用"
# 断言的验证(对照 docs/16 的 Edge 数据)。
# 闭环:唯一标题起始页 → 地址栏导航 → 文档枚举(验证 Chrome 的 feed 序数)→
# settle → 读 feed → 滚动×2 → 四档投影。零写入操作。
set -uo pipefail
cd "$(dirname "$0")/.."
ACTL="${ACTL:-target/release/actl.exe}"
CHROME="/c/Program Files/Google/Chrome/Application/chrome.exe"
APP="小红书"
OUT=/tmp/xhs_chrome
mkdir -p "$OUT"
STAMP=$(date +%H%M%S)
NAV="actl-chrome-nav-$STAMP"
NAV_HTML="$LOCALAPPDATA\\Temp\\$NAV.html"
NAV_UX="${LOCALAPPDATA//\\//}/Temp/$NAV.html"

# 预清理:残留的 Chrome 小红书窗/导航起始页(不动 Edge)
$ACTL list-windows 2>/dev/null | sed 's/},{/\n/g' | grep -E 'Google Chrome' | grep -E "$APP|actl-chrome-nav" | grep -o '"hwnd":[0-9]*' | grep -o '[0-9]*' | while read H; do
  [ -n "$H" ] && $ACTL close-window --hwnd "$H" >/dev/null 2>&1
done
# Edge 侧若有小红书窗先关(避免 --app 小红书 跨浏览器歧义)
$ACTL list-windows 2>/dev/null | sed 's/},{/\n/g' | grep 'Microsoft' | grep "$APP" | grep -o '"hwnd":[0-9]*' | grep -o '[0-9]*' | while read H; do
  [ -n "$H" ] && $ACTL close-window --hwnd "$H" >/dev/null 2>&1
done

echo "== 1. 唯一起始页 + 地址栏导航 =="
printf '<!DOCTYPE html><html><head><title>%s</title></head><body>nav stub</body></html>' "$NAV" > "$NAV_UX"
cleanup() {
  $ACTL list-windows 2>/dev/null | sed 's/},{/\n/g' | grep -E "$NAV|$APP.*Google Chrome|Google Chrome.*$APP" | grep -o '"hwnd":[0-9]*' | grep -o '[0-9]*' | while read H; do
    [ -n "$H" ] && $ACTL close-window --hwnd "$H" >/dev/null 2>&1 && echo "   cleanup: closed hwnd $H"
  done
  rm -f "$NAV_UX"
}
trap cleanup EXIT
"$CHROME" --new-window "file:///$NAV_UX" >/dev/null 2>&1 &
$ACTL wait --window "$NAV" --timeout 15000 2>&1 | grep -o '"ok":[a-z]*' | sed 's/^/   stub window: /'
$ACTL focus-window "$NAV" >/dev/null 2>&1
$ACTL press ctrl+l --app "$NAV" 2>&1 | grep -o '"ok":[a-z]*' | sed 's/^/   ctrl+l: /'
$ACTL set-clipboard "https://www.xiaohongshu.com" >/dev/null 2>&1
$ACTL paste --app "$NAV" 2>&1 | grep -o '"ok":[a-z]*' | sed 's/^/   paste: /'
$ACTL press enter --app "$NAV" 2>&1 | grep -o '"ok":[a-z]*' | sed 's/^/   enter: /'
$ACTL wait --window "$APP" --timeout 15000 2>&1 | grep -o '"ok":[a-z]*' | sed 's/^/   window became xhs: /'

echo "== 2. 文档枚举(Chrome 的 feed 序数验证) =="
sleep 3
$ACTL find --role document --app "$APP" --limit 10 > "$OUT/docs.json" 2>&1
sed 's/},{/\n/g' "$OUT/docs.json" | grep -o '"automation_id":"[^"]*"' | head -8 | sed 's/^/   /'
for N in 1 2 3 4; do
  L=$($ACTL get "role:Document#$N" --app "$APP" --property text 2>/dev/null | wc -c)
  echo "   Document#$N text envelope: ${L}B"
done

echo "== 3. settle + 读 feed(以枚举出的正主为准,预期 #2) =="
DOC="role:Document#2"
$ACTL wait --element "$DOC" --stable --app "$APP" --timeout 25000 > "$OUT/settle1.json" 2>&1
grep -o '"waited":{[^}]*}' "$OUT/settle1.json" | sed 's/^/   /'
$ACTL get "$DOC" --app "$APP" --property text > "$OUT/text1.json" 2>&1
grep -q '"ok":true' "$OUT/text1.json" && echo "   text1: $(wc -c < "$OUT/text1.json")B envelope" || { echo "   FAILED"; head -c 300 "$OUT/text1.json"; }

echo "== 4. 滚动循环 x2 =="
$ACTL focus-window "$APP" >/dev/null 2>&1
for i in 1 2; do
  $ACTL press pgdn --app "$APP" >/dev/null 2>&1
  S=$($ACTL wait --element "$DOC" --stable --app "$APP" --timeout 20000 2>&1 | grep -o '"waited":{[^}]*}')
  $ACTL get "$DOC" --app "$APP" --property text > "$OUT/text_s$i.json" 2>&1
  echo "   scroll#$i: $S text=$(wc -c < "$OUT/text_s$i.json")B"
done

echo "== 5. 四档投影(chrome 窗口内) =="
for k in full skel content both; do
  case $k in
    full) ARGS=() ;;
    skel) ARGS=(--skeleton) ;;
    content) ARGS=(--view content) ;;
    both) ARGS=(--view content --skeleton) ;;
  esac
  $ACTL snapshot --app "$APP" "${ARGS[@]}" > "$OUT/snap_$k.json" 2>/dev/null
  echo "   $k: $(wc -c < "$OUT/snap_$k.json")B / $(grep -o '"role"' "$OUT/snap_$k.json" | wc -l) el"
done
echo "== done. artifacts in $OUT/ =="
