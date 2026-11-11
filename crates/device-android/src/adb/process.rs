//! Starting `adb` without a console window.
//!
//! adb is a console program. When a desktop (GUI-subsystem) app starts a
//! console program on Windows, Windows opens a console window for it unless
//! told not to, so every `adb` call flashed a black window open and closed.
//! `CREATE_NO_WINDOW` suppresses that. Other systems have no such window, so
//! this is the only OS-specific line about launching processes.

use std::path::Path;
use std::process::Stdio;

use tokio::process::Command;

/// Windows `CREATE_NO_WINDOW` process-creation flag.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// An `adb` command with no stdin, piped output, killed when dropped, and
/// (on Windows) no console window.
pub fn adb_command(adb: &Path) -> Command {
    let mut c = Command::new(adb);
    c.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    c.creation_flags(CREATE_NO_WINDOW);
    c
}
