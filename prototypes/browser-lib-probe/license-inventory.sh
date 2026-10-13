#!/bin/bash
# License inventory: probe dep trees + actl workspace baseline.
# Reads real license fields from registry manifests (ground truth, not memory).
REG=$(ls -d ~/.cargo/registry/src/*/ | head -1)
EV="E:/explore/computer_cli-spike/target/browser-lib-probe-20261013"

inventory() {
  local lock="$1" label="$2"
  echo "=== $label ==="
  awk '
    /^name = / { gsub(/"/,""); n=$3 }
    /^version = / { gsub(/"/,""); if (n != "") { print n " " $3; n="" } }
  ' "$lock" | while read -r name ver; do
    local ct="${REG}${name}-${ver}/Cargo.toml"
    if [ -f "$ct" ]; then
      local lic
      lic=$(grep -m1 '^license' "$ct" | sed 's/^license *= *//; s/"//g; s/^license-file *= *//' )
      [ -z "$lic" ] && lic="(none declared)"
      echo "$name $ver :: $lic"
    else
      echo "$name $ver :: (local/no-manifest)"
    fi
  done
}

{
  inventory "E:/explore/computer_cli-spike/prototypes/browser-lib-probe/chromiumoxide-probe/Cargo.lock" "chromiumoxide tree"
  echo ""
  inventory "E:/explore/computer_cli-spike/prototypes/browser-lib-probe/headless-chrome-probe/Cargo.lock" "headless_chrome tree"
  echo ""
  inventory "E:/explore/computer_cli-spike/Cargo.lock" "actl workspace baseline (current)"
} | tee "$EV/license-inventory.txt"

echo ""
echo "=== license class aggregation ==="
for tree in chromiumoxide-probe headless-chrome-probe; do
  echo "--- $tree ---"
  awk '
    /^name = / { gsub(/"/,""); n=$3 }
    /^version = / { gsub(/"/,""); if (n != "") { print n " " $3; n="" } }
  ' "E:/explore/computer_cli-spike/prototypes/browser-lib-probe/$tree/Cargo.lock" | while read -r name ver; do
    ct="${REG}${name}-${ver}/Cargo.toml"
    [ -f "$ct" ] && grep -m1 '^license' "$ct" | sed 's/^license *= *//; s/"//g'
  done | grep -v '^$' | sort | uniq -c | sort -rn
done
echo "--- actl baseline ---"
awk '
  /^name = / { gsub(/"/,""); n=$3 }
  /^version = / { gsub(/"/,""); if (n != "") { print n " " $3; n="" } }
' "E:/explore/computer_cli-spike/Cargo.lock" | while read -r name ver; do
  ct="${REG}${name}-${ver}/Cargo.toml"
  [ -f "$ct" ] && grep -m1 '^license' "$ct" | sed 's/^license *= *//; s/"//g'
done | grep -v '^$' | sort | uniq -c | sort -rn
