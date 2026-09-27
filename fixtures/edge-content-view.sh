#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
exec pwsh -NoProfile -File fixtures/edge-content-view.ps1 -Actl "${ACTL:-target/validation/release/actl.exe}"
