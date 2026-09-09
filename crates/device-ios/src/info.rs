//! The full `DeviceInfo` sheet for an iOS / iPadOS device (step 3.2,
//! docs/FEATURES.md §1, docs/research/ios.md §2 "Device info").
//!
//! `read_inputs` gathers every dictionary over the wire; `build_info` is a
//! pure function over those dictionaries and is tested against
//! `fixtures/ios/**`. Anything lockdown cannot provide stays `None`: RAM
//! and uptime need the DVT tunnel (Phase 4), so at tier 1 they are dashed
//! with a note in `raw["statasmon/notes"]`.

use std::collections::BTreeMap;
use std::path::Path;

use idevice::IdeviceService;
use idevice::pairing_file::PairingFile;
use idevice::provider::IdeviceProvider;
use idevice::services::diagnostics_relay::DiagnosticsRelayClient;
use statasmon_idmap::apple;
use statasmon_protocol::{
    ConnectionInfo, DeviceHardware, DeviceIdentity, DeviceInfo, DeviceSoftware, DeviceState,
    ToolVersion, Transport,
};

use crate::battery::{self, BatteryDetail};
use crate::error::IosError;
use crate::lockdown::{self, Lockdown, bool as pbool, string, uint};

/// The `idevice` crate version reported under `connection.tools`.
pub const IDEVICE_VERSION: &str = "0.1.65";
/// MobileGestalt key tried for RAM; left `None` when the device withholds it.
pub const MG_PHYSICAL_MEMORY: &str = "PhysicalMemory";

/// Everything `build_info` needs, as read from the device (or a fixture).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InfoInputs {
    /// Lockdown `GetValue` with no domain and no key.
    pub root: plist::Dictionary,
    /// `com.apple.international`.
    pub international: Option<plist::Dictionary>,
    /// `com.apple.disk_usage`.
    pub disk_usage: Option<plist::Dictionary>,
    /// `com.apple.mobile.wireless_lockdown`.
    pub wireless: Option<plist::Dictionary>,
    /// `diagnostics_relay` MobileGestalt answer (whatever the device gave).
    pub mobilegestalt: Option<plist::Dictionary>,
    /// `IOPMPowerSource` IORegistry entry, for the design capacity.
    pub ioregistry_battery: Option<plist::Dictionary>,
    /// amfi `DeveloperModeStatus`.
    pub developer_mode: Option<bool>,
    /// Why a read failed ("amfi: ...") — shown under raw notes.
    pub notes: Vec<String>,
}

/// What the transport knows and lockdown does not.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConnectionFacts {
    pub udid: String,
    pub transport: Option<Transport>,
    /// "paired", "needs trust".
    pub pairing: Option<String>,
    /// "tier 1 (no tunnel)".
    pub tier: Option<String>,
    pub tunnel: Option<String>,
}

/// Read every input over the wire. Optional domains that fail are recorded
/// in `notes` instead of failing the whole sheet; the root dictionary is
/// mandatory.
pub async fn read_inputs(
    provider: &dyn IdeviceProvider,
    pair: &PairingFile,
) -> Result<InfoInputs, IosError> {
    let mut ld = Lockdown::connect(provider, pair).await?;
    let root = ld.root().await?;
    let mut inputs = InfoInputs {
        root,
        ..InfoInputs::default()
    };
    match ld.international().await {
        Ok(d) => inputs.international = Some(d),
        Err(e) => inputs.notes.push(format!("com.apple.international: {e}")),
    }
    match ld.disk_usage().await {
        Ok(d) => inputs.disk_usage = Some(d),
        Err(e) => inputs.notes.push(format!("com.apple.disk_usage: {e}")),
    }
    match ld.wireless_lockdown().await {
        Ok(d) => inputs.wireless = Some(d),
        Err(e) => inputs
            .notes
            .push(format!("com.apple.mobile.wireless_lockdown: {e}")),
    }
    drop(ld);

    match lockdown::developer_mode_status(provider).await {
        Ok(on) => inputs.developer_mode = Some(on),
        Err(e) => inputs.notes.push(format!("amfi DeveloperModeStatus: {e}")),
    }

    match DiagnosticsRelayClient::connect(provider).await {
        Ok(mut diag) => {
            match diag
                .mobilegestalt(Some(vec![MG_PHYSICAL_MEMORY.to_owned()]))
                .await
            {
                Ok(Some(d)) => inputs.mobilegestalt = Some(d),
                Ok(None) => inputs.notes.push("MobileGestalt: empty answer".to_owned()),
                Err(e) => inputs.notes.push(format!("MobileGestalt: {e}")),
            }
            match battery::fetch_ioregistry(&mut diag).await {
                Ok(d) => inputs.ioregistry_battery = Some(d),
                Err(e) => inputs
                    .notes
                    .push(format!("IORegistry IOPMPowerSource: {e}")),
            }
            let _ = diag.goodbye().await;
        }
        Err(e) => inputs.notes.push(format!("diagnostics_relay: {e}")),
    }
    Ok(inputs)
}

