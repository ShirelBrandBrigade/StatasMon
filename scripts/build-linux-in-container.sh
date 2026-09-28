#!/usr/bin/env bash
# Runs inside ubuntu:22.04 (linux/amd64), started by scripts/build-all.sh.
# /src is the repository (read-only), /build a persistent volume (toolchains,
# dependencies and target dir survive between runs), /out receives installers.
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive HOME=/build/home
mkdir -p "$HOME" /build/work
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
if [[ ! -x "$HOME/.cargo/bin/cargo" ]]; then
  curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none >/dev/null
fi
export PATH="$HOME/.cargo/bin:$HOME/node/bin:$PATH"
if [[ ! -x "$HOME/node/bin/node" ]]; then
  mkdir -p "$HOME/node"
  curl -fsSL https://nodejs.org/dist/v22.11.0/node-v22.11.0-linux-x64.tar.gz | tar -xz --strip-components=1 -C "$HOME/node"
  npm install -g pnpm@10 >/dev/null
fi
# Work on a copy: the source is read-only and its node_modules/target are the Mac's.
rsync -a --delete --exclude node_modules --exclude target --exclude dist --exclude .git \
  --exclude src-tauri/resources/platform-tools /src/ /build/work/
cd /build/work
mkdir -p src-tauri/resources/platform-tools
cp /src/src-tauri/resources/platform-tools/README.md src-tauri/resources/platform-tools/
export CARGO_TARGET_DIR=/build/target CI=true STATASMON_ADB_OS=linux APPIMAGE_EXTRACT_AND_RUN=1
pnpm install --frozen-lockfile >/dev/null
echo "{\"version\":\"$VERSION\",\"bundle\":{\"createUpdaterArtifacts\":false}}" > /build/overrides.json
pnpm tauri build --bundles appimage,deb --config /build/overrides.json
cp /build/target/release/bundle/appimage/*.AppImage /build/target/release/bundle/deb/*.deb /out/
ls -lh /out
