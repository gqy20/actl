#!/usr/bin/env bash
# Compatibility entry; preserve the selected validation binary.
set -euo pipefail
cd "$(dirname "$0")/.."
ACTL="${ACTL:-target/validation/release/actl.exe}"
exec pwsh -NoProfile -File fixtures/win32-table-extract.ps1 -Actl "$ACTL"