/// Read the inputs and build the sheet.
pub async fn read_info(
    provider: &dyn IdeviceProvider,
    pair: &PairingFile,
    facts: &ConnectionFacts,
) -> Result<DeviceInfo, IosError> {
    let inputs = read_inputs(provider, pair).await?;
    Ok(build_info(&inputs, facts))
}

/// Load the inputs from a fixture directory written by
/// `examples/capture_fixture.rs` (`lockdown_root.plist`, ...).
pub fn load_inputs(dir: &Path) -> Result<InfoInputs, IosError> {
    let read = |name: &str| -> Result<Option<plist::Dictionary>, IosError> {
        let path = dir.join(name);
        if !path.is_file() {
            return Ok(None);
        }
        let value: plist::Value = plist::from_file(&path)?;
        value
            .into_dictionary()
            .map(Some)
            .ok_or_else(|| IosError::Parse(format!("{} is not a plist dictionary", path.display())))
    };
    let root = read("lockdown_root.plist")?.ok_or_else(|| {
        IosError::Parse(format!("{}: lockdown_root.plist missing", dir.display()))
    })?;
    let developer_mode = read("developer_mode.plist")?.and_then(|d| pbool(&d, "status"));
    Ok(InfoInputs {
        root,
        international: read("lockdown_international.plist")?,
        disk_usage: read("lockdown_disk_usage.plist")?,
        wireless: read("lockdown_wireless_lockdown.plist")?,
        mobilegestalt: read("mobilegestalt.plist")?,
        ioregistry_battery: read("ioregistry_battery.plist")?,
        developer_mode,
        notes: Vec::new(),
    })
}

