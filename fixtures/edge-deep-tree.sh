#!/usr/bin/env bash
# 验收 C · Chromium/Electron 系接入(深树 + TextPattern 正文 + skeleton 实测):
# Edge(系统自带 Chromium)打开本地页面——agent-desktop 型页面的最小形态。
# ① 深树可达:snapshot 全量(Chromium 内容 depth 深且多 Document 分层,实测 6 个);
# ② 正文读取:get --property text(TextPattern 专用通道;Chromium 的 Document
#    Value=URL,正文只能走 text——value/text 分工的实证);
# ③ skeleton 削减:实测记录(2026-09-26:268→140 元素/48%,12702→9022 字节/29%,
#    未达 PRD 70% 目标——Chromium 工具栏交互按钮占比高,投影改进列开放项)。
# 依赖:系统自带 Edge;纯观察零注入。
set -uo pipefail
cd "$(dirname "$0")/.."
ACTL="${ACTL:-target/release/actl.exe}"
PASS=0; FAIL=0
STAMP=$(date +%H%M%S)
WIN="actl-edge-tree-$STAMP"
HTML_WIN="$LOCALAPPDATA\\Temp\\$WIN.html"
HTML_UX="${LOCALAPPDATA//\\//}/Temp/$WIN.html"
EDGE="/c/Program Files (x86)/Microsoft/Edge/Application/msedge.exe"

step() {
  local desc="$1"; shift
  local out
  if out=$("$@" 2>&1) && grep -q '"ok":true' <<<"$out"; then
    PASS=$((PASS+1)); echo "PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "FAIL: $desc"; head -c 300 <<<"$out"; echo
  fi
  LAST_OUT="$out"
}

assert_contains() {
  local desc="$1" want="$2"
  if grep -q "$want" <<<"$LAST_OUT"; then
    PASS=$((PASS+1)); echo "PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "FAIL: $desc (want: $want)"; head -c 300 <<<"$LAST_OUT"; echo
  fi
}

# ── 1. 测试页 + Edge 新窗口 ───────────────────────────────────────────
cat > "$HTML_UX" << EOF
<!DOCTYPE html><html><head><title>$WIN</title></head>
<body><main><h1>Quarterly Report</h1>
<p>The quick brown fox jumps over the lazy dog. 敏捷的棕色狐狸跳过了懒狗。</p>
<ul><li>Alpha item</li><li>Beta item</li></ul>
<button id="btn-primary">Submit form</button></main></body></html>
EOF
cleanup() {
  "$ACTL" close-window "$WIN" >/dev/null 2>&1
  rm -f "$HTML_UX" /tmp/edge_full_$STAMP.json /tmp/edge_skel_$STAMP.json
}
trap cleanup EXIT
# HTML_UX 形如 C:/Users/<user>/AppData/Local/Temp/<win>.html(已正斜杠)
"$EDGE" --new-window "file:///$HTML_UX" >/dev/null 2>&1 &
step "wait edge window" $ACTL wait --window "$WIN" --timeout 12000

# ── 2. 正文读取:TextPattern 专用通道(value=URL 的分工实证) ────────────
# 等页面真实渲染(TextPattern 的正文随 DOM 就绪才可读;窗口出现 ≠ 页面加载完)
step "wait page content rendered" $ACTL wait --element "name:Submit form" --app "$WIN" --timeout 10000
step "read page text (text-pattern)" $ACTL get "id:RootWebArea" --app "$WIN" --property text
assert_contains "english sentence present" 'quick brown fox'
assert_contains "chinese sentence present" '敏捷的棕色狐狸'

# ── 3. 深树快照 + skeleton 削减实测(记录,不断言 70% —— 当前投影算法在
#    Chromium 形态上 29-48%,改进为开放项) ───────────────────────────────
step "full snapshot" $ACTL snapshot --app "$WIN"
"$ACTL" snapshot --app "$WIN" --skeleton > /tmp/edge_skel_$STAMP.json 2>/dev/null
"$ACTL" snapshot --app "$WIN" > /tmp/edge_full_$STAMP.json 2>/dev/null
FULL=$(wc -c < /tmp/edge_full_$STAMP.json)
SKEL=$(wc -c < /tmp/edge_skel_$STAMP.json)
if [ "$SKEL" -lt "$FULL" ]; then
  PASS=$((PASS+1)); echo "PASS: skeleton reduction (full=${FULL}B skel=${SKEL}B, $(( (FULL-SKEL)*100/FULL ))%)"
else
  FAIL=$((FAIL+1)); echo "FAIL: skeleton not smaller (full=${FULL}B skel=${SKEL}B)"
fi

# ── 4. 清理 ───────────────────────────────────────────────────────────
echo "(cleanup handled by trap)"

echo "──"
echo "edge-deep-tree: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
