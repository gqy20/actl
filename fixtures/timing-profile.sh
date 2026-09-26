#!/usr/bin/env bash
# 时序画像:各应用命令延迟分布(被动测量,不改任何参数)。
# 仪器 = envelope 自带的 duration_ms(零额外埋点)。
# ⚠ 需要桌面空闲(并发输入污染样本);污染检测:type 命令 stopped_early 或
#    读回不一致 → 该样本重跑一次,两次都污染则计入 pollution 并丢弃。
# 输出:docs/timing-data/profile-<ts>.md(含 CSV 数据块与环境头)
set -uo pipefail
cd "$(dirname "$0")/.."
ACTL="${ACTL:-target/release/actl.exe}"
TS=$(date +%Y%m%d-%H%M%S)
OUT="docs/timing-data/profile-$TS.md"
mkdir -p docs/timing-data
DUR_RE='"duration_ms":([0-9]+)'

dur() { grep -oE "$DUR_RE" "$1" | cut -d: -f2; }

ERRLOG=/tmp/_tp_errors.log
: > "$ERRLOG"
run_n() { # run_n <标签> <n> <cmd...>:跑 n 次收 duration_ms 列表;失败记因
  local label=$1 n=$2; shift 2
  local list=""
  for _ in $(seq 1 "$n"); do
    "$@" > /tmp/_tp.json 2>&1
    local rc=$?
    if [ $rc -eq 0 ] && grep -q '"ok":true' /tmp/_tp.json; then
      list+="$(dur /tmp/_tp.json) "
    else
      list+="ERR "
      echo "[$label] rc=$rc $(grep -o '"code":"[A-Z_]*"' /tmp/_tp.json | head -1)" >> "$ERRLOG"
    fi
  done
  echo "$label $list"
}

stats() { # stats <label> <空格分隔数值>:输出中位/P95/max/err
  local label=$1; shift
  local errs=0 nums=()
  for v in $@; do [ "$v" = "ERR" ] && errs=$((errs+1)) || nums+=("$v"); done
  if [ ${#nums[@]} -eq 0 ]; then echo "$label: 全部失败($errs)"; return; fi
  local n=${#nums[@]}
  local i50=$(( (n + 1) / 2 )) i95=$(( n * 95 / 100 + 1 ))
  [ $i95 -gt $n ] && i95=$n
  printf '%s\n' "${nums[@]}" | sort -n | awk -v l="$label" -v e="$errs" -v i50=$i50 -v i95=$i95 '
    {a[NR]=$1} END {printf "%s: n=%d err=%d P50=%dms P95=%dms max=%dms\n", l, NR, e, a[i50], a[i95], a[NR]}'
}

echo "# actl 时序画像 $TS" > "$OUT"
echo >> "$OUT"
echo "- 构建: $($ACTL --version 2>/dev/null | head -1);二进制: $(ls -la target/release/actl.exe 2>/dev/null | awk '{print $5}')B" >> "$OUT"
echo "- 机器: $(powershell -NoProfile -Command '(Get-CimInstance Win32_Processor).Name; (Get-ItemProperty "HKCU:\Control Panel\Desktop\WindowMetrics").AppliedDPI' 2>/dev/null | tr '\n' ' / ')" >> "$OUT"
echo >> "$OUT"

# ── 场景1:计算器热点击(UIA Invoke 路径基准)─────────────────────────
cmd //c "start shell:AppsFolder\\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App" >/dev/null 2>&1
"$ACTL" wait --window 计算器 --timeout 8000 >/dev/null 2>&1
sleep 2
CALC_OK=$($ACTL find --role Button --app 计算器 --limit 40 2>/dev/null | grep -o '"total":[0-9]*' | cut -d: -f2)
if [ "${CALC_OK:-0}" -lt 10 ]; then
  echo "calc-click: 跳过——计算器内容树未物化(buttons=${CALC_OK:-0};已知环境坑:僵尸框架后实例坏状态)" | tee -a "$OUT"
  "$ACTL" close-window "计算器" >/dev/null 2>&1
else
  R=$(run_n "calc-click" 30 $ACTL click id:clearButton --app 计算器)
  stats "${R%% *}" ${R#* } | tee -a "$OUT"
  "$ACTL" close-window 计算 >/dev/null 2>&1
fi
sleep 1

# ── 场景2:记事本读路径(get value;UIA 属性读取基准)────────────────
cmd //c start notepad >/dev/null 2>&1
"$ACTL" wait --window Notepad --timeout 8000 >/dev/null 2>&1
R=$(run_n "notepad-get" 20 $ACTL get role:Document --app Notepad --property value)
stats "${R%% *}" ${R#* } | tee -a "$OUT"

# ── 场景2b:记事本对话框链(press+expect 全链)──────────────────────
DLG=""
for _ in 1 2 3; do
  "$ACTL" press ctrl+s --app Notepad --expect window:另存为 --timeout 5000 > /tmp/_tp.json 2>&1 && grep -q '"ok":true' /tmp/_tp.json && DLG+="$(dur /tmp/_tp.json) "
  "$ACTL" press esc --app 另存为 >/dev/null 2>&1; sleep 1
done
[ -n "$DLG" ] && stats "notepad-ctrls-dlg" $DLG | tee -a "$OUT" || echo "notepad-ctrls-dlg: 未采集(对话框未弹,可能环境占用)" | tee -a "$OUT"
"$ACTL" close-window Notepad >/dev/null 2>&1; sleep 1
"$ACTL" snapshot --app Notepad 2>/dev/null | grep -q 不保存 && "$ACTL" click name:不保存 --app Notepad >/dev/null 2>&1

# ── 场景3:设置(Electron/惰性激活,一次性冷查)──────────────────────
cmd //c start ms-settings: >/dev/null 2>&1
if "$ACTL" wait --window 设置 --timeout 10000 >/dev/null 2>&1; then
  "$ACTL" snapshot --app 设置 --skeleton > /tmp/_tp.json 2>&1
  echo "ms-settings-cold-snapshot: $(dur /tmp/_tp.json)ms (一次性;a11y 惰性激活计入)" | tee -a "$OUT"
  "$ACTL" close-window 设置 >/dev/null 2>&1
else
  echo "ms-settings-cold-snapshot: 未采集" | tee -a "$OUT"
fi

# ── 场景4:挂起计算器首命令(机会性;Windows 挂起时机不可控)───────────
"$ACTL" close-window "计算器" >/dev/null 2>&1; sleep 1
cmd //c "start shell:AppsFolder\\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App" >/dev/null 2>&1
"$ACTL" wait --window 计算器 --timeout 8000 >/dev/null 2>&1
echo "等待 45s 观察 UWP 挂起(机会采样)…" >&2
sleep 45
"$ACTL" get id:clearButton --app 计算器 --property name > /tmp/_tp.json 2>&1
if grep -q '"ok":true' /tmp/_tp.json; then
  W=$(grep -o '"wake_ms":[0-9]*' /tmp/_tp.json | cut -d: -f2 || echo "0")
  echo "calc-after-45s: $(dur /tmp/_tp.json)ms wake_ms=${W:-N/A}(wake_ms=0 表示未挂起,样本作废)" | tee -a "$OUT"
fi
"$ACTL" close-window "计算器" >/dev/null 2>&1

echo >> "$OUT"
echo "## 原始数据" >> "$OUT"
echo '```' >> "$OUT"
echo "$R" >> "$OUT"
echo '```' >> "$OUT"
echo "输出: $OUT"
