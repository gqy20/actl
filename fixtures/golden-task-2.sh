#!/usr/bin/env bash
# 金任务 2:键盘输入 → 组合键 → 系统另存为对话框 → UIA 写值 → 磁盘验证。
# 覆盖能力:type(键盘文本)、press(组合键)、--expect 窗口断言、owned dialog
# 可见性、set-value(ValuePattern)、跨窗口护栏。
# 前置:可交互 Windows 桌面会话;Win11 新记事本(单实例多标签)。
# ⚠ 需要桌面空闲:本流程多次抢前台,若有人在打字,其键入会与注入交错进编辑器
#   (2026-09-26 四次实测:并发输入下 readback 出现重复字母/无关字符群)。
# 安全:所有注入都带 --app(verify-then-inject 护栏),键只进 Notepad/另存为。
set -uo pipefail
cd "$(dirname "$0")/.."
ACTL=./target/debug/actl.exe
PASS=0; FAIL=0

step() { # step <desc> <cmd...>
  local desc="$1"; shift
  local out
  if out=$("$@" 2>&1) && grep -q '"ok":true' <<<"$out"; then
    PASS=$((PASS+1)); echo "PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "FAIL: $desc"; head -c 300 <<<"$out"; echo
  fi
}

DOCS=$(powershell -NoProfile -Command '[Environment]::GetFolderPath("MyDocuments")' | tr -d '\r')
DOCS="${DOCS//\\//}" # 统一为正斜杠,避免与 $MARK 拼接出混合分隔路径
MARK="actl_golden2_$(date +%H%M%S).txt"
TEXT="golden task 2: keyboard + dialog + value pattern"

# ── 失败安全清理 + 前置断言(同金任务 1 模板;GUI 禁 taskkill——记事本会话复活)──
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
cleanup() { close_all_by_title "Notepad"; }
trap cleanup EXIT
WINCNT=$("$ACTL" list-windows 2>/dev/null | grep -o "{[^{}]*\"title\":\"[^\"]*Notepad[^\"]*\"[^{}]*}" | wc -l)
if [ "$WINCNT" -gt 0 ]; then
  echo "ENV-DIRTY: Notepad 窗口已存在 ×$WINCNT——已清理,请重跑本 fixture"; exit 1
fi
# 0. 确保 Notepad 在(有窗口就复用)
$ACTL list-windows 2>/dev/null | grep -q Notepad || cmd //c start notepad
sleep 1

# 1. 前置窗口(键盘命令的目标绑定)
step "focus-window Notepad" \
  $ACTL focus-window Notepad

# 2. 新标签:未命名文档,保证 ctrl+s 必弹另存为(已命名文档会静默保存)
step "press ctrl+n (new untitled tab)" \
  $ACTL press ctrl+n --app Notepad
sleep 1

# 3. 键盘输入 golden 文本(--paste:剪贴板+ctrl+v 通道,中文模式 IME 下唯一
#    可靠的文本送达方式;直注 UNICODE 会被 IME 组合改写,战记实测)
step "type marker text into editor (paste channel)" \
  $ACTL type role:Document "$TEXT" --app Notepad --paste
sleep 1

# 4. 读回验证:UIA Value 必须等于打入的文本(不信标题,信内容);新标签的
#    ValueProvider 偶尔迟于粘贴就绪,重试一次再判
v=""
for _ in 1 2; do
  v=$($ACTL get role:Document --app Notepad --property value 2>&1)
  grep -q "\"value\":\"$TEXT\"" <<<"$v" && break
  sleep 1
done
if grep -q "\"value\":\"$TEXT\"" <<<"$v"; then
  PASS=$((PASS+1)); echo "PASS: readback equals typed text"
else
  FAIL=$((FAIL+1)); echo "FAIL: readback mismatch"; head -c 300 <<<"$v"; echo
fi

# 5. ctrl+s:组合键 → 另存为对话框必须**新出现**(--expect RuntimeId 基线断言)
step "press ctrl+s, expect NEW 另存为 dialog" \
  $ACTL press ctrl+s --app Notepad --expect window:另存为 --timeout 5000

# 6. 文件名写入:文件名 Edit(id 1001);ValuePattern 失败则宿主 ComboBox 兜底
if $ACTL set-value id:1001 "$MARK" --app 另存为 2>/dev/null | grep -q '"ok":true'; then
  PASS=$((PASS+1)); echo "PASS: set filename (edit value pattern)"
else
  step "set filename (combobox host fallback)" \
    $ACTL set-value id:FileNameControlHost "$MARK" --app 另存为
fi

# 7. enter 确认 → 对话框关闭、文档已命名
step "press enter to confirm save" \
  $ACTL press enter --app 另存为
sleep 2

# 8. 后置状态:对话框已消失,且 Notepad 标题变为文件名(保存成功的窗口级证据)
if ! $ACTL list-windows 2>/dev/null | grep -q 另存为 \
   && $ACTL list-windows 2>/dev/null | grep -q "\"title\":\"$MARK - Notepad\""; then
  PASS=$((PASS+1)); echo "PASS: dialog closed and tab retitled to saved file"
else
  FAIL=$((FAIL+1)); echo "FAIL: post-save window state"
fi

# 9. 磁盘终验:文件存在且内容逐字一致
F="$DOCS/$MARK"
for _ in 1 2 3 4 5; do [ -f "$F" ] && break; sleep 1; done
if [ -f "$F" ] && [ "$(cat "$F")" = "$TEXT" ]; then
  PASS=$((PASS+1)); echo "PASS: file on disk with exact content ($F)"
else
  FAIL=$((FAIL+1)); echo "FAIL: disk verify at $F"
fi

# 10. 清理:关标签(刚保存,直接关) + 删文件,保持可重复运行
step "press ctrl+w close saved tab" \
  $ACTL press ctrl+w --app Notepad
rm -f "$F"

echo "----"
echo "golden-task-2: $PASS pass, $FAIL fail"
[ "$FAIL" -eq 0 ]
