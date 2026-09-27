#!/usr/bin/env bash
# 验收 · --view content 内容域投影 + wait --stable settle 谓词(spec 批实测):
# Edge(系统自带 Chromium)打开本地页面——含慢速 JS 追加器(settle 的活体信号)。
# ① 投影削减:full vs skeleton vs --view content vs 组合,四档字节/元素数实测
#    (断言仅:content < skeleton——工具栏交互按钮被整枝丢弃的机制验证;
#     绝对削减率记录不断言,数字进 docs/16);
# ② view 标记:content 快照 data.view == "content";
# ③ settle:追加器运行期间 wait --stable 阻塞至内容停止变化(waited_ms ≥ 追加窗);
# ④ 正文:text 通道读回含追加器产物。
# 依赖:系统自带 Edge;纯观察 + wait,零键盘注入(桌面不必空闲)。
set -uo pipefail
cd "$(dirname "$0")/.."
ACTL="${ACTL:-target/release/actl.exe}"
PASS=0; FAIL=0
STAMP=$(date +%H%M%S)
WIN="actl-edge-view-$STAMP"
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

# ── 1. 测试页(慢速追加器:250ms 一条,共 12 条 ≈ 3s 追加窗)+ Edge 新窗口 ──
cat > "$HTML_UX" << EOF
<!DOCTYPE html><html><head><title>$WIN</title></head>
<body><main><h1>Content View Probe</h1>
<nav><a href="#a">Alpha link</a> <a href="#b">Beta link</a> <a href="#c">Gamma link</a></nav>
<p>The quick brown fox jumps over the lazy dog. 敏捷的棕色狐狸跳过了懒狗。</p>
<ol id="feed"></ol>
<button id="dup-a">dup-target</button><button id="dup-b">dup-target</button>
<script>
let i = 0;
const t = setInterval(() => {
  i++;
  const li = document.createElement('li');
  li.textContent = 'lazy-item-' + i;
  document.getElementById('feed').appendChild(li);
  if (i >= 40) clearInterval(t);
}, 250);
</script></main></body></html>
EOF
cleanup() {
  "$ACTL" close-window "$WIN" >/dev/null 2>&1
  rm -f "$HTML_UX" /tmp/edgev_*_$STAMP.json
}
trap cleanup EXIT
"$EDGE" --new-window "file:///$HTML_UX" >/dev/null 2>&1 &
step "wait edge window" $ACTL wait --window "$WIN" --timeout 12000
# 等文档根物化(Chromium a11y 按需构建:首次查询才建树,RootWebArea 是
# 最稳的就绪锚点——链接/按钮名在树建全前可能未解析)
step "wait document root materialized" $ACTL wait --element "id:RootWebArea" --app "$WIN" --timeout 10000

# ── 2. settle:追加器仍在跑 → --stable 须阻塞到内容停止 ─────────────────
# 追加窗 ~10s(40×250ms);前置(Edge 起窗 + RootWebArea 就绪 + actl 冷启)
# 实测 2-5s,余量充足。changes ≥ 1 证明"变化期间不返回"(对照实验
# 2026-09-27:changes=5/samples=7/waited=8004ms);waited ≥ 1000 证明确实等待。
SETTLE_OUT=$("$ACTL" wait --element "id:RootWebArea" --stable --app "$WIN" --timeout 20000 2>&1)
if grep -q '"ok":true' <<<"$SETTLE_OUT"; then
  WAITED=$(grep -o '"waited_ms":[0-9]*' <<<"$SETTLE_OUT" | grep -o '[0-9]*')
  CHANGES=$(grep -o '"changes":[0-9]*' <<<"$SETTLE_OUT" | grep -o '[0-9]*')
  if [ "${WAITED:-0}" -ge 1000 ] && [ "${CHANGES:-0}" -ge 1 ]; then
    PASS=$((PASS+1)); echo "PASS: settle blocked through ${CHANGES} content changes (waited ${WAITED}ms)"
  else
    FAIL=$((FAIL+1)); echo "FAIL: settle waited=${WAITED}ms changes=${CHANGES} (expected >=1000ms / >=1 change)"
  fi
  echo "settle evidence: waited=${WAITED}ms changes=${CHANGES:-n/a}"
else
  FAIL=$((FAIL+1)); echo "FAIL: wait --stable errored"; head -c 300 <<<"$SETTLE_OUT"; echo
fi

# ── 2b. wait 族 AMBIGUOUS 快速失败(2026-09-27 spec):同名双按钮 →
# 等待不可能收敛,必须立即 AMBIGUOUS(不等满超时;samples 冻结类误导性
# TIMEOUT 的根治)
AMBIG_OUT=$("$ACTL" wait --element "name:dup-target" --app "$WIN" --timeout 5000 2>&1)
AMBIG_DUR=$(grep -o '"duration_ms":[0-9]*' <<<"$AMBIG_OUT" | grep -o '[0-9]*')
if grep -q '"code":"AMBIGUOUS"' <<<"$AMBIG_OUT" && [ "${AMBIG_DUR:-99999}" -lt 3000 ]; then
  PASS=$((PASS+1)); echo "PASS: wait fast-fails on AMBIGUOUS (${AMBIG_DUR}ms, not 5000ms)"
else
  FAIL=$((FAIL+1)); echo "FAIL: AMBIGUOUS fast-fail (code/duration wrong)"; head -c 300 <<<"$AMBIG_OUT"; echo
fi

# ── 3. 四档快照实测(字节 + 元素数;数字回显,断言只做机制方向) ──────────
measure() { # $1=tag $2...=actl args
  local tag="$1"; shift
  "$ACTL" snapshot --app "$WIN" "$@" > /tmp/edgev_${tag}_$STAMP.json 2>/dev/null
  SIZE[$tag]=$(wc -c < /tmp/edgev_${tag}_$STAMP.json)
  CNT[$tag]=$(grep -o '"role"' /tmp/edgev_${tag}_$STAMP.json | wc -l)
}
declare -A SIZE CNT
measure full
measure skel --skeleton
measure content --view content
measure both --view content --skeleton
echo "sizes: full=${SIZE[full]}B/${CNT[full]}el skeleton=${SIZE[skel]}B/${CNT[skel]}el content=${SIZE[content]}B/${CNT[content]}el content+skeleton=${SIZE[both]}B/${CNT[both]}el"
if [ "${SIZE[content]}" -lt "${SIZE[skel]}" ]; then
  PASS=$((PASS+1)); echo "PASS: content < skeleton bytes (${SIZE[content]} < ${SIZE[skel]})"
else
  FAIL=$((FAIL+1)); echo "FAIL: content not smaller than skeleton (${SIZE[content]} >= ${SIZE[skel]})"
fi
grep -q '"view":"content"' /tmp/edgev_content_$STAMP.json \
  && { PASS=$((PASS+1)); echo "PASS: view marker present"; } \
  || { FAIL=$((FAIL+1)); echo "FAIL: view marker missing"; }

# ── 4. 正文:text 通道读回追加器产物(settle 之后应全量可见) ─────────────
step "read page text (text-pattern)" $ACTL get "id:RootWebArea" --app "$WIN" --property text
assert_contains "appender tail item present" 'lazy-item-40'
assert_contains "chinese sentence present" '敏捷的棕色狐狸'

echo "──"
echo "edge-content-view: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
