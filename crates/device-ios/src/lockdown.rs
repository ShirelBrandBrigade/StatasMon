//! A thin wrapper over `idevice`'s `LockdownClient` (step 3.2) plus the
//! plist lookup helpers every tier-1 parser uses.
//!
//! `Lockdown::connect` opens the lockdown port and starts the TLS session
//! with the pair record; `Lockdown::connect_unpaired` opens it without a
//! session (only a handful of keys are readable that way, and it is what
//! the pairing flow needs). `developer_mode_status` goes through the
//! `com.apple.amfi.lockdown` service.

use idevice::IdeviceService;
use idevice::pairing_file::PairingFile;
use idevice::provider::IdeviceProvider;
use idevice::services::amfi::AmfiClient;
use idevice::services::lockdown::LockdownClient;

use crate::error::IosError;

/// Lockdown domain: battery percent and charging flags.
pub const DOMAIN_BATTERY: &str = "com.apple.mobile.battery";
/// Lockdown domain: storage totals.
pub const DOMAIN_DISK_USAGE: &str = "com.apple.disk_usage";
/// Lockdown domain: Wi-Fi sync / wireless lockdown flags.
pub const DOMAIN_WIRELESS: &str = "com.apple.mobile.wireless_lockdown";
/// Lockdown domain: language and locale.
pub const DOMAIN_INTERNATIONAL: &str = "com.apple.international";
/// Lockdown domain: Developer Mode flag (also readable via amfi).
pub const DOMAIN_AMFI: &str = "com.apple.security.mac.amfi";

/// The label every StatasMon connection announces to lockdownd.
pub const LABEL: &str = "StatasMon";

/// A lockdown connection, with or without a session.
#[derive(Debug)]
pub struct Lockdown {
    client: LockdownClient,
    session: bool,
}

impl Lockdown {
    /// Connect and start the TLS session with `pair`.
    pub async fn connect(
        provider: &dyn IdeviceProvider,
        pair: &PairingFile,
    ) -> Result<Self, IosError> {
        let mut client = LockdownClient::connect(provider)
            .await
            .map_err(|e| IosError::service("lockdown", e))?;
        client
            .start_session(pair)
            .await
            .map_err(|e| IosError::service("lockdown/StartSession", e))?;
        Ok(Self {
            client,
            session: true,
        })
    }

    /// Connect without a session (pairing, or the few public keys).
    pub async fn connect_unpaired(provider: &dyn IdeviceProvider) -> Result<Self, IosError> {
        let client = LockdownClient::connect(provider)
            .await
            .map_err(|e| IosError::service("lockdown", e))?;
        Ok(Self {
            client,
            session: false,
        })
    }

    /// True when a TLS session is active.
    pub fn has_session(&self) -> bool {
        self.session
    }

    /// The underlying client (for `pair_once` and `start_service`).
    pub fn client_mut(&mut self) -> &mut LockdownClient {
        &mut self.client
    }

    /// `GetValue` for `key` in `domain` (both optional; neither = the whole
    /// root dictionary).
    pub async fn get_value(
        &mut self,
        domain: Option<&str>,
        key: Option<&str>,
    ) -> Result<plist::Value, IosError> {
        self.client
            .get_value(key, domain)
            .await
            .map_err(|e| IosError::service("lockdown/GetValue", e))
    }

    /// `GetValue` of a whole domain as a dictionary.
    pub async fn get_domain(
        &mut self,
        domain: Option<&str>,
    ) -> Result<plist::Dictionary, IosError> {
        match self.get_value(domain, None).await? {
            plist::Value::Dictionary(d) => Ok(d),
            other => Err(IosError::Parse(format!(
                "lockdown domain {} is not a dictionary: {}",
                domain.unwrap_or("<root>"),
                type_name(&other)
            ))),
        }
    }

    /// The root dictionary (DeviceName, ProductType, ...).
    pub async fn root(&mut self) -> Result<plist::Dictionary, IosError> {
        self.get_domain(None).await
    }

    /// `com.apple.mobile.battery`.
    pub async fn battery_domain(&mut self) -> Result<plist::Dictionary, IosError> {
        self.get_domain(Some(DOMAIN_BATTERY)).await
    }

    /// `com.apple.disk_usage`.
    pub async fn disk_usage(&mut self) -> Result<plist::Dictionary, IosError> {
        self.get_domain(Some(DOMAIN_DISK_USAGE)).await
    }

    /// `com.apple.mobile.wireless_lockdown`.
    pub async fn wireless_lockdown(&mut self) -> Result<plist::Dictionary, IosError> {
        self.get_domain(Some(DOMAIN_WIRELESS)).await
    }

    /// `com.apple.international` (Language, Locale).
    pub async fn international(&mut self) -> Result<plist::Dictionary, IosError> {
        self.get_domain(Some(DOMAIN_INTERNATIONAL)).await
    }

    /// Ask lockdownd to start `service`; returns (port, ssl).
    pub async fn start_service(&mut self, service: &str) -> Result<(u16, bool), IosError> {
        self.client
            .start_service(service)
            .await
            .map_err(|e| IosError::service("lockdown/StartService", e))
    }
}

