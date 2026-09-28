#!/usr/bin/env bash
# Builds every installer on one Mac and collects them in dist/ (ADR-023):
#   macOS arm64 + x86_64 (.dmg, ad-hoc sealed), Windows x64 (.exe, NSIS,
#   cross-compiled with cargo-xwin), Linux x86_64 (.deb + .rpm, built in an
#   Ubuntu 22.04 container through Colima/Docker). The .AppImage needs a real
#   x86_64 Linux host or GitHub's runners (STATASMON_APPIMAGE=1 tries anyway).
#
#   scripts/build-all.sh                 # everything
#   scripts/build-all.sh mac windows     # a subset: mac | windows | linux
#   STATASMON_VERSION=0.1.62 scripts/build-all.sh   # pin the version
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

# <major>.<minor> from tauri.conf.json + the commit count, or STATASMON_VERSION to pin it.
version="${STATASMON_VERSION:-$(node -p 'require("./src-tauri/tauri.conf.json").version.split(".").slice(0,2).join(".")').$(git rev-list --count HEAD)}"
# Every installer lands flat in dist/, named with its version.
dist="$here/dist"
mkdir -p "$dist"
overrides="$(mktemp -t statasmon-overrides).json"
trap 'rm -f "$overrides"' EXIT
what=("$@"); [[ ${#what[@]} -eq 0 ]] && what=(mac windows linux)
echo "== StatasMon $version → $dist (${what[*]})"

start_docker() {
  docker info >/dev/null 2>&1 || colima start --arch aarch64 --vm-type vz --vz-rosetta --cpu 4 --memory 4 --disk 40
  # The final crate needs more memory than a 4 GB VM has; swap makes up for it.
  colima ssh -- sh -c 'swapon --show | grep -q /swapfile || { sudo fallocate -l 8G /swapfile && sudo chmod 600 /swapfile && sudo mkswap /swapfile >/dev/null && sudo swapon /swapfile; }' || true
}

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
  # zlib: Homebrew's makensis runs out of memory with the default LZMA on Apple Silicon.
  echo "{\"version\":\"$version\",\"bundle\":{\"createUpdaterArtifacts\":false,\"windows\":{\"nsis\":{\"compression\":\"zlib\",\"installMode\":\"currentUser\"}}}}" > "$overrides"
  nsis_dir="target/x86_64-pc-windows-msvc/release/nsis/x64"
  rm -f "$nsis_dir/nsis-output.exe"
  # Homebrew's makensis crashes (bad_alloc) on Apple Silicon: let Tauri compile
  # and write installer.nsi, then pack it with makensis in a Linux container
  # that sees the same absolute paths.
  STATASMON_ADB_OS=windows pnpm tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis --config "$overrides" \
    || echo "== makensis failed on the host; packing in a container"
  exe="target/x86_64-pc-windows-msvc/release/bundle/nsis/StatasMon_${version}_x64-setup.exe"
  if [[ ! -f "$exe" || "$exe" -ot "$nsis_dir/installer.nsi" ]]; then
    start_docker
    docker run --rm -v "$here":"$here" -v "$HOME/Library/Caches/tauri":"$HOME/Library/Caches/tauri" \
      -w "$here/$nsis_dir" ubuntu:24.04 bash -c \
      'export DEBIAN_FRONTEND=noninteractive; apt-get update -qq >/dev/null && apt-get install -y -qq nsis >/dev/null 2>&1; makensis -V2 -INPUTCHARSET UTF8 installer.nsi | grep -v "File /a is disabled"'
    exe="$nsis_dir/nsis-output.exe"
  fi
  [[ -s "$exe" ]] || { echo "no Windows installer was produced" >&2; exit 1; }
  cp "$exe" "$dist/StatasMon_${version}_x64-setup.exe"
fi

if want linux; then
  echo "== Linux x86_64 (Ubuntu 22.04 container)"
  start_docker
  # The web part is the same everywhere: build it here, compile Rust in the container.
  pnpm --filter @statasmon/desktop build
  cli="$(node -p 'require("./package.json").devDependencies["@tauri-apps/cli"].replace(/^[^0-9]*/, "")')"
  docker run --rm --platform linux/amd64 \
    -v "$here":/src:ro -v statasmon-linux-build:/build -v "$dist":/out \
    -e VERSION="$version" -e TAURI_CLI_VERSION="$cli" -e CARGO_BUILD_JOBS=2 -e STATASMON_APPIMAGE="${STATASMON_APPIMAGE:-0}" ubuntu:22.04 bash /src/scripts/build-linux-in-container.sh
fi

# Whatever was built last, leave the Mac's own adb in the resource folder.
STATASMON_ADB_OS=darwin bash scripts/fetch-platform-tools.sh >/dev/null
# The VM holds 4 GB of this Mac's memory; give it back.
colima status >/dev/null 2>&1 && colima stop >/dev/null 2>&1 || true
echo "== done"; ls -lh "$dist"
