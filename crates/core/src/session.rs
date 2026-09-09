//! Per-device recording session (docs/FEATURES.md §0): state, elapsed time
//! that excludes pauses, marks and counters. The wire shapes
//! ([`SessionState`], [`SessionStatus`], [`Mark`]) come from the protocol;
//! this module owns the bookkeeping. The engine passes `now_ms` explicitly
//! so timing is testable without a fake clock.
//!
//! `Idle` = never started on this connection; `Stopped` = a session ended
//! and its counters are kept for display until the next `start`.

use statasmon_protocol::{Mark, SessionState, SessionStatus};

/// Session bookkeeping for one device.
#[derive(Debug, Clone)]
pub struct Session {
    id: Option<String>,
    status: SessionStatus,
    started_at_ms: Option<i64>,
    /// Recording time accumulated before the current recording stretch.
    banked_ms: i64,
    /// Start of the current recording stretch (while Recording).
    stretch_start_ms: Option<i64>,
    marks: Vec<Mark>,
    sample_count: u64,
    log_count: u64,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    /// An idle session with no history.
    pub fn new() -> Self {
        Self {
            id: None,
            status: SessionStatus::Idle,
            started_at_ms: None,
            banked_ms: 0,
            stretch_start_ms: None,
            marks: Vec::new(),
            sample_count: 0,
            log_count: 0,
        }
    }

    /// Current status.
    pub fn status(&self) -> SessionStatus {
        self.status
    }

    /// True while data should be stored and forwarded.
    pub fn is_recording(&self) -> bool {
        self.status == SessionStatus::Recording
    }

    /// The session id, if one was started.
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// Start a new session at `now_ms`. Allowed when idle or stopped.
    pub fn start(&mut self, now_ms: i64) -> Result<(), SessionStatus> {
        if !matches!(self.status, SessionStatus::Idle | SessionStatus::Stopped) {
            return Err(self.status);
        }
        *self = Self {
            id: Some(uuid::Uuid::new_v4().to_string()),
            status: SessionStatus::Recording,
            started_at_ms: Some(now_ms),
            banked_ms: 0,
            stretch_start_ms: Some(now_ms),
            marks: Vec::new(),
            sample_count: 0,
            log_count: 0,
        };
        Ok(())
    }

    /// Pause at `now_ms`. Allowed only while recording.
    pub fn pause(&mut self, now_ms: i64) -> Result<(), SessionStatus> {
        if self.status != SessionStatus::Recording {
            return Err(self.status);
        }
        self.bank(now_ms);
        self.status = SessionStatus::Paused;
        Ok(())
    }

    /// Resume at `now_ms`. Allowed only while paused.
    pub fn resume(&mut self, now_ms: i64) -> Result<(), SessionStatus> {
        if self.status != SessionStatus::Paused {
            return Err(self.status);
        }
        self.stretch_start_ms = Some(now_ms);
        self.status = SessionStatus::Recording;
        Ok(())
    }

    /// Stop at `now_ms`. Allowed while recording or paused. Counters and
    /// marks are kept for display until the next `start`.
    pub fn stop(&mut self, now_ms: i64) -> Result<(), SessionStatus> {
        if matches!(self.status, SessionStatus::Idle | SessionStatus::Stopped) {
            return Err(self.status);
        }
        self.bank(now_ms);
        self.status = SessionStatus::Stopped;
        Ok(())
    }

    /// Add a mark. Allowed while recording or paused.
    pub fn mark(&mut self, now_ms: i64, label: impl Into<String>) -> Result<Mark, SessionStatus> {
        if matches!(self.status, SessionStatus::Idle | SessionStatus::Stopped) {
            return Err(self.status);
        }
        let mark = Mark {
            ts_ms: now_ms,
            label: label.into(),
        };
        self.marks.push(mark.clone());
        Ok(mark)
    }

    /// Count one stored sample tick.
    pub fn count_samples(&mut self, n: u64) {
        self.sample_count += n;
    }

    /// Count stored log entries.
    pub fn count_logs(&mut self, n: u64) {
        self.log_count += n;
    }

    /// Recording time at `now_ms`, excluding pauses.
    pub fn elapsed_ms(&self, now_ms: i64) -> i64 {
        let live = match (self.status, self.stretch_start_ms) {
            (SessionStatus::Recording, Some(start)) => (now_ms - start).max(0),
            _ => 0,
        };
        self.banked_ms + live
    }

    /// Snapshot for the UI.
    pub fn state(&self, now_ms: i64) -> SessionState {
        SessionState {
            id: self.id.clone(),
            status: self.status,
            started_at_ms: self.started_at_ms,
            elapsed_ms: self.elapsed_ms(now_ms),
            sample_count: self.sample_count,
            log_count: self.log_count,
            marks: self.marks.clone(),
        }
    }

    fn bank(&mut self, now_ms: i64) {
        if let (SessionStatus::Recording, Some(start)) = (self.status, self.stretch_start_ms) {
            self.banked_ms += (now_ms - start).max(0);
        }
        self.stretch_start_ms = None;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_excludes_paused_time() {
        let mut s = Session::new();
        assert_eq!(s.status(), SessionStatus::Idle);
        assert_eq!(s.elapsed_ms(5_000), 0);
        s.start(1_000).unwrap();
        assert!(s.id().is_some());
        assert_eq!(s.elapsed_ms(4_000), 3_000);
        s.pause(4_000).unwrap();
        assert_eq!(s.elapsed_ms(9_000), 3_000);
        s.resume(9_000).unwrap();
        assert_eq!(s.elapsed_ms(11_000), 5_000);
        s.stop(12_000).unwrap();
        assert_eq!(s.elapsed_ms(50_000), 6_000);
        assert_eq!(s.state(50_000).status, SessionStatus::Stopped);
    }

    #[test]
    fn transitions_are_checked() {
        let mut s = Session::new();
        assert_eq!(s.pause(0), Err(SessionStatus::Idle));
        assert_eq!(s.resume(0), Err(SessionStatus::Idle));
        assert_eq!(s.stop(0), Err(SessionStatus::Idle));
        assert_eq!(s.mark(0, "x"), Err(SessionStatus::Idle));
        s.start(0).unwrap();
        assert_eq!(s.start(0), Err(SessionStatus::Recording));
        assert_eq!(s.resume(0), Err(SessionStatus::Recording));
        s.pause(1).unwrap();
        assert_eq!(s.pause(1), Err(SessionStatus::Paused));
        assert!(s.mark(2, "while paused").is_ok());
        s.stop(3).unwrap();
        assert_eq!(s.stop(3), Err(SessionStatus::Stopped));
        let first_id = s.id().unwrap().to_owned();
        s.start(10).unwrap();
        assert_ne!(s.id().unwrap(), first_id);
        assert!(s.state(10).marks.is_empty(), "marks reset on a new session");
    }

    #[test]
    fn marks_and_counters_are_reported() {
        let mut s = Session::new();
        s.start(0).unwrap();
        s.mark(5, "Login").unwrap();
        s.count_samples(3);
        s.count_logs(40);
        let st = s.state(10);
        assert_eq!(
            st.marks,
            vec![Mark {
                ts_ms: 5,
                label: "Login".into()
            }]
        );
        assert_eq!(st.sample_count, 3);
        assert_eq!(st.log_count, 40);
        assert_eq!(st.started_at_ms, Some(0));
    }
}
