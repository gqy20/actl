#!/usr/bin/env bash
# 验收 A · 虚拟化列表条目级定位(P0:ItemContainerPattern L4 层):
# 61 个文件的资源管理器目录,目标文件按字母序排在首屏外(未物化)——
# walker 遍历恒 0 命中(US1 挂账的虚拟化黑洞),定位链 L4 经
# ItemContainerPattern::FindItemByProperty 精确命中。
# 场景对齐 US1:"保存文件后在 Explorer 里确认它存在"(late-create 模拟刚保存)。
# 依赖:系统自带资源管理器;无需桌面空闲(纯观察,零注入)。
set -uo pipefail
cd "$(dirname "$0")/.."
ACTL="${ACTL:-target/release/actl.exe}"
PASS=0; FAIL=0
STAMP=$(date +%H%M%S)
DIR_NAME="actl-vlist-$STAMP"
# $LOCALAPPDATA 为反斜杠 Windows 路径:bash 侧操作转正斜杠,Win32 调用保留反斜杠
DIR_UX="${LOCALAPPDATA//\\//}/Temp/$DIR_NAME"
DIR_WIN="$LOCALAPPDATA\\Temp\\$DIR_NAME"
TARGET="zzz_late_$STAMP.txt"

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

# ── 1. 准备:60 个 padding(目标按字母序必在首屏外)+ 开窗 ───────────────
rm -rf "$DIR_UX"; mkdir -p "$DIR_UX"
for i in $(seq -w 1 60); do echo "pad $i" > "$DIR_UX/f${i}_padding.txt"; done
explorer "$DIR_WIN" &
step "wait explorer window" $ACTL wait --window "$DIR_NAME" --timeout 8000

# ── 2. late-create:模拟"刚保存的文件"(窗口已开,树已成型) ─────────────
echo "late content" > "$DIR_UX/$TARGET"

# ── 3. L4 定位:find 应落 item-container 层并命中 ──────────────────────
step "find late file (virtualized)" $ACTL find --name "$TARGET" --app "$DIR_NAME"
assert_contains "resolved_by = item-container" '"resolved_by":"item-container"'
assert_contains "element name reported" "\"$TARGET\""

# ── 4. 定位链全通:get / verify 继承 L4 ────────────────────────────────
step "get properties via L4" $ACTL get "name:$TARGET" --app "$DIR_NAME" --property name
assert_contains "get returns the file name" "\"$TARGET\""
step "verify exists via L4" $ACTL verify --exists "name:$TARGET" --app "$DIR_NAME"

# ── 5. 清理:关窗(优雅)+ 删目录 ───────────────────────────────────────
step "close explorer window" $ACTL close-window "$DIR_NAME"
sleep 1
rm -rf "$DIR_UX"

echo "──"
echo "virtualized-list: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