/// Build the sheet (pure).
pub fn build_info(inputs: &InfoInputs, facts: &ConnectionFacts) -> DeviceInfo {
    let root = &inputs.root;
    let get = |k: &str| string(root, k);
    let product_type = get("ProductType");
    let device_class = get("DeviceClass");
    let mut notes = inputs.notes.clone();

    let identity = DeviceIdentity {
        name: get("DeviceName"),
        marketing_name: product_type
            .as_deref()
            .and_then(apple::product_marketing_name)
            .map(str::to_owned),
        model_identifier: product_type.clone(),
        model_number: model_number(root),
        manufacturer: Some("Apple".to_owned()),
        serial_number: get("SerialNumber"),
        unique_id: get("UniqueDeviceID").or_else(|| Some(facts.udid.clone())),
        region: get("RegionInfo"),
    };

    let os_version = get("ProductVersion");
    let software = DeviceSoftware {
        os_name: os_name(device_class.as_deref(), os_version.as_deref()),
        os_version,
        os_build: get("BuildVersion"),
        security_patch: None,
        kernel_version: None,
        api_level: None,
        firmware_version: get("BasebandVersion").or_else(|| get("FirmwareVersion")),
    };

    let platform = get("HardwarePlatform");
    let chip_name = platform
        .as_deref()
        .and_then(apple::chip_name_for_platform)
        .or_else(|| product_type.as_deref().and_then(apple::chip_for_product))
        .map(str::to_owned);
    let layout = chip_name.as_deref().and_then(apple::chip_core_layout);
    let ram_bytes = ram_bytes(inputs.mobilegestalt.as_ref());
    if ram_bytes.is_none() {
        notes.push(
            "RAM: not reported by lockdown or MobileGestalt at tier 1 (DVT hardwareInformation, Phase 4)"
                .to_owned(),
        );
    }
    let design_mah = inputs
        .ioregistry_battery
        .as_ref()
        .map(BatteryDetail::parse)
        .and_then(|b| b.design_mah)
        .and_then(|v| u32::try_from(v).ok());
    let hardware = DeviceHardware {
        chip_name,
        chip_identifier: platform.clone().or_else(|| chip_id_hex(root)),
        chip_manufacturer: Some("Apple".to_owned()),
        architecture: get("CPUArchitecture"),
        abis: get("CPUArchitecture").into_iter().collect(),
        cpu_cores: layout.map(|l| l.total),
        performance_cores: layout.map(|l| l.performance),
        efficiency_cores: layout.map(|l| l.efficiency),
        cpu_max_mhz: None,
        gpu_name: None,
        ram_bytes,
        storage_bytes: inputs
            .disk_usage
            .as_ref()
            .and_then(|d| uint(d, "TotalDiskCapacity")),
        battery_design_mah: design_mah,
        board: get("HardwareModel"),
        screen: None,
    };
    notes.push("Uptime: needs the developer tunnel (Phase 4)".to_owned());

    let state = DeviceState {
        uptime_secs: None,
        boot_time_ms: None,
        time_zone: get("TimeZone"),
        locale: inputs.international.as_ref().and_then(|d| {
            string(d, "Locale")
                .or_else(|| string(d, "Language"))
                .or_else(|| string(d, "SupportedLanguages"))
        }),
        developer_mode: inputs.developer_mode,
        usb_debugging: None,
        wireless_debugging: None,
        selinux_mode: None,
    };

    let connection = ConnectionInfo {
        transport: facts.transport,
        tier: facts.tier.clone(),
        pairing: facts.pairing.clone(),
        tunnel: facts.tunnel.clone(),
        last_heartbeat_ms: None,
        tools: vec![ToolVersion {
            name: "idevice".to_owned(),
            version: IDEVICE_VERSION.to_owned(),
        }],
    };

    let mut raw = BTreeMap::new();
    for (k, v) in root {
        raw.insert(k.clone(), lockdown::render(v));
    }
    let mut add_domain = |domain: &str, d: Option<&plist::Dictionary>| {
        if let Some(d) = d {
            for (k, v) in d {
                raw.insert(format!("{domain}/{k}"), lockdown::render(v));
            }
        }
    };
    add_domain(
        lockdown::DOMAIN_INTERNATIONAL,
        inputs.international.as_ref(),
    );
    add_domain(lockdown::DOMAIN_DISK_USAGE, inputs.disk_usage.as_ref());
    add_domain(lockdown::DOMAIN_WIRELESS, inputs.wireless.as_ref());
    add_domain("MobileGestalt", inputs.mobilegestalt.as_ref());
    if let Some(on) = inputs.developer_mode {
        raw.insert(
            format!("{}/DeveloperModeStatus", lockdown::DOMAIN_AMFI),
            on.to_string(),
        );
    }
    if !notes.is_empty() {
        raw.insert("statasmon/notes".to_owned(), notes.join("; "));
    }

    DeviceInfo {
        identity,
        software,
        hardware,
        state,
        connection,
        raw,
    }
}

/// "iPadOS" for iPads on 13+, "iOS" otherwise.
fn os_name(device_class: Option<&str>, version: Option<&str>) -> Option<String> {
    let major: u32 = version?
        .split('.')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    Some(match device_class {
        Some("iPad") if major >= 13 => "iPadOS".to_owned(),
        Some(_) | None => "iOS".to_owned(),
    })
}

/// `ModelNumber` + `RegionInfo` ("MPQ03" + "LL/A" → "MPQ03LL/A").
fn model_number(root: &plist::Dictionary) -> Option<String> {
    let model = string(root, "ModelNumber")?;
    match string(root, "RegionInfo") {
        Some(region) if !model.ends_with(&region) => Some(format!("{model}{region}")),
        _ => Some(model),
    }
}

/// `ChipID` as "0x8101" when `HardwarePlatform` is absent.
fn chip_id_hex(root: &plist::Dictionary) -> Option<String> {
    uint(root, "ChipID").map(|id| format!("0x{id:x}"))
}

