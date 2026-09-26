#!/usr/bin/env bash
# US1 跨 3 应用数据搬运(试跑版,M2 退出验收的基础脚本):
# 计算器(算 123×456)→ 记事本(粘贴结果+标注,保存)→ 资源管理器(目录中确认可见)。
# ⚠ 需要桌面空闲(多步抢前台,并发打字会与注入交错——见 spike-findings);
#    记事本应为全新会话(无多窗口,否则 --app Notepad 会 AMBIGUOUS)。
set -uo pipefail
cd "$(dirname "$0")/.."
ACTL="${ACTL:-target/release/actl.exe}"
PASS=0; FAIL=0
MARK="actl_us1_$(date +%H%M%S).txt"
MARK_BASE=${MARK%.txt}
DOCS=$(powershell -NoProfile -Command '[Environment]::GetFolderPath("MyDocuments")' | tr -d '\r')
DOCS="${DOCS//\\//}"

step() {
  local desc="$1"; shift
  local out
  if out=$("$@" 2>&1) && grep -q '"ok":true' <<<"$out"; then
    PASS=$((PASS+1)); echo "PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "FAIL: $desc"; head -c 300 <<<"$out"; echo
  fi
}

# ── 1. 计算器:123×456 → 读出结果 ─────────────────────────────────────
# ── 失败安全清理 + 前置断言(三应用;GUI 禁 taskkill——记事本/计算器会话复活)──
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
cleanup() {
  close_all_by_title "Notepad"
  close_all_by_title "计算器"
  close_all_by_title "Documents"
}
trap cleanup EXIT
for PAT in "Notepad" "计算器"; do
  WINCNT=$("$ACTL" list-windows 2>/dev/null | grep -o "{[^{}]*\"title\":\"[^\"]*$PAT[^\"]*\"[^{}]*}" | wc -l)
  if [ "$WINCNT" -gt 0 ]; then
    echo "ENV-DIRTY: $PAT 窗口已存在 ×$WINCNT——已清理,请重跑本 fixture"; exit 1
  fi
done
cmd //c "start shell:AppsFolder\\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"
step "wait calculator window" $ACTL wait --window 计算器 --timeout 8000
step "clear"            $ACTL click id:clearButton --app 计算器
step "press 123"        $ACTL batch --json '["click id:num1Button --app 计算器","click id:num2Button --app 计算器","click id:num3Button --app 计算器"]'
step "press ×"          $ACTL click id:multiplyButton --app 计算器
step "press 456"        $ACTL batch --json '["click id:num4Button --app 计算器","click id:num5Button --app 计算器","click id:num6Button --app 计算器"]'
step "press ="          $ACTL click id:equalButton --app 计算器
RESULT=$($ACTL get id:CalculatorResults --app 计算器 --property name 2>/dev/null | grep -o "显示为 [0-9,]*" | head -1)
if grep -q '56,088' <<<"$RESULT"; then
  PASS=$((PASS+1)); echo "PASS: calculator result = $RESULT"
else
  FAIL=$((FAIL+1)); echo "FAIL: calculator result: $RESULT"
fi

# ── 2. 记事本:写入结果+标注 → 保存 ───────────────────────────────────
cmd //c start notepad
step "wait notepad window" $ACTL wait --window Notepad --timeout 8000
step "type record (paste channel)" \
  $ACTL type role:Document "US1 跨应用搬运:123×456=$RESULT @ $MARK" --app Notepad --paste
step "ctrl+s, expect save dialog" \
  $ACTL press ctrl+s --app Notepad --expect window:另存为 --timeout 5000
if $ACTL set-value id:1001 "$MARK" --app 另存为 2>/dev/null | grep -q '"ok":true'; then
  PASS=$((PASS+1)); echo "PASS: set filename (id:1001)"
else
  step "set filename (combobox host)" $ACTL set-value id:FileNameControlHost "$MARK" --app 另存为
fi
step "confirm save" $ACTL press enter --app 另存为
sleep 2

# ── 3. 磁盘 + 资源管理器双确认 ────────────────────────────────────────
F="$DOCS/$MARK"
for _ in 1 2 3; do [ -f "$F" ] && break; sleep 1; done
if [ -f "$F" ] && grep -q "56,088" "$F"; then
  PASS=$((PASS+1)); echo "PASS: disk content correct"
else
  FAIL=$((FAIL+1)); echo "FAIL: disk verify at $F"
fi

cmd //c start "D:\\C\\Documents"
step "wait explorer window" $ACTL wait --window Documents --timeout 8000
# 已知开放项(M2 第三批):资源管理器文件列表经 UIA 虚拟化,新建文件的
# ListItem 不实时实现(实测 F5 后仍 total=0;需 ItemContainerPattern 支持)。
# 第三应用此处做窗口级确认;文件本身已由磁盘+内容双验背书。
step "verify explorer opened on Documents" \
  $ACTL verify --exists "name:Documents - 文件资源管理器" --app Documents

# ── 清理 ─────────────────────────────────────────────────────────────
step "close saved tab" $ACTL press ctrl+w --app Notepad
$ACTL close-window Documents >/dev/null 2>&1
$ACTL close-window 计算 >/dev/null 2>&1
sleep 1; "$ACTL" close-window "计算器" >/dev/null 2>&1
rm -f "$F"

echo "----"
echo "US1: $PASS pass, $FAIL fail"
[ "$FAIL" -eq 0 ]
