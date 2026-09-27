#!/usr/bin/env bash
# 场景实测(轨 2,不进 CI):小红书网页版浏览闭环 —— 真实会话浏览器赛道。
# 战记与数字:docs/16-browser-scenarios.md(2026-09-27 首验)。
# 闭环:地址栏导航(ctrl+l+paste)→ settle → 读 feed(text 通道)→ 滚动×2
# (pgdn + settle + 增量读)。零写入操作(不点赞不评论不发送)。
# 怪癖(实测沉淀,详见 apps/browser.yml):xhs 有 2 个 RootWebArea(主文档 +
# 登录墙 iframe),feed 正主 = role:Document#2;未登录态卡片是图片对象
# (U+FFFC),文本通道只见骨架——登录态实测留待用户扫码后补。
# 注:脚本文件 UTF-8(AGENTS:含中文的命令序列禁止 bash 内联)。
set -uo pipefail
cd "$(dirname "$0")/.."
ACTL="${ACTL:-target/release/actl.exe}"
EDGE="/c/Program Files (x86)/Microsoft/Edge/Application/msedge.exe"
APP="小红书"
DOC="role:Document#2"
OUT=/tmp/xhs_browse
mkdir -p "$OUT"

# 预清理:残留的 about:blank / 小红书 Edge 窗口(避免多窗口 AMBIGUOUS)
for PAT in 'about:blank' "$APP"; do
  $ACTL list-windows 2>/dev/null | sed 's/},{/\n/g' | grep "$PAT" | grep -o '"hwnd":[0-9]*' | grep -o '[0-9]*' | while read H; do
    [ -n "$H" ] && $ACTL close-window --hwnd "$H" >/dev/null 2>&1
  done
done
sleep 1

echo "== 1. 新窗口 + 地址栏导航 =="
"$EDGE" --new-window "about:blank" >/dev/null 2>&1 &
sleep 2
$ACTL focus-window "about:blank" >/dev/null 2>&1
$ACTL press ctrl+l --app "about:blank" >/dev/null 2>&1
$ACTL set-clipboard "https://www.xiaohongshu.com" >/dev/null 2>&1
$ACTL paste --app "about:blank" >/dev/null 2>&1
$ACTL press enter --app "about:blank" >/dev/null 2>&1
$ACTL wait --window "$APP" --timeout 15000 2>&1 | grep -o '"ok":[a-z]*' | sed 's/^/   window: /'

echo "== 2. settle + 读 feed =="
$ACTL wait --element "$DOC" --stable --app "$APP" --timeout 25000 > "$OUT/settle1.json" 2>&1
grep -o '"waited":{[^}]*}' "$OUT/settle1.json" | sed 's/^/   /'
$ACTL get "$DOC" --app "$APP" --property text > "$OUT/text1.json" 2>&1
grep -q '"ok":true' "$OUT/text1.json" && echo "   text1: $(wc -c < "$OUT/text1.json")B envelope" || { echo "   FAILED"; head -c 300 "$OUT/text1.json"; }

echo "== 3. 滚动循环 x2(pgdn + settle + 增量读) =="
$ACTL focus-window "$APP" >/dev/null 2>&1
for i in 1 2; do
  $ACTL press pgdn --app "$APP" >/dev/null 2>&1
  S=$($ACTL wait --element "$DOC" --stable --app "$APP" --timeout 20000 2>&1 | grep -o '"waited":{[^}]*}')
  $ACTL get "$DOC" --app "$APP" --property text > "$OUT/text_s$i.json" 2>&1
  echo "   scroll#$i: $S text=$(wc -c < "$OUT/text_s$i.json")B"
done

echo "== 4. 树形观察削减(chrome 整枝) =="
for k in full skel both; do
  case $k in
    full) ARGS=() ;;
    skel) ARGS=(--skeleton) ;;
    both) ARGS=(--view content --skeleton) ;;
  esac
  $ACTL snapshot --app "$APP" "${ARGS[@]}" > "$OUT/snap_$k.json" 2>/dev/null
  echo "   $k: $(wc -c < "$OUT/snap_$k.json")B / $(grep -o '"role"' "$OUT/snap_$k.json" | wc -l) el"
done

echo "== 5. cleanup =="
$ACTL list-windows 2>/dev/null | sed 's/},{/\n/g' | grep "$APP" | grep -o '"hwnd":[0-9]*' | grep -o '[0-9]*' | while read H; do
  [ -n "$H" ] && $ACTL close-window --hwnd "$H" >/dev/null 2>&1 && echo "   closed hwnd $H"
done
echo "done. artifacts in $OUT/"
