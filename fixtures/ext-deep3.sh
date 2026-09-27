#!/usr/bin/env bash
# 扩展面收尾:① 弹层树形读取(text 空但树可能有);② chrome://extensions 地址栏导航;
# ③ 右键菜单(先确保前台无遮挡,对可见元素右键)。
set -uo pipefail
cd "$(dirname "$0")/.."
ACTL="${ACTL:-target/release/actl.exe}"
CHROME="/c/Program Files/Google/Chrome/Application/chrome.exe"
APP="小红书"
OUT=/tmp/ext_deep3
mkdir -p "$OUT"

$ACTL list-windows 2>/dev/null | sed 's/},{/\n/g' | grep 'Google Chrome' | grep -E "$APP|新标签" | grep -o '"hwnd":[0-9]*' | grep -o '[0-9]*' | while read H; do
  [ -n "$H" ] && $ACTL close-window --hwnd "$H" >/dev/null 2>&1
done
sleep 1
"$CHROME" --new-window "https://www.xiaohongshu.com" >/dev/null 2>&1 &
$ACTL wait --window "$APP" --timeout 20000 >/dev/null 2>&1
sleep 4
$ACTL wait --element role:Document#2 --stable --app "$APP" --timeout 25000 >/dev/null 2>&1

echo "== 1. 开弹层 + 树形读取(--view content --skeleton) =="
BREF=$($ACTL find --name "猫抓" --app "$APP" --limit 5 2>/dev/null | sed 's/},{/\n/g' | grep -o '"ref":"[^"]*"' | head -1 | grep -o '@e[0-9]*')
echo "   btn ref: $BREF"
$ACTL focus-window "$APP" >/dev/null 2>&1
$ACTL click "$BREF" --app "$APP" --physical >/dev/null 2>&1 && echo "   popup opened"
sleep 2
$ACTL snapshot --app "$APP" --view content --skeleton > "$OUT/popup_tree.json" 2>/dev/null
echo "   树元素数: $(grep -o '"role"' "$OUT/popup_tree.json" | wc -l)"
echo "   弹层区采样(找 mp4/资源/合并 等关键词):"
for K in "mp4" "MB" "合并" "资源" "订阅" "cat-catch"; do
  C=$(grep -o "$K" "$OUT/popup_tree.json" | wc -l); echo "     '$K': $C"
done
echo "   尾部元素 name 采样:"
sed 's/},{/\n/g' "$OUT/popup_tree.json" | grep -o '"name":"[^"]\{2,40\}"' | tail -15 | sed 's/^/     /'
$ACTL press esc --app "$APP" >/dev/null 2>&1

echo "== 2. chrome://extensions(地址栏导航,锚定扩展程序) =="
$ACTL focus-window "$APP" >/dev/null 2>&1
$ACTL press ctrl+l --app "$APP" >/dev/null 2>&1
$ACTL set-clipboard "chrome://extensions" >/dev/null 2>&1
$ACTL paste --app "$APP" >/dev/null 2>&1
$ACTL press enter --app "$APP" >/dev/null 2>&1
sleep 3
$ACTL wait --window "扩展程序" --timeout 10000 2>&1 | grep -o '"ok":[a-z]*' | sed 's/^/   window: /'
$ACTL find --name "篡改猴" --app "扩展程序" --limit 5 > "$OUT/tamper.json" 2>&1
sed 's/},{/\n/g' "$OUT/tamper.json" | grep -o '"ref":"[^"]*"\|"role":"[^"]*"' | head -4 | sed 's/^/   /'
echo "   toggle 枚举(只读不切):"
$ACTL find --role toggle --app "扩展程序" --limit 30 2>&1 | sed 's/},{/\n/g' | grep -c '"ref"' | sed 's/^/     found: /'
$ACTL get role:Document#1 --app "扩展程序" --property text > "$OUT/ext_text.json" 2>&1
echo "   页面 text: $(wc -c < "$OUT/ext_text.json")B"
grep -o '"text":"[^"]*"' "$OUT/ext_text.json" | head -c 400 | tail -c 350; echo
$ACTL screenshot --app "扩展程序" > "$OUT/shot_ext.json" 2>&1
grep -o '"path":"[^"]*"' "$OUT/shot_ext.json" | head -1 | sed 's/^/   shot: /'

echo "== 3. 右键菜单(回 xhs,对可见卡片右键) =="
$ACTL press alt+left --app "扩展程序" >/dev/null 2>&1
sleep 2
$ACTL wait --element role:Document#2 --stable --app "$APP" --timeout 20000 >/dev/null 2>&1
$ACTL focus-window "$APP" >/dev/null 2>&1
sleep 1
TREF=$($ACTL find --role link --app "$APP" --limit 5 2>/dev/null | sed 's/},{/\n/g' | grep -o '"ref":"[^"]*"' | head -1 | grep -o '@e[0-9]*')
echo "   目标: $TREF"
$ACTL right-click "$TREF" --app "$APP" > "$OUT/rc.json" 2>&1
grep -o '"ok":[a-z]*\|"via":"[^"]*"' "$OUT/rc.json" | head -2 | sed 's/^/   right-click: /'
sleep 1
echo "   菜单项:"
$ACTL find --role menuitem --app "$APP" --limit 20 2>&1 | sed 's/},{/\n/g' | grep -o '"name":"[^"]\{1,34\}"' | head -14 | sed 's/^/     /'
$ACTL screenshot --app "$APP" > "$OUT/shot_menu.json" 2>&1
grep -o '"path":"[^"]*"' "$OUT/shot_menu.json" | head -1 | sed 's/^/   shot: /'
$ACTL press esc --app "$APP" >/dev/null 2>&1

echo "== 4. cleanup =="
$ACTL list-windows 2>/dev/null | sed 's/},{/\n/g' | grep 'Google Chrome' | grep "$APP" | grep -o '"hwnd":[0-9]*' | grep -o '[0-9]*' | while read H; do
  [ -n "$H" ] && $ACTL close-window --hwnd "$H" >/dev/null 2>&1 && echo "   closed $H"
done
echo "done. artifacts in $OUT/"
