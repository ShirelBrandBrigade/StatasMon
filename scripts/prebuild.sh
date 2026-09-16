#!/usr/bin/env bash
# tauri.conf.json beforeBuildCommand: bundle adb, then build the frontend.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
bash "$here/scripts/fetch-platform-tools.sh"
pnpm --filter @statasmon/desktop build
