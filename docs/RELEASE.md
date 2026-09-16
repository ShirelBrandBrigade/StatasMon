# Release runbook

How a StatasMon version goes from `main` to installers people can run, and
how those installers update themselves. Decisions behind this: ADR-003
(Tauri), ADR-015 (distribution and update policy). Nothing here contacts
the network except the explicit update check the user opts into.

## 0. What ships
One self-contained file per operating system (ADR-021):
- macOS: `.dmg` for Apple Silicon and Intel (the `.app` inside carries
  adb under `Contents/Resources/platform-tools/`).
- Windows: per-user NSIS `.exe` and `.msi` (adb.exe and its DLLs under
  the install folder's `platform-tools/`). iPhone/iPad support needs the
  free Apple Devices app or iTunes on the machine; the app says so.
- Linux x86_64: `.AppImage` and `.deb` (the `.deb` depends on `usbmuxd`
  for iPhone/iPad; the AppImage tells the user to install it).
- Updater artifacts and `latest.json` when the signing key is present.
`scripts/fetch-platform-tools.sh` runs before every build (locally through
`beforeBuildCommand`, in CI as its own step) and needs `curl` + `unzip`.
Send `docs/GETTING-STARTED.md` along with the file.

## 1. Version bump

All manifests carry the same version: `Cargo.toml` (workspace, inherited
by every crate), `src-tauri/tauri.conf.json`, `package.json`,
`apps/desktop/package.json`, `packages/ui/package.json`,
`packages/protocol/package.json`. Bump them in one go:

```bash
scripts/bump-version.sh 0.2.0
```

The script rejects anything that is not semver, rewrites only the
`version` keys, refreshes `Cargo.lock`, and prints the next commands. Then:

1. Move the `Unreleased` items in `CHANGELOG.md` under a new
   `## [0.2.0] - YYYY-MM-DD` heading (Keep a Changelog).
2. `git commit -am "release: v0.2.0"`, `git tag v0.2.0`,
   `git push origin main v0.2.0`.

The tag is what triggers the release build; the version inside the
manifests is what the updater compares, so the two must agree
(`v0.2.0` ↔ `0.2.0`). A pre-release tag such as `v0.2.0-beta.1` is marked
as a pre-release on GitHub and is ignored by the updater's `latest.json`
unless you publish it as the latest release.

## 2. What CI produces (`.github/workflows/release.yml`)
Three ways a build starts:
- **Every push to `main`** → a published release. The `version` job
  computes `<major>.<minor>` from `tauri.conf.json` plus the number of
  commits on `main` as the patch (`0.1.57`, `0.1.58`, …), tags the commit
  `v<version>`, and the four build jobs (macOS arm64, macOS x86_64,
  Windows, Linux) upload their installers to that release with
  `--config ci-overrides.json` carrying the version. `latest.json` is
  attached when the updater key exists, so installed apps update to the
  newest push. Coworkers always take the top entry under Releases.
- **A hand-made tag `v*`** → the same build as a **draft** release, for a
  curated version you publish by hand after a smoke test.
- **Run workflow (Actions tab)** → an unsigned test build attached to the
  run as artifacts; no release.
Runs on the same ref queue behind each other (concurrency group), so two
pushes in a row produce two releases in order.

## 3. Secrets to add (GitHub → Settings → Secrets and variables → Actions)

### Updater signature (both platforms) — **required for the updater**

| Secret | Value |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | contents of `~/.tauri/statasmon.key` (the whole file, one base64 line) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | the key's password; the current key was generated with `--ci`, so it is **empty** (create the secret with an empty value or omit it) |

The matching **public** key is already in `tauri.conf.json`
(`plugins.updater.pubkey`). The private key exists only in
`~/.tauri/statasmon.key` on the machine that generated it (2026-09-09);
back it up in the team password manager. It is **never** committed. If it
is lost, generate a new pair (`pnpm tauri signer generate -w
~/.tauri/statasmon.key`), put the new public key in `tauri.conf.json` and
ship one release that users install by hand: an app with the old public
key will reject artifacts signed with the new private key (which is the
point).

### macOS signing and notarization (Apple Developer Program membership)

1. In the Apple Developer portal create a **Developer ID Application**
   certificate; download it into Keychain Access together with its private
   key. Export it as `.p12` with a password.
2. `base64 -i DeveloperID.p12 | pbcopy` → secret `APPLE_CERTIFICATE`;
   the export password → `APPLE_CERTIFICATE_PASSWORD`.
3. `APPLE_SIGNING_IDENTITY` = the certificate's common name, e.g.
   `Developer ID Application: Your Name (TEAMID1234)` (see
   `security find-identity -v -p codesigning`).
4. Notarization, preferred route — an **App Store Connect API key** (Users
   and Access → Integrations → App Store Connect API, role Developer):
   `APPLE_API_ISSUER` (issuer UUID), `APPLE_API_KEY` (key id, e.g.
   `AB12CD34EF`), `APPLE_API_KEY_CONTENT` (contents of the downloaded
   `AuthKey_<id>.p8`). The workflow writes the `.p8` to a temp file and
   exports `APPLE_API_KEY_PATH` for the bundler.
   Alternative route — an Apple ID: `APPLE_ID`, `APPLE_PASSWORD` (an
   **app-specific password** from appleid.apple.com, never the account
   password), `APPLE_TEAM_ID`.

The Tauri bundler imports the certificate into a temporary keychain,
signs the `.app` with the hardened runtime, notarizes with `notarytool`,
staples the ticket and builds the DMG. Verify a downloaded build with
`spctl -a -vv -t install StatasMon.app` (expects `source=Notarized
Developer ID`) and `xcrun stapler validate StatasMon.app`.

### Windows signing (pick one route)

**Route A — Azure Artifact Signing (Trusted Signing), recommended.** No
hardware token, per-file signing through a service principal.

| Secret | Value |
|---|---|
| `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET` | the app registration allowed to sign with the profile |
| `AZURE_SIGNING_ENDPOINT` | e.g. `https://weu.codesigning.azure.net` |
| `AZURE_SIGNING_ACCOUNT` | the Trusted Signing account name |
| `AZURE_SIGNING_PROFILE` | the certificate profile name |

The workflow installs `trusted-signing-cli` and sets
`bundle.windows.signCommand` to
`trusted-signing-cli -e <endpoint> -a <account> -c <profile> %1`, so every
binary and installer is signed by the bundler.

**Route B — an OV/EV code-signing certificate you own.** OV certificates
have been hardware-bound since 2023, so the usual CI path is a `.pfx`
exported from a non-HSM certificate (or a legacy one): `WINDOWS_CERTIFICATE`
(base64 of the `.pfx`), `WINDOWS_CERTIFICATE_PASSWORD`. The workflow
imports it into the runner's user store and sets
`bundle.windows.certificateThumbprint` + a DigiCert timestamp URL, so
`signtool` signs everything. For a token-bound EV certificate, sign on a
machine that has the token instead (a self-hosted runner, or by hand with
`signtool sign /tr http://timestamp.digicert.com /td sha256 /fd sha256
/a <file>`), then attach the signed installers to the draft release and
regenerate `latest.json` (§5).

SmartScreen shows "unknown publisher" for a new OV identity until it
earns reputation; EV and Trusted Signing skip that.

## 4. Building locally

```bash
export TAURI_SIGNING_PRIVATE_KEY=~/.tauri/statasmon.key   # path or contents
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""               # this key has none
pnpm tauri build                                           # unsigned .app + .dmg on macOS
```

Output under `target/release/bundle/`: `macos/StatasMon.app`,
`dmg/StatasMon_<v>_aarch64.dmg`, `macos/StatasMon.app.tar.gz` (+ `.sig`).
On Windows: `nsis/*.exe`, `msi/*.msi` (+ `.sig` for the NSIS installer).
Without `TAURI_SIGNING_PRIVATE_KEY` the build stops with "A public key
has been found, but no private key"; pass
`--config '{"bundle":{"createUpdaterArtifacts":false}}'` for a quick
unsigned installer. Unsigned macOS builds are ad-hoc signed by the linker
and launch on the building machine; on another Mac they need
"Open anyway" in System Settings → Privacy & Security. A signed build:
`APPLE_SIGNING_IDENTITY="Developer ID Application: …" pnpm tauri build`
(notarization needs the `APPLE_*` variables from §3 as well).

## 5. The updater

- **Endpoint**: `plugins.updater.endpoints[0]` =
  `https://github.com/ShirelBrandBrigade/StatasMon/releases/latest/download/latest.json`.
  GitHub resolves `latest` to the most recent **published, non-pre-release**
  release. If the repository moves, change the owner/name in
  `tauri.conf.json`; the app treats an endpoint still containing the
  literal `OWNER` as "updater not configured" and says so instead of
  failing.
- **Policy** (ADR-015): the app checks only when the user turned on
  "Check for updates on launch" in Settings (default off). A found update
  is announced as a notice ("Update available: v0.2.0 (…)") and is
  installed only when the user asks (`install_update` command; Phase 8
  adds the button). The plugin verifies the minisign signature against the
  embedded public key before installing; an unsigned or tampered artifact
  is rejected. macOS replaces the `.app` in place; Windows runs the NSIS
  installer in passive mode (progress bar, no questions) and relaunches.
- **`latest.json`** is produced by `tauri-action` and attached to the
  release. Its shape, should it ever need to be written by hand:

```json
{
  "version": "0.2.0",
  "notes": "One line from the CHANGELOG",
  "pub_date": "2026-09-09T12:00:00Z",
  "platforms": {
    "darwin-aarch64": { "signature": "<contents of StatasMon.app.tar.gz.sig>", "url": "https://github.com/ShirelBrandBrigade/StatasMon/releases/download/v0.2.0/StatasMon_aarch64.app.tar.gz" },
    "darwin-x86_64":  { "signature": "<…>", "url": "https://github.com/ShirelBrandBrigade/StatasMon/releases/download/v0.2.0/StatasMon_x64.app.tar.gz" },
    "windows-x86_64": { "signature": "<contents of StatasMon_0.2.0_x64-setup.exe.sig>", "url": "https://github.com/ShirelBrandBrigade/StatasMon/releases/download/v0.2.0/StatasMon_0.2.0_x64-setup.exe" }
  }
}
```

### Testing the updater locally with a static `latest.json`

1. Build the "new" version: `scripts/bump-version.sh 0.1.1` (do not
   commit), `pnpm tauri build` with the signing key exported. Copy
   `target/release/bundle/macos/StatasMon.app.tar.gz` and its `.sig` to a
   scratch folder; write `latest.json` there with `version: "0.1.1"`, the
   `.sig` contents as `signature` and
   `url: "http://localhost:8787/StatasMon.app.tar.gz"`.
2. `git checkout -- .` to return to 0.1.0, then serve the folder:
   `python3 -m http.server 8787 --directory <scratch>`.
3. Run the **old** version against the local endpoint. `http://` is
   rejected in release builds, so use a dev run:
   `pnpm tauri dev -- --config '{"plugins":{"updater":{"endpoints":["http://localhost:8787/latest.json"]}}}'`
   (a release build can use `dangerousInsecureTransportProtocol: true` in
   the same override; never commit that). Turn on "Check for updates on
   launch" in Settings and relaunch: the log shows `update available
   version=0.1.1` and the UI receives the "Update available: v0.1.1" notice.
4. Call `install_update` (from the Phase 8 button or the devtools console:
   `window.__TAURI__.core.invoke('install_update')`). The app downloads,
   verifies the signature, replaces itself and relaunches; About shows
   0.1.1. A wrong `.sig` must fail with a signature error — try it once.

## 6. Checklist before publishing a release

- [ ] `pnpm check`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace` green locally and the `ci` workflow green on `main`.
- [ ] `CHANGELOG.md` has a dated section for this version; the version was
      bumped with `scripts/bump-version.sh` and all manifests agree.
- [ ] `docs/KNOWN-ISSUES.md` reviewed; anything users would hit is in the
      CHANGELOG's "Known limitations".
- [ ] The `release` workflow for the tag is green on all three targets and
      the draft release holds: two `.dmg`, one `.exe`, one `.msi`, the
      updater artifacts (`.tar.gz`/`.exe` + `.sig`) and `latest.json`.
- [ ] macOS: `spctl -a -vv -t install` says Notarized Developer ID on a
      downloaded `.dmg`; the app opens on a clean machine without warnings.
- [ ] Windows: the `.exe` installs per-user without an admin prompt and
      shows the publisher (no "unknown publisher"); the `.msi` installs;
      both launch and find a device.
- [ ] Both installers smoke-tested: attach one Android and one iOS device,
      record 5 minutes, export a `.zip`, quit and relaunch (session is listed).
- [ ] Updater tested from the previous published version against this
      draft (temporarily publish, or point a dev build at the draft's
      `latest.json` URL with a token), including a bad-signature rejection.
- [ ] Publish the draft. Announce. Tag the `docs/PLAN.md` status line.

## 7. Where the pieces are

| Piece | Path |
|---|---|
| Bundle, updater, window config | `src-tauri/tauri.conf.json` |
| Plugins registered, startup check | `src-tauri/src/lib.rs` |
| `check_for_update`, `install_update` commands | `src-tauri/src/commands.rs`, `apps/desktop/src/bridge/commands.ts` |
| Permissions (updater, process) | `src-tauri/capabilities/default.json` |
| CI | `.github/workflows/ci.yml` (checks), `.github/workflows/release.yml` (installers) |
| Version bump | `scripts/bump-version.sh` |
| Icons | `src-tauri/icons/` (regenerate with `pnpm tauri icon <1024px png>`) |
