//! Progress reporting: a callback called about every 2 %.

/// Minimum change between two reports.
const STEP_PCT: f32 = 2.0;

/// Tracks work done against an expected total and reports percentages.
pub struct Progress<'a> {
    cb: &'a mut dyn FnMut(f32, &str),
    total: u64,
    done: u64,
    last_pct: f32,
}

impl std::fmt::Debug for Progress<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Progress")
            .field("total", &self.total)
            .field("done", &self.done)
            .field("last_pct", &self.last_pct)
            .finish_non_exhaustive()
    }
}

impl<'a> Progress<'a> {
    /// A tracker with no work known yet.
    pub fn new(cb: &'a mut dyn FnMut(f32, &str)) -> Self {
        Self {
            cb,
            total: 0,
            done: 0,
            last_pct: -1.0,
        }
    }

    /// Replace the expected total.
    pub fn set_total(&mut self, total: u64) {
        self.total = total;
    }

    /// Add to the expected total.
    pub fn add_total(&mut self, n: u64) {
        self.total = self.total.saturating_add(n);
    }

    /// Work done so far.
    pub fn done(&self) -> u64 {
        self.done
    }

    /// Record `n` units of work; reports when the percentage moved ≥ 2 %.
    pub fn tick(&mut self, n: u64, what: &str) {
        self.done = self.done.saturating_add(n);
        let pct = if self.total == 0 {
            0.0
        } else {
            (self.done as f64 / self.total as f64 * 99.0).min(99.0) as f32
        };
        if pct - self.last_pct >= STEP_PCT {
            self.report(pct, what);
        }
    }

    /// Report unconditionally.
    pub fn report(&mut self, pct: f32, what: &str) {
        self.last_pct = pct;
        (self.cb)(pct, what);
    }
}
