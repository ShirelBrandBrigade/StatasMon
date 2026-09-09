#!/usr/bin/env bash
# Bump the app version everywhere it is declared, in one go.
#
#   scripts/bump-version.sh 0.2.0
#
# Touches: Cargo.toml ([workspace.package] version), src-tauri/tauri.conf.json,
# package.json, apps/desktop/package.json, packages/ui/package.json,
# packages/protocol/package.json, and Cargo.lock (via `cargo update -w`).
# It refuses a version that is not plain semver (x.y.z or x.y.z-pre.n) and
# prints every file it changed. Commit and tag afterwards (docs/RELEASE.md).
set -euo pipefail

VERSION="${1:-}"
if [[ -z "$VERSION" ]]; then
  echo "usage: $0 <version>   e.g. $0 0.2.0" >&2
  exit 2
fi
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]]; then
  echo "error: '$VERSION' is not a semver version (x.y.z or x.y.z-pre.1)" >&2
  exit 2
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

current="$(node -p 'require("./package.json").version')"
echo "bump: $current -> $VERSION"

# The workspace version line (only the first `version = "..."` inside
# [workspace.package]); every crate inherits it with `version.workspace = true`.
perl -0pi -e 's/(\[workspace\.package\]\s*\nversion = ")[^"]+(")/${1}'"$VERSION"'${2}/' Cargo.toml
echo "  Cargo.toml"

# JSON manifests: rewrite only the top-level "version" key.
for f in src-tauri/tauri.conf.json package.json apps/desktop/package.json \
         packages/ui/package.json packages/protocol/package.json; do
  node -e '
    const fs = require("fs");
    const [file, version] = process.argv.slice(1);
    const text = fs.readFileSync(file, "utf8");
    const next = text.replace(/^(\s*"version":\s*")[^"]+(")/m, `$1${version}$2`);
    if (next === text) { console.error(`no version key in ${file}`); process.exit(1); }
    fs.writeFileSync(file, next);
  ' "$f" "$VERSION"
  echo "  $f"
done

# Refresh Cargo.lock for the workspace crates without touching dependencies.
cargo update --workspace --offline >/dev/null 2>&1 || cargo update --workspace >/dev/null
echo "  Cargo.lock"

echo
echo "now: add a [$VERSION] section to CHANGELOG.md, then"
echo "  git commit -am \"release: v$VERSION\" && git tag v$VERSION && git push origin main v$VERSION"
