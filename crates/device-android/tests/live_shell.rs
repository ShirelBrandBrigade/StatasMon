//! Live smoke test against a connected device. Skipped unless
//! STATASMON_LIVE_ANDROID=<serial> and STATASMON_ADB=<path to adb> are set.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use statasmon_device_android::{ProcessShell, Shell};

#[tokio::test]
async fn live_getprop_and_stream() {
    let (Ok(serial), Ok(adb)) = (
        std::env::var("STATASMON_LIVE_ANDROID"),
        std::env::var("STATASMON_ADB"),
    ) else {
        eprintln!("skipped: set STATASMON_LIVE_ANDROID and STATASMON_ADB");
        return;
    };
    let shell = ProcessShell::new(adb, serial);
    let model = shell.exec("getprop ro.product.model", 5000).await.unwrap();
    assert!(!model.trim().is_empty(), "model should not be empty");
    let batch = shell
        .exec("cat /proc/loadavg; echo ---; cat /proc/uptime", 5000)
        .await
        .unwrap();
    assert!(batch.contains("---"));
    use tokio::io::AsyncReadExt;
    let mut s = shell.stream("logcat -v threadtime -t 20").await.unwrap();
    let mut buf = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        s.read_to_string(&mut buf),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        buf.lines().count() >= 2,
        "expected logcat lines, got: {buf}"
    );
}
