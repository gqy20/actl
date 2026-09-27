#!/usr/bin/env bash
# 诊断:登录态到底生效没有 —— ① 枚举用户的 Chrome 窗口 ② 新开窗截图仲裁
set -uo pipefail
cd "$(dirname "$0")/.."
ACTL="${ACTL:-target/release/actl.exe}"
CHROME="/c/Program Files/Google/Chrome/Application/chrome.exe"
APP="小红书"
OUT=/tmp/xhs_diag
mkdir -p "$OUT"

echo "== A. 现存 Chrome 窗口(用户的会话) =="
$ACTL list-windows 2>/dev/null | sed 's/},{/\n/g' | grep 'Google Chrome' | grep -o '"title":"[^"]\{0,60\}"' | head -10

echo "== B. 新开窗 + 截图 =="
$ACTL list-windows 2>/dev/null | sed 's/},{/\n/g' | grep 'Google Chrome' | grep "$APP" | grep -o '"hwnd":[0-9]*' | grep -o '[0-9]*' | while read H; do
  [ -n "$H" ] && $ACTL close-window --hwnd "$H" >/dev/null 2>&1
done
sleep 1
"$CHROME" --new-window "https://www.xiaohongshu.com" >/dev/null 2>&1 &
$ACTL wait --window "$APP" --timeout 20000 >/dev/null 2>&1
sleep 6
$ACTL wait --element role:Document#2 --stable --app "$APP" --timeout 25000 >/dev/null 2>&1
$ACTL screenshot --app "$APP" > "$OUT/shot.json" 2>&1
grep -o '"path":"[^"]*"' "$OUT/shot.json" | head -1
echo "== C. 地址栏 URL(登录态判据之二) =="
$ACTL get id:RootWebArea --app "$APP" --property value 2>/dev/null | grep -o '"value":"[^"]*"' | head -1
