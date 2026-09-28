#!/usr/bin/env bash
# Builds every installer on one Mac and collects them in dist/ (ADR-023):
#   macOS arm64 + x86_64 (.dmg, ad-hoc sealed), Windows x64 (.exe, NSIS,
#   cross-compiled with cargo-xwin), Linux x86_64 (.AppImage + .deb, built
#   in an Ubuntu 22.04 container through Colima/Docker).
#
#   scripts/build-all.sh                 # everything
#   scripts/build-all.sh mac windows     # a subset: mac | windows | linux
#
# One-time setup on the Mac:
#   brew install nsis llvm colima docker
#   rustup target add x86_64-apple-darwin x86_64-pc-windows-msvc
#   cargo install --locked cargo-xwin
set -euo pipefail

here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
export PATH="/opt/homebrew/bin:/opt/homebrew/opt/llvm/bin:/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
export CI=true

version="$(node -p 'require("./src-tauri/tauri.conf.json").version.split(".").slice(0,2).join(".")').$(git rev-list --count HEAD)"
dist="$here/dist/$version"
mkdir -p "$dist"
overrides="$(mktemp -t statasmon-overrides).json"
trap 'rm -f "$overrides"' EXIT
what=("$@"); [[ ${#what[@]} -eq 0 ]] && what=(mac windows linux)
echo "== StatasMon $version → $dist (${what[*]})"

want() { local x; for x in "${what[@]}"; do [[ "$x" == "$1" ]] && return 0; done; return 1; }

if want mac; then
  echo "{\"version\":\"$version\",\"bundle\":{\"createUpdaterArtifacts\":false,\"macOS\":{\"signingIdentity\":\"-\"}}}" > "$overrides"
  for target in aarch64-apple-darwin x86_64-apple-darwin; do
    echo "== macOS $target"
    STATASMON_ADB_OS=darwin pnpm tauri build --target "$target" --bundles app --config "$overrides"
    app="target/$target/release/bundle/macos/StatasMon.app"
    codesign --verify --deep --strict "$app"
    arch="${target%%-*}"; [[ "$arch" == "x86_64" ]] && arch=x64
    rm -f "$dist/StatasMon_${version}_${arch}.dmg"
    hdiutil create -quiet -volname "StatasMon" -srcfolder "$app" -ov -format UDZO "$dist/StatasMon_${version}_${arch}.dmg"
  done
fi

if want windows; then
  echo "== Windows x86_64 (cargo-xwin + NSIS)"
  echo "{\"version\":\"$version\",\"bundle\":{\"createUpdaterArtifacts\":false}}" > "$overrides"
  STATASMON_ADB_OS=windows pnpm tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis --config "$overrides"
  cp target/x86_64-pc-windows-msvc/release/bundle/nsis/*-setup.exe "$dist/"
fi

if want linux; then
  echo "== Linux x86_64 (Ubuntu 22.04 container)"
  if ! docker info >/dev/null 2>&1; then
    colima start --arch aarch64 --vm-type vz --vz-rosetta --cpu 4 --memory 5 --disk 40
  fi
  docker run --rm --platform linux/amd64 \
    -v "$here":/src:ro -v statasmon-linux-build:/build -v "$dist":/out \
    -e VERSION="$version" ubuntu:22.04 bash /src/scripts/build-linux-in-container.sh
fi

# Whatever was built last, leave the Mac's own adb in the resource folder.
STATASMON_ADB_OS=darwin bash scripts/fetch-platform-tools.sh >/dev/null
echo "== done"; ls -lh "$dist"
