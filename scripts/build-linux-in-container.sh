#!/usr/bin/env bash
# Runs inside ubuntu:22.04 (linux/amd64), started by scripts/build-all.sh.
# /src is the repository (read-only), /build a persistent volume (toolchains,
# dependencies and target dir survive between runs), /out receives installers.
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
# Toolchains live on the persistent volume; HOME stays the container's own
# (rustup refuses a HOME that is not the user's).
export RUSTUP_HOME=/build/rustup CARGO_HOME=/build/cargo TOOLS=/build/tools
mkdir -p "$TOOLS" /build/work
if [[ ! -f /build/.apt-done ]]; then
  apt-get update -qq
  apt-get install -y -qq curl unzip git build-essential pkg-config file rsync ca-certificates \
    libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libssl-dev xdg-utils >/dev/null
  touch /build/.apt-done
else
  # The container is fresh every run; the packages are not. Reinstall quietly from the apt cache.
  apt-get update -qq
  apt-get install -y -qq curl unzip git build-essential pkg-config file rsync ca-certificates \
    libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libssl-dev xdg-utils >/dev/null
fi
if [[ ! -x "$CARGO_HOME/bin/cargo" ]]; then
  # The shell installer cannot detect the platform under Rosetta; fetch the binary.
  curl -fsSL -o /tmp/rustup-init https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init
  chmod +x /tmp/rustup-init
  /tmp/rustup-init -y --no-modify-path --profile minimal --default-host x86_64-unknown-linux-gnu --default-toolchain none >/dev/null
fi
export PATH="$CARGO_HOME/bin:$TOOLS/node/bin:$PATH"
if [[ ! -x "$TOOLS/node/bin/node" ]]; then
  mkdir -p "$TOOLS/node"
  curl -fsSL https://nodejs.org/dist/v22.11.0/node-v22.11.0-linux-x64.tar.gz | tar -xz --strip-components=1 -C "$TOOLS/node"
fi
# Only the Tauri CLI is needed here: the web part is built on the Mac (it is
# the same on every system, and pnpm crashes under x86 emulation).
if [[ ! -x "$TOOLS/cli/node_modules/.bin/tauri" || "$(cat "$TOOLS/cli/.version" 2>/dev/null)" != "$TAURI_CLI_VERSION" ]]; then
  rm -rf "$TOOLS/cli" && mkdir -p "$TOOLS/cli"
  (cd "$TOOLS/cli" && npm init -y >/dev/null && npm install --no-audit --no-fund "@tauri-apps/cli@$TAURI_CLI_VERSION" >/dev/null)
  echo "$TAURI_CLI_VERSION" > "$TOOLS/cli/.version"
fi
# Work on a copy: the source is read-only and its node_modules/target are the Mac's.
rsync -a --delete --exclude node_modules --exclude /target --exclude /dist --exclude .git \
  --exclude test-results --exclude playwright-report --exclude .DS_Store \
  --exclude src-tauri/resources/platform-tools /src/ /build/work/ \
  || { code=$?; [[ $code -eq 23 || $code -eq 24 ]] || exit $code; }
cd /build/work
[[ -f apps/desktop/dist/index.html ]] || { echo "apps/desktop/dist is missing: build the web part on the host first" >&2; exit 1; }
mkdir -p src-tauri/resources/platform-tools
cp /src/src-tauri/resources/platform-tools/README.md src-tauri/resources/platform-tools/
export CARGO_TARGET_DIR=/build/target CI=true APPIMAGE_EXTRACT_AND_RUN=1
STATASMON_ADB_OS=linux bash scripts/fetch-platform-tools.sh
echo "{\"version\":\"$VERSION\",\"build\":{\"beforeBuildCommand\":\"\"},\"bundle\":{\"createUpdaterArtifacts\":false}}" > /build/overrides.json
cd src-tauri
"$TOOLS/cli/node_modules/.bin/tauri" build --bundles appimage,deb --config /build/overrides.json
cp /build/target/release/bundle/appimage/*.AppImage /build/target/release/bundle/deb/*.deb /out/
ls -lh /out
