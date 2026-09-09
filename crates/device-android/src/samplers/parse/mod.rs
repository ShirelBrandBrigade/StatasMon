//! Pure parsers for everything the samplers read. Each `parse_x(&str)` is
//! side-effect free and tested against `fixtures/android/<device>/*.txt`.
//! Nothing here knows about the shell, the sink or timing.

pub mod battery;
pub mod cpu;
pub mod disk;
pub mod logcat;
pub mod mem;
pub mod net;
pub mod netstats;
pub mod process;
pub mod thermal;
pub mod wifi;

/// A parser could not make sense of its input. Samplers log these at
/// `debug` and skip the affected metrics for that tick (never zeros).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("missing field: {0}")]
    Missing(&'static str),
    #[error("invalid {field}: {value:?}")]
    Invalid { field: &'static str, value: String },
    #[error("empty input for {0}")]
    Empty(&'static str),
}

impl ParseError {
    pub(crate) fn invalid(field: &'static str, value: impl Into<String>) -> Self {
        Self::Invalid {
            field,
            value: value.into(),
        }
    }
}

/// Sentinel line the batched shell commands print before every section:
/// `==[name]==`. Output before the first sentinel is keyed by `""`.
pub const SECTION_PREFIX: &str = "==[";
pub const SECTION_SUFFIX: &str = "]==";

/// The `echo` that emits a sentinel for `name`.
pub fn sentinel(name: &str) -> String {
    format!("echo '{SECTION_PREFIX}{name}{SECTION_SUFFIX}'")
}

/// Split one batched command output into named sections.
pub fn split_sections(output: &str) -> Vec<(String, String)> {
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut current = String::new();
    let mut body = String::new();
    for line in output.lines() {
        let trimmed = line.trim_end_matches('\r');
        if let Some(rest) = trimmed.strip_prefix(SECTION_PREFIX)
            && let Some(name) = rest.strip_suffix(SECTION_SUFFIX)
        {
            if !current.is_empty() || !body.trim().is_empty() {
                sections.push((std::mem::take(&mut current), std::mem::take(&mut body)));
            }
            body.clear();
            current = name.to_owned();
            continue;
        }
        body.push_str(trimmed);
        body.push('\n');
    }
    if !current.is_empty() || !body.trim().is_empty() {
        sections.push((current, body));
    }
    sections
}

/// Convenience: the body of section `name`, if present and non-blank.
pub fn section<'a>(sections: &'a [(String, String)], name: &str) -> Option<&'a str> {
    sections
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, b)| b.as_str())
        .filter(|b| !b.trim().is_empty())
}

pub(crate) fn parse_u64(field: &'static str, s: &str) -> Result<u64, ParseError> {
    s.trim()
        .parse::<u64>()
        .map_err(|_| ParseError::invalid(field, s.trim()))
}

pub(crate) fn parse_i64(field: &'static str, s: &str) -> Result<i64, ParseError> {
    s.trim()
        .parse::<i64>()
        .map_err(|_| ParseError::invalid(field, s.trim()))
}

pub(crate) fn parse_f64(field: &'static str, s: &str) -> Result<f64, ParseError> {
    s.trim()
        .parse::<f64>()
        .map_err(|_| ParseError::invalid(field, s.trim()))
}

#[cfg(test)]
pub(crate) fn fixture(name: &str) -> String {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/android/sm-x700-android16");
    crate::FixtureShell::load(&dir)
        .unwrap_or_else(|e| panic!("fixture dir {}: {e}", dir.display()))
        .file(name)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn splits_sections_by_sentinel() {
        let out = "==[a]==\nline1\nline2\n==[b]==\n==[c]==\nx\n";
        let s = split_sections(out);
        assert_eq!(section(&s, "a"), Some("line1\nline2\n"));
        assert_eq!(section(&s, "b"), None, "blank section is absent");
        assert_eq!(section(&s, "c"), Some("x\n"));
        assert_eq!(section(&s, "zzz"), None);
    }

    #[test]
    fn sentinel_round_trips() {
        assert_eq!(sentinel("stat"), "echo '==[stat]=='");
    }
}
