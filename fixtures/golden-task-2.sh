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

# 0. 确保 Notepad 在(有窗口就复用)
$ACTL list-windows 2>/dev/null | grep -q Notepad || cmd //c start notepad
sleep 1

# 1. 新标签:未命名文档,保证 ctrl+s 必弹另存为(已命名文档会静默保存)
step "press ctrl+n (new untitled tab)" \
  $ACTL press ctrl+n --app Notepad
sleep 1

# 2. 键盘输入 golden 文本(元素置焦 + 护栏 + VK/UNICODE 管线)
step "type marker text into editor" \
  $ACTL type role:Document "$TEXT" --app Notepad

# 3. 读回验证:UIA Value 必须等于打入的文本(不信标题,信内容)
if v=$($ACTL get role:Document --app Notepad --property value 2>&1) \
   && grep -q "\"value\":\"$TEXT\"" <<<"$v"; then
  PASS=$((PASS+1)); echo "PASS: readback equals typed text"
else
  FAIL=$((FAIL+1)); echo "FAIL: readback mismatch"; head -c 300 <<<"$v"; echo
fi

# 4. ctrl+s:组合键 → 另存为对话框必须出现(--expect 轮询断言)
step "press ctrl+s, expect 另存为 dialog" \
  $ACTL press ctrl+s --app Notepad --expect window:另存为 --timeout 5000

# 5. 文件名写入:文件名 Edit(id 1001);ValuePattern 失败则宿主 ComboBox 兜底
if $ACTL set-value id:1001 "$MARK" --app 另存为 2>/dev/null | grep -q '"ok":true'; then
  PASS=$((PASS+1)); echo "PASS: set filename (edit value pattern)"
else
  step "set filename (combobox host fallback)" \
    $ACTL set-value id:FileNameControlHost "$MARK" --app 另存为
fi

# 6. enter 确认 → 对话框关闭、文档已命名
step "press enter to confirm save" \
  $ACTL press enter --app 另存为
sleep 2

# 7. 磁盘终验:文件存在且内容逐字一致
F="$DOCS/$MARK"
for _ in 1 2 3 4 5; do [ -f "$F" ] && break; sleep 1; done
if [ -f "$F" ] && [ "$(cat "$F")" = "$TEXT" ]; then
  PASS=$((PASS+1)); echo "PASS: file on disk with exact content ($F)"
else
  FAIL=$((FAIL+1)); echo "FAIL: disk verify at $F"
fi

# 8. 清理:关标签(刚保存,直接关) + 删文件,保持可重复运行
$ACTL press ctrl+w --app Notepad >/dev/null 2>&1 || true
rm -f "$F"

echo "----"
echo "golden-task-2: $PASS pass, $FAIL fail"
[ "$FAIL" -eq 0 ]
