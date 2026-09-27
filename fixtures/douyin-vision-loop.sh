#!/usr/bin/env bash
# 视觉兜底闭环演示:抖音评论栏(screenshot → VLM 坐标 → click xy)。
# 第一段:开窗 + 读屏标签 + 截图 + 输出窗口 rect(坐标换算基准)。
set -uo pipefail
cd "$(dirname "$0")/.."
ACTL="${ACTL:-target/release/actl.exe}"
CHROME="/c/Program Files/Google/Chrome/Application/chrome.exe"
APP="抖音"
OUT=/tmp/dy_vision
mkdir -p "$OUT"

$ACTL list-windows 2>/dev/null | sed 's/},{/\n/g' | grep 'Google Chrome' | grep "$APP" | grep -o '"hwnd":[0-9]*' | grep -o '[0-9]*' | while read H; do
  [ -n "$H" ] && $ACTL close-window --hwnd "$H" >/dev/null 2>&1
done
sleep 1

"$CHROME" --new-window "https://www.douyin.com" >/dev/null 2>&1 &
$ACTL wait --window "$APP" --timeout 25000 >/dev/null 2>&1
sleep 5
$ACTL wait --element role:Document#2 --stable --app "$APP" --timeout 25000 >/dev/null 2>&1

# 读屏标签开启(内容层进树的钥匙)
RS=$($ACTL find --name "开启读屏标签" --app "$APP" --limit 3 2>/dev/null | sed 's/},{/\n/g' | grep -o '"ref":"[^"]*"' | head -1 | grep -o '@e[0-9]*')
if [ -n "$RS" ]; then
  $ACTL focus-window "$APP" >/dev/null 2>&1
  $ACTL click "$RS" --app "$APP" --physical >/dev/null 2>&1 && echo "读屏标签已开"
  sleep 2
fi

$ACTL wait --element role:Document#2 --stable --app "$APP" --timeout 20000 >/dev/null 2>&1
echo "评论标记(点击前):"
$ACTL get role:Document#2 --app "$APP" --property text 2>/dev/null | grep -o "评论" | wc -l | sed 's/^/  /'

# 截图 + 窗口 rect(换算基准:屏幕坐标 = 窗口左上角 + PNG 坐标)
$ACTL screenshot --app "$APP" > "$OUT/shot.json" 2>&1
grep -o '"path":"[^"]*"\|"width":[0-9]*\|"height":[0-9]*' "$OUT/shot.json" | head -3 | sed 's/^/  /'
$ACTL list-windows 2>/dev/null | sed 's/},{/\n/g' | grep 'Google Chrome' | grep "$APP" | grep -o '"rect":\[[^]]*\]' | head -1 | sed 's/^/  win-rect: /'
