#!/usr/bin/env bash
# 验收 · xy: 坐标寻址(spec):窗口边界守卫 + 坐标点击端到端。
# ① 正路径:记事本(系统自带)窗口 rect 中心点 → click xy --physical →
#    点击落焦 → type --paste → verify 文本落位(坐标点击真实生效);
# ② 守卫负测试:窗外坐标 → NOT_ACTIONABLE 拒绝(不注入);
# ③ 协议门由 cli_contract 离线覆盖。含键盘注入:需交互桌面空闲。
set -uo pipefail
cd "$(dirname "$0")/.."
ACTL="${ACTL:-target/release/actl.exe}"
PASS=0; FAIL=0
APP="Notepad"

cleanup() {
  "$ACTL" close-window "$APP" >/dev/null 2>&1
}
trap cleanup EXIT

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

cmd //c start notepad
step "wait notepad window" $ACTL wait --window "$APP" --timeout 12000

# 窗口 rect(list-windows 物理层事实)→ 中心点
RECT=$($ACTL list-windows 2>/dev/null | sed 's/},{/\n/g' | grep Notepad | grep -o '"rect":\[[^]]*\]' | head -1)
py - "$RECT" /tmp/coord_pt.txt <<'EOF'
import json, sys
# list-windows 的 rect 是 [left, top, right, bottom] 数组(物理像素)
l, t, r, b = json.loads(sys.argv[1].split(':', 1)[1])
cx, cy = (l + r) // 2, (t + b) // 2
open(sys.argv[2], 'w').write(f"{cx},{cy}")
print(f"  window rect center: ({cx},{cy}) from [{l},{t},{r},{b}]")
EOF
PT=$(cat /tmp/coord_pt.txt)
X=${PT%,*}; Y=${PT#*,}

step "coordinate click at window center" $ACTL click "xy:$X,$Y" --app "$APP" --physical
step "type after coordinate click (focus landed)" $ACTL type role:Document "hello-xy" --app "$APP" --paste
step "verify text landed" $ACTL verify --value role:Document "hello-xy" --contains --app "$APP"

# 守卫负测试:窗外坐标(右移 20000px)→ NOT_ACTIONABLE,不注入
OUT=$($ACTL click "xy:$((X+20000)),$Y" --app "$APP" --physical 2>&1)
if grep -q '"code":"NOT_ACTIONABLE"' <<<"$OUT" && grep -q "outside target window bounds" <<<"$OUT"; then
  PASS=$((PASS+1)); echo "PASS: out-of-bounds coordinate refused (window-bounds guard)"
else
  FAIL=$((FAIL+1)); echo "FAIL: out-of-bounds not refused:"; head -c 300 <<<"$OUT"; echo
fi

echo "──"
echo "coord-click: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
