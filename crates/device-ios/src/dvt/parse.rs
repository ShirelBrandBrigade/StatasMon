//! Small pure helpers over the plist values DVT services return
//! (NSKeyedArchive-decoded by the `idevice` crate).

use plist::{Dictionary, Value};

/// Any numeric plist value as f64.
pub fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Real(f) => Some(*f),
        Value::Integer(i) => i
            .as_signed()
            .map(|x| x as f64)
            .or_else(|| i.as_unsigned().map(|x| x as f64)),
        Value::Boolean(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// A non-negative integer value (reals are truncated; negatives are `None`).
pub fn uint(v: &Value) -> Option<u64> {
    match v {
        Value::Integer(i) => i
            .as_unsigned()
            .or_else(|| i.as_signed().and_then(|s| u64::try_from(s).ok())),
        Value::Real(f) if *f >= 0.0 && f.is_finite() => Some(*f as u64),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// `num` of `d[key]`.
pub fn get_num(d: &Dictionary, key: &str) -> Option<f64> {
    d.get(key).and_then(num)
}

/// `uint` of `d[key]`.
pub fn get_uint(d: &Dictionary, key: &str) -> Option<u64> {
    d.get(key).and_then(uint)
}

/// `d[key]` as a string.
pub fn get_str(d: &Dictionary, key: &str) -> Option<String> {
    d.get(key).and_then(|v| v.as_string()).map(str::to_owned)
}

/// A counter delta that treats a wrap or reset as "no delta".
pub fn delta(prev: Option<u64>, now: u64) -> Option<u64> {
    prev.and_then(|p| now.checked_sub(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_from_every_representation() {
        assert_eq!(num(&Value::Real(1.5)), Some(1.5));
        assert_eq!(num(&Value::Integer(7i64.into())), Some(7.0));
        assert_eq!(num(&Value::Integer(u64::MAX.into())), Some(u64::MAX as f64));
        assert_eq!(num(&Value::Boolean(true)), Some(1.0));
        assert_eq!(num(&Value::String(" 12 ".into())), Some(12.0));
        assert_eq!(num(&Value::Data(vec![])), None);
        assert_eq!(uint(&Value::Integer((-3i64).into())), None);
        assert_eq!(uint(&Value::Real(2.9)), Some(2));
        assert_eq!(delta(Some(10), 15), Some(5));
        assert_eq!(delta(Some(20), 15), None);
        assert_eq!(delta(None, 15), None);
    }
}
