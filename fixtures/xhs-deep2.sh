#!/usr/bin/env bash
# 深度闭环(续):点击具体卡片(@ref)→ 笔记页断言 → 滚动读评论 → esc 返回。
# 前置:xhs-login-check.sh 开的窗口仍在(登录态,卡片已物化,ref 已反查到)。
set -uo pipefail
cd "$(dirname "$0")/.."
ACTL="${ACTL:-target/release/actl.exe}"
APP="小红书"
DOC="role:Document#2"
OUT=/tmp/xhs_deep2
mkdir -p "$OUT"

echo "== 1. 重新定位卡片(ref 以最新 find 为准) =="
$ACTL find --name "Cherry Studio" --app "$APP" --limit 5 > "$OUT/find1.json" 2>&1
py - "$OUT/find1.json" "$OUT/ref.txt" <<'EOF'
import json, sys
d = json.load(open(sys.argv[1], encoding='utf-8'))
els = d.get('data', {}).get('elements', [])
pick = next((e['ref'] for e in els if e.get('ref') and e.get('role') == 'Hyperlink'), None)
open(sys.argv[2], 'w').write(pick or '')
print('  ref:', pick)
EOF
REF=$(cat "$OUT/ref.txt")
[ -z "$REF" ] && { echo "  no ref"; exit 1; }

echo "== 2. 点击卡片 =="
$ACTL focus-window "$APP" >/dev/null 2>&1
$ACTL click "$REF" --app "$APP" --timeout 10000 > "$OUT/click.json" 2>&1
grep -o '"ok":[a-z]*\|"resolved_by":"[^"]*"' "$OUT/click.json" | head -2 | sed 's/^/   /'

echo "== 3. 笔记页:settle + 文本断言 =="
sleep 2
$ACTL wait --element "$DOC" --stable --app "$APP" --timeout 25000 > "$OUT/settle_n.json" 2>&1
grep -o '"waited":{[^}]*}' "$OUT/settle_n.json" | sed 's/^/   /'
$ACTL get "$DOC" --app "$APP" --property text > "$OUT/note.json" 2>&1
grep -q '"ok":true' "$OUT/note.json" && echo "   note text: $(wc -c < "$OUT/note.json")B" || { echo "   FAILED"; head -c 300 "$OUT/note.json"; }
grep -q "Cherry Studio" "$OUT/note.json" && echo "   PASS: 笔记标题出现在详情文本" || echo "   (标题不在 Document#2——检查弹层/新文档)"
for N in 1 2 3 4 5; do
  L=$($ACTL get "role:Document#$N" --app "$APP" --property text 2>/dev/null | wc -c)
  [ "$L" -gt 200 ] && echo "   Document#$N 现为 ${L}B(弹层?)"
done

echo "== 4. 滚动读评论(pgdn ×4) =="
for i in 1 2 3 4; do
  $ACTL press pgdn --app "$APP" >/dev/null 2>&1
  S=$($ACTL wait --element "$DOC" --stable --app "$APP" --timeout 15000 2>&1 | grep -o '"changes":[0-9]*')
  $ACTL get "$DOC" --app "$APP" --property text > "$OUT/note_s$i.json" 2>&1
  echo "   scroll#$i: $S text=$(wc -c < "$OUT/note_s$i.json")B"
done
for M in "评论" "回复" "收藏" "点赞" "条评论"; do
  C=$(cat "$OUT"/note*.json 2>/dev/null | grep -o "$M" | wc -l)
  echo "   标记 '$M': $C 次"
done

echo "== 5. 截图仲裁(笔记页当前状态) =="
$ACTL screenshot --app "$APP" > "$OUT/shot2.json" 2>&1
grep -o '"path":"[^"]*"' "$OUT/shot2.json" | head -1

echo "== 6. esc 返回 + 断言 =="
$ACTL press esc --app "$APP" >/dev/null 2>&1
sleep 1
$ACTL wait --element "$DOC" --stable --app "$APP" --timeout 15000 >/dev/null 2>&1
$ACTL get "$DOC" --app "$APP" --property text > "$OUT/back.json" 2>&1
echo "   返回后 text=$(wc -c < "$OUT/back.json")B(笔记前 feed≈4451B)"

echo "== 7. cleanup =="
$ACTL list-windows 2>/dev/null | sed 's/},{/\n/g' | grep 'Google Chrome' | grep "$APP" | grep -o '"hwnd":[0-9]*' | grep -o '[0-9]*' | while read H; do
  [ -n "$H" ] && $ACTL close-window --hwnd "$H" >/dev/null 2>&1 && echo "   closed $H"
done
echo "done. artifacts in $OUT/"
