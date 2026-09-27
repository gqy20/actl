#!/usr/bin/env bash
# B 站深度闭环(第五轮,确定性入口):搜索结果页选真视频卡 → ref 精点 → 新锚 → 评论
set -uo pipefail
cd "$(dirname "$0")/.."
ACTL="${ACTL:-target/release/actl.exe}"
CHROME="/c/Program Files/Google/Chrome/Application/chrome.exe"
APP="哔哩哔哩"
DOC="role:Document#2"
OUT=/tmp/bili_deep5
mkdir -p "$OUT"

$ACTL list-windows 2>/dev/null | sed 's/},{/\n/g' | grep 'Google Chrome' | grep -E "$APP|bilibili|冠军赛" | grep -o '"hwnd":[0-9]*' | grep -o '[0-9]*' | while read H; do
  [ -n "$H" ] && $ACTL close-window --hwnd "$H" >/dev/null 2>&1
done
sleep 1

echo "== 1. 直达搜索结果页(ASCII 关键词防 GBK) =="
"$CHROME" --new-window "https://search.bilibili.com/all?keyword=AI" >/dev/null 2>&1 &
$ACTL wait --window "$APP" --timeout 20000 >/dev/null 2>&1
sleep 3
$ACTL wait --element "$DOC" --stable --app "$APP" --timeout 25000 2>&1 | grep -o '"waited":{[^}]*}' | sed 's/^/   /'

echo "== 2. 选结果卡(搜索结果的 Hyperlink 即视频标题) =="
$ACTL find --role link --app "$APP" --limit 80 > "$OUT/links.json" 2>&1
py - "$OUT/links.json" "$OUT/pick.txt" <<'EOF'
import json, sys
d = json.load(open(sys.argv[1], encoding='utf-8'))
els = d.get('data', {}).get('elements', [])
BAD = ('bilibili','登录','注册','搜索','筛选','排序','综合','播放量','收藏','帮帮',
       'AI赋能','活动','大赛','企划','直播','专栏','话题','用户')
cands = [e for e in els if e.get('name') and len(e['name']) >= 12
         and not any(b in e['name'] for b in BAD) and e.get('ref') and e.get('role') == 'Hyperlink']
for c in cands[:4]: print('   cand:', c['name'][:38])
if cands:
    open(sys.argv[2], 'w', encoding='utf-8').write(cands[0]['ref'] + '\t' + cands[0]['name'])
EOF
LINE=$(cat "$OUT/pick.txt")
[ -z "$LINE" ] && { echo "no candidate"; exit 1; }
REF=$(cut -f1 <<<"$LINE"); TITLE=$(cut -f2 <<<"$LINE")
echo "   picked ref=$REF title=${TITLE:0:40}"

echo "== 3. ref 精点 + 新标题锚 =="
HW=$($ACTL list-windows 2>/dev/null | sed 's/},{/\n/g' | grep 'Google Chrome' | grep "$APP" | grep -o '"hwnd":[0-9]*' | grep -o '[0-9]*' | head -1)
$ACTL focus-window "$APP" >/dev/null 2>&1
$ACTL click "$REF" --app "$APP" --timeout 10000 > "$OUT/click.json" 2>&1
grep -o '"ok":[a-z]*\|"resolved_by":"[^"]*"' "$OUT/click.json" | head -2 | sed 's/^/   /'
sleep 3
PRE=$($ACTL list-windows 2>/dev/null | sed 's/},{/\n/g' | grep 'Google Chrome' | grep "$APP" | grep -o '"title":"[^"]*"' | head -1)
NEW=$($ACTL list-windows 2>/dev/null | sed 's/},{/\n/g' | grep 'Google Chrome' | grep -o '"title":"[^"]\{1,60\}"' | grep -v "$PRE" | head -1 | grep -o '"title":"[^"]*"' | sed 's/"title":"//;s/"$//')
echo "   新锚: $NEW(点击前: $PRE)"
if [ -z "$NEW" ]; then echo "   未捕获新标题——沿用原锚"; NEW="$APP"; fi
$ACTL wait --element "$DOC" --stable --app "$NEW" --timeout 30000 2>&1 | grep -o '"waited":{[^}]*}' | sed 's/^/   /'

echo "== 4. 视频页 + 滚到评论区(pgdn×5) =="
$ACTL get "$DOC" --app "$NEW" --property text > "$OUT/video.json" 2>&1
echo "   video text: $(wc -c < "$OUT/video.json")B; '弹幕' $(grep -o '弹幕' "$OUT/video.json" | wc -l) 次"
$ACTL focus-window "$NEW" >/dev/null 2>&1
for i in 1 2 3 4 5; do
  $ACTL press pgdn --app "$NEW" >/dev/null 2>&1
  $ACTL wait --element "$DOC" --stable --app "$NEW" --timeout 12000 >/dev/null 2>&1
done
$ACTL get "$DOC" --app "$NEW" --property text > "$OUT/comments.json" 2>&1
echo "   滚后 text: $(wc -c < "$OUT/comments.json")B"
for M in "评论" "回复" "条评论"; do
  C=$(grep -o "$M" "$OUT/comments.json" | wc -l); echo "   '$M': $C"
done
echo "   评论文本采样:"
grep -o '"text":"[^"]*"' "$OUT/comments.json" | grep -o '.\{0,80\}评论.\{0,120\}' | head -4 | sed 's/^/     /'

echo "== 5. 截图仲裁 =="
$ACTL screenshot --app "$NEW" > "$OUT/shot.json" 2>&1
grep -o '"path":"[^"]*"' "$OUT/shot.json" | head -1

echo "== 6. cleanup =="
$ACTL close-window --hwnd "$HW" >/dev/null 2>&1 && echo "   closed $HW"
echo "done."
