#!/usr/bin/env bash
# Downloads Google's Android platform-tools for the build host and keeps only
# adb (plus its Windows DLLs and the NOTICE) under src-tauri/resources/
# platform-tools/, which tauri.conf.json bundles into the app. The folder is
# gitignored: the zip is fetched at build time, never committed (ADR-021).
#
#   scripts/fetch-platform-tools.sh            # host OS
#   scripts/fetch-platform-tools.sh windows    # darwin | windows | linux
#   STATASMON_SKIP_ADB_BUNDLE=1                # build without adb inside
set -euo pipefail

here="$(cd "$(dirname "$0")/.." && pwd)"
dest="$here/src-tauri/resources/platform-tools"

if [[ "${STATASMON_SKIP_ADB_BUNDLE:-}" == "1" ]]; then
  echo "fetch-platform-tools: skipped (STATASMON_SKIP_ADB_BUNDLE=1)"
  mkdir -p "$dest"
  exit 0
fi

os="${1:-}"
if [[ -z "$os" ]]; then
  case "$(uname -s)" in
    Darwin) os=darwin ;;
    Linux) os=linux ;;
    MINGW*|MSYS*|CYGWIN*|Windows_NT) os=windows ;;
    *) echo "fetch-platform-tools: unknown host $(uname -s)" >&2; exit 1 ;;
  esac
fi

adb="adb"; [[ "$os" == "windows" ]] && adb="adb.exe"
if [[ -f "$dest/$adb" && -f "$dest/source.properties" ]]; then
  echo "fetch-platform-tools: $dest/$adb present ($(grep -m1 Pkg.Revision "$dest/source.properties" || true))"
  exit 0
fi

url="https://dl.google.com/android/repository/platform-tools-latest-${os}.zip"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
echo "fetch-platform-tools: downloading $url"
curl -fsSL --retry 3 -o "$tmp/pt.zip" "$url"
unzip -q "$tmp/pt.zip" -d "$tmp"
rm -rf "$dest"; mkdir -p "$dest"
for f in "$adb" AdbWinApi.dll AdbWinUsbApi.dll NOTICE.txt source.properties; do
  [[ -f "$tmp/platform-tools/$f" ]] && cp "$tmp/platform-tools/$f" "$dest/$f"
done
[[ "$os" != "windows" ]] && chmod 755 "$dest/$adb"
cat > "$dest/README.txt" <<'TXT'
adb from Google's Android SDK platform-tools, bundled so StatasMon needs
no separate download. Google's terms for the SDK platform-tools apply
(https://developer.android.com/studio/terms); NOTICE.txt lists the
open-source licences of the components. source.properties has the version.
TXT
echo "fetch-platform-tools: bundled $(grep -m1 Pkg.Revision "$dest/source.properties" || echo 'unknown version') into $dest"
