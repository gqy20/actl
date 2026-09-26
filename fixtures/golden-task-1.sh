#!/usr/bin/env bash
# 金任务 1:计算器 123×456+789 → 断言 "56,877"(定义:docs/07-golden-tasks.md)
# 用法:bash fixtures/golden-task-1.sh   (ACTL 可覆盖二进制路径,默认 release)
# 轮数:默认 3(日常回归);正式 M2 验收跑 RUNS=10(docs/07 十次重跑口径)
# 说明:结果控件 CalculatorResults 是 Text(非 ValuePattern),经 name 读取 —— 与 07 文档
#      的 "--property value" 写法差异为实测修正,断言依赖真实输出形状。
#      12 键经单次 batch 执行(@eN ref 重放语义保留;进程冷启动 13 次 → 1 次)。
set -uo pipefail
cd "$(dirname "$0")/.."
ACTL="${ACTL:-target/release/actl.exe}"
APP="计算器"
# ── 失败安全清理:trap EXIT 必执行(成败/中断都跑);优雅关闭 + 对话框处理,GUI 应用禁 taskkill(会话复活)──
close_all_by_title() {
  local n=0 H
  while [ $n -lt 12 ]; do
    n=$((n+1))
    H=$("$ACTL" list-windows 2>/dev/null | grep -o "{[^{}]*\"title\":\"[^\"]*$1[^\"]*\"[^{}]*}" | grep -o '"hwnd":[0-9]*' | head -1 | grep -o '[0-9]*')
    [ -z "$H" ] && break
    "$ACTL" close-window --hwnd "$H" >/dev/null 2>&1 || break
    sleep 0.7
    "$ACTL" press n --app "是否" >/dev/null 2>&1
    sleep 0.3
  done
}
cleanup() { close_all_by_title "计算器"; }
trap cleanup EXIT

# 前置断言:残留会让 --app AMBIGUOUS;脏则 fail-fast(trap 顺手清掉,重跑即净)
WINCNT=$("$ACTL" list-windows 2>/dev/null | grep -o "{[^{}]*\"title\":\"[^\"]*计算器[^\"]*\"[^{}]*}" | wc -l)
if [ "$WINCNT" -gt 0 ]; then
  echo "ENV-DIRTY: 计算器窗口已存在 ×$WINCNT——已清理,请重跑本 fixture"; exit 1
fi
cmd //c "start shell:AppsFolder\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"
RUNS="${RUNS:-3}"
BUTTONS=(num1Button num2Button num3Button multiplyButton num4Button num5Button
         num6Button plusButton num7Button num8Button num9Button equalButton)

pass=0; fail=0
for i in $(seq 1 "$RUNS"); do
  t0=$(date +%s%N)
  ok=1
  "$ACTL" click id:clearButton --app "$APP" >/dev/null 2>&1 || ok=0
  snap=$("$ACTL" snapshot --app "$APP" --skeleton 2>/dev/null)
  cmds=""
  for btn in "${BUTTONS[@]}"; do
    # 元素对象内字段为字母序,automation_id 先于 ref,单行内截取到对象边界
    ref=$(printf '%s' "$snap" | grep -o "\"automation_id\":\"$btn\"[^}]*" | grep -o '@e[0-9]*' | head -1)
    if [ -z "$ref" ]; then
      echo "round $i: FAIL no ref resolves for $btn"
      ok=0; break
    fi
    cmds+="\"click $ref --app $APP\","
  done
  if [ "$ok" -eq 1 ]; then
    out=$("$ACTL" batch --json "[${cmds%,}]" 2>/dev/null)
    if ! printf '%s' "$out" | grep -q '"executed":12'; then
      echo "round $i: FAIL batch clicks: $(printf '%s' "$out" | head -c 200)"
      ok=0
    fi
  fi
  result=$("$ACTL" get id:CalculatorResults --app "$APP" 2>/dev/null)
  ms=$(( ($(date +%s%N) - t0) / 1000000 ))
  if [ "$ok" -eq 1 ] && printf '%s' "$result" | grep -q '56,877'; then
    pass=$((pass + 1)); echo "round $i: PASS (${ms}ms)"
  else
    fail=$((fail + 1))
    echo "round $i: FAIL (${ms}ms) result=$(printf '%s' "$result" | grep -o '"name":"[^"]*"')"
  fi
done

echo "=== golden task 1: $pass/$((pass + fail)) passed (expect $RUNS/$RUNS) ==="
[ "$fail" -eq 0 ]