/// Developer Mode state via `com.apple.amfi.lockdown`.
pub async fn developer_mode_status(provider: &dyn IdeviceProvider) -> Result<bool, IosError> {
    let mut amfi = AmfiClient::connect(provider)
        .await
        .map_err(|e| IosError::service("amfi", e))?;
    amfi.get_developer_mode_status()
        .await
        .map_err(|e| IosError::service("amfi/DeveloperModeStatus", e))
}

/// A string value, or a number/bool rendered as text.
pub fn string(d: &plist::Dictionary, key: &str) -> Option<String> {
    match d.get(key)? {
        plist::Value::String(s) => Some(s.clone()),
        plist::Value::Integer(i) => Some(i.to_string()),
        plist::Value::Real(r) => Some(r.to_string()),
        plist::Value::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

/// A signed integer. Apple sometimes sends negative amperages as the
/// two's-complement unsigned value; those wrap back to negative here.
pub fn int(d: &plist::Dictionary, key: &str) -> Option<i64> {
    match d.get(key)? {
        plist::Value::Integer(i) => i.as_signed().or_else(|| i.as_unsigned().map(|u| u as i64)),
        plist::Value::Real(r) => Some(*r as i64),
        plist::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// A non-negative integer (values below zero are rejected).
pub fn uint(d: &plist::Dictionary, key: &str) -> Option<u64> {
    match d.get(key)? {
        plist::Value::Integer(i) => i.as_unsigned(),
        plist::Value::Real(r) if *r >= 0.0 => Some(*r as u64),
        plist::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// A boolean (also accepts 0/1 integers).
pub fn bool(d: &plist::Dictionary, key: &str) -> Option<bool> {
    match d.get(key)? {
        plist::Value::Boolean(b) => Some(*b),
        plist::Value::Integer(i) => i.as_signed().map(|n| n != 0),
        _ => None,
    }
}

/// A nested dictionary.
pub fn dict<'a>(d: &'a plist::Dictionary, key: &str) -> Option<&'a plist::Dictionary> {
    d.get(key)?.as_dictionary()
}

/// A flat one-line rendering of any plist value for `DeviceInfo::raw`.
pub fn render(value: &plist::Value) -> String {
    match value {
        plist::Value::String(s) => s.clone(),
        plist::Value::Integer(i) => i.to_string(),
        plist::Value::Real(r) => r.to_string(),
        plist::Value::Boolean(b) => b.to_string(),
        plist::Value::Date(d) => d.to_xml_format(),
        plist::Value::Data(bytes) => format!("<{} bytes>", bytes.len()),
        plist::Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(render).collect();
            format!("[{}]", parts.join(", "))
        }
        plist::Value::Dictionary(d) => {
            let parts: Vec<String> = d
                .iter()
                .map(|(k, v)| format!("{k}={}", render(v)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
        _ => String::from("?"),
    }
}

fn type_name(v: &plist::Value) -> &'static str {
    match v {
        plist::Value::String(_) => "string",
        plist::Value::Integer(_) => "integer",
        plist::Value::Real(_) => "real",
        plist::Value::Boolean(_) => "boolean",
        plist::Value::Date(_) => "date",
        plist::Value::Data(_) => "data",
        plist::Value::Array(_) => "array",
        plist::Value::Dictionary(_) => "dictionary",
        _ => "unknown",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn sample() -> plist::Dictionary {
        let mut d = plist::Dictionary::new();
        d.insert("s".into(), plist::Value::String("x".into()));
        d.insert("n".into(), plist::Value::Integer(42.into()));
        d.insert("neg".into(), plist::Value::Integer((-7i64).into()));
        d.insert(
            "wrapped".into(),
            plist::Value::Integer(18_446_744_073_709_551_000u64.into()),
        );
        d.insert("b".into(), plist::Value::Boolean(true));
        d.insert("one".into(), plist::Value::Integer(1.into()));
        d.insert("r".into(), plist::Value::Real(2.5));
        d
    }

    #[test]
    fn lookups() {
        let d = sample();
        assert_eq!(string(&d, "s").as_deref(), Some("x"));
        assert_eq!(string(&d, "n").as_deref(), Some("42"));
        assert_eq!(int(&d, "n"), Some(42));
        assert_eq!(int(&d, "neg"), Some(-7));
        assert_eq!(int(&d, "wrapped"), Some(-616));
        assert_eq!(uint(&d, "neg"), None);
        assert_eq!(uint(&d, "n"), Some(42));
        assert_eq!(bool(&d, "b"), Some(true));
        assert_eq!(bool(&d, "one"), Some(true));
        assert_eq!(int(&d, "r"), Some(2));
        assert_eq!(string(&d, "missing"), None);
    }

    #[test]
    fn render_is_flat() {
        let d = sample();
        let mut outer = plist::Dictionary::new();
        outer.insert("inner".into(), plist::Value::Dictionary(d));
        outer.insert(
            "list".into(),
            plist::Value::Array(vec![1.into(), "a".into()]),
        );
        outer.insert("data".into(), plist::Value::Data(vec![1, 2, 3]));
        let text = render(&plist::Value::Dictionary(outer));
        assert!(text.contains("s=x"));
        assert!(text.contains("list=[1, a]"));
        assert!(text.contains("data=<3 bytes>"));
    }
}
