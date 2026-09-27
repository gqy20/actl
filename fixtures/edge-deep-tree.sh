#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
exec pwsh -NoProfile -File fixtures/edge-deep-tree.ps1 -Actl "${ACTL:-target/validation/release/actl.exe}"
