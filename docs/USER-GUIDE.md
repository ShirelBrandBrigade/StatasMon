# StatasMon user guide

## Connecting an Android device
1. On the device: Settings → About → tap **Build number** seven times, then
   Settings → Developer options → enable **USB debugging**.
2. Plug the device in. Accept **Allow USB debugging** on the device (tick
   "Always allow from this computer").
3. StatasMon finds `adb` automatically (an installed Android SDK, PATH, or a
   copy under Downloads/platform-tools). If it cannot, the empty state shows
   the exact download step; the path can also be set in Settings.
4. Wireless: Developer options → **Wireless debugging** → Pair device with
   pairing code, then in StatasMon "+ Add a device" → Pair (host, port, code)
   → Connect (host, port).

On Windows, Google's USB driver covers Google devices; other manufacturers
ship their own driver with their desktop tools.

What Samsung and some other vendors lock away from the debugging shell:
battery cycle count and capacities, thermal zone files. StatasMon shows a
dash with the reason.

## Connecting an iPhone or iPad
1. Plug the device in and tap **Trust** when the device asks about this
   computer. Battery, storage, device information, logs, crash reports and
   packet capture work immediately.
2. For CPU, memory, processes and per-app network, enable **Developer
   Mode**: Settings → Privacy & Security → Developer Mode → on, then restart
   the device and confirm. StatasMon upgrades the device automatically; no
   root or extra tools are needed on iOS 17.4 and later.
3. On Windows, install iTunes or the **Apple Devices** app from the Microsoft
   Store first: it provides the USB driver StatasMon relies on.

Thermal state on iOS is read from the device's own thermal log messages, so
it says "not reported yet" until the device reports a change. Only the
battery temperature is readable.

## Sessions and export
Every connected device records from the moment it attaches. Pause stops
storing without disconnecting; Mark (or the `M` key) adds a labelled point
on every chart. Sessions are kept locally for 30 days (4 hours per session
by default; see Settings). Export from the top bar, the log card or the
Sessions screen: `.log` (plain text), `.jsonl` (full fields), `.csv`
(metrics), or a `.zip` bundle with device info, metrics, logs, events,
marks, crash reports and captures.

## Where things are stored
- macOS: `~/Library/Application Support/com.statasmon.app/`
- Windows: `%APPDATA%\com.statasmon.app\`
Nothing leaves the computer unless you export it.