/// RAM from the MobileGestalt answer, only when it is a plausible byte
/// count (≥ 256 MiB). The answer is either flat or nested under
/// `MobileGestalt`.
pub fn ram_bytes(mg: Option<&plist::Dictionary>) -> Option<u64> {
    let mg = mg?;
    let d = lockdown::dict(mg, "MobileGestalt").unwrap_or(mg);
    let v = uint(d, MG_PHYSICAL_MEMORY)?;
    (v >= 256 << 20).then_some(v)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn root() -> plist::Dictionary {
        let mut d = plist::Dictionary::new();
        for (k, v) in [
            ("DeviceName", "Test iPad"),
            ("ProductType", "iPad13,18"),
            ("ProductVersion", "17.4.1"),
            ("BuildVersion", "21E237"),
            ("HardwareModel", "J271AP"),
            ("HardwarePlatform", "t8101"),
            ("CPUArchitecture", "arm64e"),
            ("DeviceClass", "iPad"),
            ("SerialNumber", "SERIAL0000"),
            ("UniqueDeviceID", "00000000-000000000000AAAA"),
            ("ModelNumber", "MPQ03"),
            ("RegionInfo", "LL/A"),
            ("TimeZone", "Asia/Jerusalem"),
        ] {
            d.insert(k.into(), plist::Value::String(v.into()));
        }
        d.insert("ChipID".into(), plist::Value::Integer(0x8101.into()));
        d
    }

    #[test]
    fn builds_the_ipad_sheet() {
        let mut intl = plist::Dictionary::new();
        intl.insert("Locale".into(), "en_IL".into());
        let mut disk = plist::Dictionary::new();
        disk.insert(
            "TotalDiskCapacity".into(),
            plist::Value::Integer(64_000_000_000u64.into()),
        );
        let inputs = InfoInputs {
            root: root(),
            international: Some(intl),
            disk_usage: Some(disk),
            developer_mode: Some(false),
            ..InfoInputs::default()
        };
        let facts = ConnectionFacts {
            udid: "00000000-000000000000AAAA".into(),
            transport: Some(Transport::Usb),
            pairing: Some("paired".into()),
            tier: Some("tier 1 (no tunnel)".into()),
            tunnel: None,
        };
        let info = build_info(&inputs, &facts);
        assert_eq!(
            info.identity.marketing_name.as_deref(),
            Some("iPad (10th generation)")
        );
        assert_eq!(info.identity.model_number.as_deref(), Some("MPQ03LL/A"));
        assert_eq!(info.software.os_name.as_deref(), Some("iPadOS"));
        assert_eq!(info.hardware.chip_name.as_deref(), Some("A14 Bionic"));
        assert_eq!(info.hardware.chip_identifier.as_deref(), Some("t8101"));
        assert_eq!(info.hardware.cpu_cores, Some(6));
        assert_eq!(info.hardware.performance_cores, Some(2));
        assert_eq!(info.hardware.storage_bytes, Some(64_000_000_000));
        assert_eq!(info.hardware.ram_bytes, None);
        assert_eq!(info.state.locale.as_deref(), Some("en_IL"));
        assert_eq!(info.state.developer_mode, Some(false));
        assert_eq!(info.state.uptime_secs, None);
        assert_eq!(info.connection.transport, Some(Transport::Usb));
        assert_eq!(
            info.raw.get("ProductType").map(String::as_str),
            Some("iPad13,18")
        );
        assert_eq!(
            info.raw
                .get("com.apple.disk_usage/TotalDiskCapacity")
                .map(String::as_str),
            Some("64000000000")
        );
        assert!(info.raw["statasmon/notes"].contains("RAM"));
    }

    #[test]
    fn unknown_product_is_not_guessed() {
        let mut r = root();
        r.insert("ProductType".into(), "iPad99,9".into());
        r.remove("HardwarePlatform");
        let info = build_info(
            &InfoInputs {
                root: r,
                ..InfoInputs::default()
            },
            &ConnectionFacts::default(),
        );
        assert_eq!(info.identity.marketing_name, None);
        assert_eq!(info.hardware.chip_name, None);
        assert_eq!(info.hardware.chip_identifier.as_deref(), Some("0x8101"));
        assert_eq!(info.hardware.cpu_cores, None);
    }

    #[test]
    fn ram_needs_a_plausible_byte_count() {
        let mut mg = plist::Dictionary::new();
        mg.insert(
            MG_PHYSICAL_MEMORY.into(),
            plist::Value::Integer(4096.into()),
        );
        assert_eq!(ram_bytes(Some(&mg)), None);
        let mut inner = plist::Dictionary::new();
        inner.insert(
            MG_PHYSICAL_MEMORY.into(),
            plist::Value::Integer(4_294_967_296u64.into()),
        );
        let mut outer = plist::Dictionary::new();
        outer.insert("MobileGestalt".into(), plist::Value::Dictionary(inner));
        assert_eq!(ram_bytes(Some(&outer)), Some(4_294_967_296));
    }

    #[test]
    fn os_name_by_class_and_version() {
        assert_eq!(
            os_name(Some("iPad"), Some("17.4.1")).as_deref(),
            Some("iPadOS")
        );
        assert_eq!(os_name(Some("iPad"), Some("12.5")).as_deref(), Some("iOS"));
        assert_eq!(
            os_name(Some("iPhone"), Some("18.0")).as_deref(),
            Some("iOS")
        );
        assert_eq!(os_name(None, None), None);
    }
}
