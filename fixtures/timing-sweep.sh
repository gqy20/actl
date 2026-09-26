#!/usr/bin/env bash
# 时序参数扫描:回答"哪个速度更适合"。前置 = timing 环境旋钮(免重编译)。
# Q2 轮询间隔(wait 检出延迟) · Q3 分批大小(type 吞吐) · Q5 焦点链(press 成功率)
# ⚠ 需要桌面解锁且空闲(锁屏下注入被护栏拒绝是正确行为,数据无意义)。
# 输出:docs/timing-data/sweep-<ts>.md
set -uo pipefail
cd "$(dirname "$0")/.."
ACTL="${ACTL:-target/release/actl.exe}"
TS=$(date +%Y%m%d-%H%M%S)
OUT="docs/timing-data/sweep-$TS.md"
mkdir -p docs/timing-data
DUR_RE='"duration_ms":([0-9]+)'

echo "# actl 时序扫描 $TS" > "$OUT"
echo >> "$OUT"
LOCKED=$(./target/release/actl.exe press shift --app __nope__ 2>&1 | grep -c "锁屏\|锁屏界面" || true)
if ./target/release/actl.exe list-windows 2>/dev/null | grep -q "锁屏"; then
  echo "**环境:检测到锁屏,注入场景数据无效——请在解锁状态重跑**" | tee -a "$OUT"
fi
echo >> "$OUT"

dur() { grep -oE "$DUR_RE" "$1" | cut -d: -f2; }

median() { printf '%s\n' "$@" | sort -n | awk '{a[NR]=$1} END {print a[int((NR+1)/2)]}'; }

# ── Q2:轮询间隔 → wait --window 检出延迟 ──────────────────────────────
# 构造确定延迟事件:后台延时 1.5s 启动记事本,测 wait 从发令到检出(应≈1500+轮询粒度)
echo "## Q2 轮询间隔 × wait 检出延迟(理论下限 = 间隔/2)" | tee -a "$OUT"
"$ACTL" close-window Notepad >/dev/null 2>&1; sleep 1
for P in 50 100 250; do
  TIMES=""
  for _ in 1 2 3 4 5; do
    "$ACTL" close-window Notepad >/dev/null 2>&1; sleep 1
    ( sleep 1.5 && cmd //c start notepad >/dev/null 2>&1 ) &
    ACTL_POLL_MS=$P "$ACTL" wait --window Notepad --timeout 10000 > /tmp/_sw.json 2>&1
    if grep -q '"ok":true' /tmp/_sw.json; then
      t=$(dur /tmp/_sw.json); TIMES+=$(( t - 1500 ))" "   # 减去构造延迟≈检出开销
    fi
  done
  wait
  [ -n "$TIMES" ] && echo "poll=${P}ms: 中位检出开销 $(median $TIMES)ms(样本: $TIMES)" | tee -a "$OUT" \
                   || echo "poll=${P}ms: 无有效样本" | tee -a "$OUT"
done
"$ACTL" close-window Notepad >/dev/null 2>&1

# ── Q3:分批大小 → type 键盘通道吞吐(300 字符)────────────────────────
echo >> "$OUT"; echo "## Q3 分批大小 × type 吞吐(300 字符键盘通道;停止粒度=批大小)" | tee -a "$OUT"
cmd //c start notepad >/dev/null 2>&1; "$ACTL" wait --window Notepad --timeout 8000 >/dev/null 2>&1; sleep 1
BODY=$(printf 'x%.0s' $(seq 1 300))
for SEG in 16 32 64 128; do
  TIMES=""
  for _ in 1 2 3; do
    "$ACTL" press ctrl+a --app Notepad >/dev/null 2>&1; sleep 0.3
    ACTL_SEGMENT_CHARS=$SEG "$ACTL" type role:Document "$BODY" --app Notepad > /tmp/_sw.json 2>&1
    if grep -q '"ok":true' /tmp/_sw.json && grep -q '"delivered":300' /tmp/_sw.json; then
      TIMES+="$(dur /tmp/_sw.json) "
    fi
  done
  [ -n "$TIMES" ] && echo "segment=${SEG}: 中位 $(median $TIMES)ms / 300 字符(样本: $TIMES)" | tee -a "$OUT" \
                   || echo "segment=${SEG}: 无有效样本(污染或注入被拒)" | tee -a "$OUT"
done

# ── Q5:焦点链参数 × press 成功率 ─────────────────────────────────────
echo >> "$OUT"; echo "## Q5 焦点链(press_focus_ms × retries)× 成功率(每格 10 次 press)" | tee -a "$OUT"
for CFG in "60 3" "120 2" "120 3" "150 3"; do
  set -- $CFG; FMS=$1; RET=$2
  OK=0 FAIL=0
  for _ in $(seq 1 10); do
    if ACTL_PRESS_FOCUS_MS=$FMS ACTL_FOCUS_RETRIES=$RET "$ACTL" press f12 --app Notepad >/dev/null 2>&1; then
      OK=$((OK+1))
    else
      FAIL=$((FAIL+1))
    fi
    sleep 0.2
  done
  echo "focus=${FMS}ms retries=${RET}: ok=${OK}/10" | tee -a "$OUT"
done
"$ACTL" close-window Notepad >/dev/null 2>&1; sleep 1
"$ACTL" snapshot --app Notepad 2>/dev/null | grep -q 不保存 && "$ACTL" click name:不保存 --app Notepad >/dev/null 2>&1

echo >> "$OUT"
echo "扫描完成: $OUT"
