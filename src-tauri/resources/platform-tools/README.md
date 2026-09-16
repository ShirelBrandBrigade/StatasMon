# platform-tools

adb from Google's Android SDK platform-tools is placed here at build time by
`scripts/fetch-platform-tools.sh` (run through `tauri.conf.json`'s
beforeBuildCommand and by CI) and bundled into the app as a resource
(ADR-021). Only this file is tracked; the binaries are downloaded, never
committed. Google's terms for the SDK platform-tools apply
(https://developer.android.com/studio/terms); the NOTICE.txt that comes
with the download lists the open-source licences of the components.
