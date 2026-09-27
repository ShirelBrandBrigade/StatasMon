# Getting started (for the people you send the app to)

One file per computer. adb (for Android) travels inside the app, so there
is nothing to download for Android on any system. iPhones and iPads use
Apple's USB service, which the app cannot carry itself; what that means
per system is below.

| Your computer | Install | Android | iPhone / iPad |
| --- | --- | --- | --- |
| **macOS 12 or newer** (Apple Silicon or Intel) | open `StatasMon-<version>-<arch>.dmg`, drag StatasMon to Applications | works out of the box | works out of the box (macOS ships the USB service) |
| **Windows 10 / 11** | run `StatasMon_<version>_x64-setup.exe` (installs for your user, no admin) or the `.msi` | works out of the box; if a phone is not seen, enable USB debugging on it and accept the prompt | install the free **Apple Devices** app from the Microsoft Store (or iTunes from apple.com) once, then plug in the device |
| **Linux x86_64** (Ubuntu 22.04+, Fedora 39+ or similar) | `StatasMon_<version>_amd64.AppImage` (make it executable, run) or the `.deb` (`sudo apt install ./StatasMon_<version>_amd64.deb`) | works out of the box; some distributions need your user in the `plugdev` group for USB access | `sudo apt install usbmuxd` (the `.deb` pulls it in automatically), then plug in the device |

## First connection
1. Plug the device in over USB.
2. **Android**: turn on Developer options → USB debugging, then tap
   *Allow* on the phone when it asks about this computer.
3. **iPhone / iPad**: unlock it and tap *Trust* when asked. For CPU, memory,
   processes and network turn on Settings → Privacy & Security →
   **Developer Mode** and restart the device once. Battery, storage, device
   information, logs and crash reports work without it.
4. The device appears in the left column and recording starts on its own.

If something is missing the app says why in the top-right notice and in
the tooltip of every dashed value; nothing is ever guessed.

## What is inside the app
- adb 37.0.1 from Google's Android platform-tools (`platform-tools/` next
  to the app's resources, with Google's NOTICE file).
- The app's own implementation of Apple's USB, lockdown, diagnostics and
  developer-tunnel protocols. No Xcode, no libimobiledevice, no iTunes on
  macOS.
- Nothing is sent anywhere. The only network access is the device
  connection and the optional update check you can turn on in Settings.

## If macOS refuses to open the app
The builds are not yet signed with an Apple Developer ID, so macOS asks
before the first launch:
1. Open StatasMon once (it will be refused).
2. Open System Settings → Privacy & Security, scroll to the message about
   StatasMon and click **Open Anyway**, then confirm.
On macOS 14 and older, right-click the app and choose Open instead.
If a build older than 0.1.61 says the app "is damaged", that build was not
sealed; take the newest release, or run once in Terminal:
`xattr -cr /Applications/StatasMon.app`.
