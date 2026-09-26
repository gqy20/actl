#!/usr/bin/env bash
# 验收 B · 老 Win32 大列表结构化提取(P3:extract --table):
# msinfo32(系统信息)= PRD 金任务 3"老 Win32 报表工具抠数"的正宗标的。
# 右侧摘要列表(ListView)→ 44 行 [名称, 值] 二列结构化 JSON。
# 断言走结构与关键内容(行数随机器浮动,核心行必有);来源标记如实上报
# (msinfo32 无 GridPattern → tree-flatten;grid-pattern 直读待真实 Grid 控件)。
# 依赖:系统自带 msinfo32;启动较慢(首次 ~8s),无需桌面空闲(纯观察)。
set -uo pipefail
cd "$(dirname "$0")/.."
ACTL="${ACTL:-target/release/actl.exe}"
PASS=0; FAIL=0

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

# ── 1. 启动 msinfo32(老 Win32,#32770 dialog;冷启动慢) ───────────────
# 幂等:残留的同名窗口会让 --app AMBIGUOUS,先关净
taskkill //IM msinfo32.exe //F >/dev/null 2>&1
sleep 1
cmd //c "start msinfo32"
step "wait msinfo32 window" $ACTL wait --window "系统信息" --timeout 20000

# ── 2. extract:右侧摘要列表 → [名称, 值] 二列结构 ────────────────────
step "extract summary table" $ACTL extract "role:List" --app "系统信息"
assert_contains "source reported" '"source":"[a-z-]*"'

# 行数与关键内容断言(行数随机器浮动,断言下界 + 核心行)
ROWS=$(grep -o '"rows":[0-9]*' <<<"$LAST_OUT" | grep -o '[0-9]*')
if [ "${ROWS:-0}" -ge 30 ]; then
  PASS=$((PASS+1)); echo "PASS: rows=$ROWS (>=30)"
else
  FAIL=$((FAIL+1)); echo "FAIL: rows=$ROWS (<30)"
fi
assert_contains "OS name row present" '操作系统名称'
assert_contains "OS value row present" 'Microsoft Windows'
assert_contains "BIOS row present" 'BIOS'

# 每行两列(名称, 值)——抽查首行结构
if grep -q '\[\["' <<<"$LAST_OUT"; then
  PASS=$((PASS+1)); echo "PASS: rows are JSON arrays"
else
  FAIL=$((FAIL+1)); echo "FAIL: table structure unexpected"
fi

# ── 3. 清理 ───────────────────────────────────────────────────────────
taskkill //IM msinfo32.exe //F >/dev/null 2>&1; PASS=$((PASS+1)); echo "PASS: close msinfo32"

echo "──"
echo "win32-table-extract: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
