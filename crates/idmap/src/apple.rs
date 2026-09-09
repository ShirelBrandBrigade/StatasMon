//! Apple identifiers → marketing names (docs/PLAN.md step 3.2,
//! docs/research/ios.md §5).
//!
//! Sources: AppleDB (github.com/littlebyteorg/appledb) and
//! theapplewiki.com/wiki/CHIP, snapshot September 2026. Every entry here is
//! one the author is sure of; an unknown identifier returns `None` so the
//! UI shows the raw `ProductType` / `HardwarePlatform` instead of a guess.
//! `t8140` is deliberately absent from the platform table because Apple
//! reports it for both the A18 and the A18 Pro; the product table decides.

/// Core layout of a chip: total, performance, efficiency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreLayout {
    pub total: u32,
    pub performance: u32,
    pub efficiency: u32,
}

/// Marketing name for a lockdown `ProductType` ("iPad13,18").
pub fn product_marketing_name(product_type: &str) -> Option<&'static str> {
    lookup(PRODUCTS, product_type).map(|(name, _)| name)
}

/// Chip marketing name for a lockdown `ProductType` (fallback when the
/// platform identifier is ambiguous or missing).
pub fn chip_for_product(product_type: &str) -> Option<&'static str> {
    lookup(PRODUCTS, product_type).map(|(_, chip)| chip)
}

/// Chip marketing name for a lockdown `HardwarePlatform` ("t8101") or a
/// `ChipID` rendered as "0x8101" / "8101" / 33025.
pub fn chip_name_for_platform(platform: &str) -> Option<&'static str> {
    let p = platform.trim().to_ascii_lowercase();
    let key = if let Some(hex) = p.strip_prefix("0x") {
        format!("t{hex}")
    } else if p.chars().all(|c| c.is_ascii_digit()) {
        let n: u64 = p.parse().ok()?;
        format!("t{n:x}")
    } else {
        p
    };
    PLATFORMS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, name)| *name)
}

/// Core layout for a chip marketing name ("A14 Bionic" → 6 = 2P + 4E).
/// `None` when the layout depends on the storage tier (M4 iPad Pro).
pub fn chip_core_layout(chip_name: &str) -> Option<CoreLayout> {
    let layout = |total, performance, efficiency| CoreLayout {
        total,
        performance,
        efficiency,
    };
    match chip_name.trim() {
        "A12 Bionic" | "A13 Bionic" | "A14 Bionic" | "A15 Bionic" | "A16 Bionic" | "A17 Pro"
        | "A18" | "A18 Pro" | "A19 Pro" => Some(layout(6, 2, 4)),
        "A12Z Bionic" | "M1" | "M2" | "M3" => Some(layout(8, 4, 4)),
        _ => None,
    }
}

/// The platform family for a `ProductType` ("iPhone", "iPad").
pub fn product_family(product_type: &str) -> Option<&'static str> {
    let p = product_type.trim();
    if p.starts_with("iPhone") {
        Some("iPhone")
    } else if p.starts_with("iPad") {
        Some("iPad")
    } else if p.starts_with("iPod") {
        Some("iPod touch")
    } else {
        None
    }
}

fn lookup<'a>(
    table: &'a [(&'static str, &'static str, &'static str)],
    key: &str,
) -> Option<(&'a str, &'a str)> {
    let key = key.trim();
    table
        .iter()
        .find(|(k, _, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, name, chip)| (*name, *chip))
}

/// (ProductType, marketing name, chip).
const PRODUCTS: &[(&str, &str, &str)] = &[
    // iPhone 11 family (2019) and iPhone SE (2nd generation)
    ("iPhone12,1", "iPhone 11", "A13 Bionic"),
    ("iPhone12,3", "iPhone 11 Pro", "A13 Bionic"),
    ("iPhone12,5", "iPhone 11 Pro Max", "A13 Bionic"),
    ("iPhone12,8", "iPhone SE (2nd generation)", "A13 Bionic"),
    // iPhone 12 family (2020)
    ("iPhone13,1", "iPhone 12 mini", "A14 Bionic"),
    ("iPhone13,2", "iPhone 12", "A14 Bionic"),
    ("iPhone13,3", "iPhone 12 Pro", "A14 Bionic"),
    ("iPhone13,4", "iPhone 12 Pro Max", "A14 Bionic"),
    // iPhone 13 family (2021) and iPhone SE (3rd generation)
    ("iPhone14,4", "iPhone 13 mini", "A15 Bionic"),
    ("iPhone14,5", "iPhone 13", "A15 Bionic"),
    ("iPhone14,2", "iPhone 13 Pro", "A15 Bionic"),
    ("iPhone14,3", "iPhone 13 Pro Max", "A15 Bionic"),
    ("iPhone14,6", "iPhone SE (3rd generation)", "A15 Bionic"),
    // iPhone 14 family (2022)
    ("iPhone14,7", "iPhone 14", "A15 Bionic"),
    ("iPhone14,8", "iPhone 14 Plus", "A15 Bionic"),
    ("iPhone15,2", "iPhone 14 Pro", "A16 Bionic"),
    ("iPhone15,3", "iPhone 14 Pro Max", "A16 Bionic"),
    // iPhone 15 family (2023)
    ("iPhone15,4", "iPhone 15", "A16 Bionic"),
    ("iPhone15,5", "iPhone 15 Plus", "A16 Bionic"),
    ("iPhone16,1", "iPhone 15 Pro", "A17 Pro"),
    ("iPhone16,2", "iPhone 15 Pro Max", "A17 Pro"),
    // iPhone 16 family (2024) and iPhone 16e (2025)
    ("iPhone17,1", "iPhone 16 Pro", "A18 Pro"),
    ("iPhone17,2", "iPhone 16 Pro Max", "A18 Pro"),
    ("iPhone17,3", "iPhone 16", "A18"),
    ("iPhone17,4", "iPhone 16 Plus", "A18"),
    ("iPhone17,5", "iPhone 16e", "A18"),
    // iPhone 17 Pro family (2025); the non-Pro 17 and the iPhone Air share
    // the iPhone18,3/18,4 identifiers in an order not confirmed here.
    ("iPhone18,1", "iPhone 17 Pro", "A19 Pro"),
    ("iPhone18,2", "iPhone 17 Pro Max", "A19 Pro"),
    // iPad (8th–11th generation)
    ("iPad11,6", "iPad (8th generation)", "A12 Bionic"),
    ("iPad11,7", "iPad (8th generation)", "A12 Bionic"),
    ("iPad12,1", "iPad (9th generation)", "A13 Bionic"),
    ("iPad12,2", "iPad (9th generation)", "A13 Bionic"),
    ("iPad13,18", "iPad (10th generation)", "A14 Bionic"),
    ("iPad13,19", "iPad (10th generation)", "A14 Bionic"),
    ("iPad15,7", "iPad (A16)", "A16 Bionic"),
    ("iPad15,8", "iPad (A16)", "A16 Bionic"),
    // iPad Air (4th–7th generation)
    ("iPad13,1", "iPad Air (4th generation)", "A14 Bionic"),
    ("iPad13,2", "iPad Air (4th generation)", "A14 Bionic"),
    ("iPad13,16", "iPad Air (5th generation)", "M1"),
    ("iPad13,17", "iPad Air (5th generation)", "M1"),
    ("iPad14,8", "iPad Air 11-inch (M2)", "M2"),
    ("iPad14,9", "iPad Air 11-inch (M2)", "M2"),
    ("iPad14,10", "iPad Air 13-inch (M2)", "M2"),
    ("iPad14,11", "iPad Air 13-inch (M2)", "M2"),
    ("iPad15,3", "iPad Air 11-inch (M3)", "M3"),
    ("iPad15,4", "iPad Air 11-inch (M3)", "M3"),
    ("iPad15,5", "iPad Air 13-inch (M3)", "M3"),
    ("iPad15,6", "iPad Air 13-inch (M3)", "M3"),
    // iPad mini (6th–7th generation)
    ("iPad14,1", "iPad mini (6th generation)", "A15 Bionic"),
    ("iPad14,2", "iPad mini (6th generation)", "A15 Bionic"),
    ("iPad16,1", "iPad mini (A17 Pro)", "A17 Pro"),
    ("iPad16,2", "iPad mini (A17 Pro)", "A17 Pro"),
    // iPad Pro 2020 (A12Z)
    (
        "iPad8,9",
        "iPad Pro 11-inch (2nd generation)",
        "A12Z Bionic",
    ),
    (
        "iPad8,10",
        "iPad Pro 11-inch (2nd generation)",
        "A12Z Bionic",
    ),
    (
        "iPad8,11",
        "iPad Pro 12.9-inch (4th generation)",
        "A12Z Bionic",
    ),
    (
        "iPad8,12",
        "iPad Pro 12.9-inch (4th generation)",
        "A12Z Bionic",
    ),
    // iPad Pro 2021 (M1)
    ("iPad13,4", "iPad Pro 11-inch (3rd generation)", "M1"),
    ("iPad13,5", "iPad Pro 11-inch (3rd generation)", "M1"),
    ("iPad13,6", "iPad Pro 11-inch (3rd generation)", "M1"),
    ("iPad13,7", "iPad Pro 11-inch (3rd generation)", "M1"),
    ("iPad13,8", "iPad Pro 12.9-inch (5th generation)", "M1"),
    ("iPad13,9", "iPad Pro 12.9-inch (5th generation)", "M1"),
    ("iPad13,10", "iPad Pro 12.9-inch (5th generation)", "M1"),
    ("iPad13,11", "iPad Pro 12.9-inch (5th generation)", "M1"),
    // iPad Pro 2022 (M2)
    ("iPad14,3", "iPad Pro 11-inch (4th generation)", "M2"),
    ("iPad14,4", "iPad Pro 11-inch (4th generation)", "M2"),
    ("iPad14,5", "iPad Pro 12.9-inch (6th generation)", "M2"),
    ("iPad14,6", "iPad Pro 12.9-inch (6th generation)", "M2"),
    // iPad Pro 2024 (M4)
    ("iPad16,3", "iPad Pro 11-inch (M4)", "M4"),
    ("iPad16,4", "iPad Pro 11-inch (M4)", "M4"),
    ("iPad16,5", "iPad Pro 13-inch (M4)", "M4"),
    ("iPad16,6", "iPad Pro 13-inch (M4)", "M4"),
];

/// (HardwarePlatform, chip). Only unambiguous platforms.
const PLATFORMS: &[(&str, &str)] = &[
    ("t8020", "A12 Bionic"),
    ("t8027", "A12Z Bionic"),
    ("t8030", "A13 Bionic"),
    ("t8101", "A14 Bionic"),
    ("t8103", "M1"),
    ("t8110", "A15 Bionic"),
    ("t8112", "M2"),
    ("t8120", "A16 Bionic"),
    ("t8122", "M3"),
    ("t8130", "A17 Pro"),
    ("t8132", "M4"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipad_10th_generation() {
        assert_eq!(
            product_marketing_name("iPad13,18"),
            Some("iPad (10th generation)")
        );
        assert_eq!(
            product_marketing_name("iPad13,19"),
            Some("iPad (10th generation)")
        );
        assert_eq!(chip_for_product("iPad13,18"), Some("A14 Bionic"));
        assert_eq!(chip_name_for_platform("t8101"), Some("A14 Bionic"));
        assert_eq!(chip_name_for_platform("T8101"), Some("A14 Bionic"));
        assert_eq!(chip_name_for_platform("0x8101"), Some("A14 Bionic"));
        assert_eq!(chip_name_for_platform("33025"), Some("A14 Bionic"));
        assert_eq!(
            chip_core_layout("A14 Bionic"),
            Some(CoreLayout {
                total: 6,
                performance: 2,
                efficiency: 4
            })
        );
        assert_eq!(product_family("iPad13,18"), Some("iPad"));
    }

    #[test]
    fn iphones_and_m_series() {
        assert_eq!(product_marketing_name("iPhone17,1"), Some("iPhone 16 Pro"));
        assert_eq!(chip_for_product("iPhone17,3"), Some("A18"));
        assert_eq!(chip_for_product("iPhone16,1"), Some("A17 Pro"));
        assert_eq!(chip_name_for_platform("t8130"), Some("A17 Pro"));
        assert_eq!(chip_name_for_platform("t8103"), Some("M1"));
        assert_eq!(chip_core_layout("M2").map(|l| l.total), Some(8));
    }

    #[test]
    fn ambiguous_or_unknown_returns_none() {
        assert_eq!(chip_name_for_platform("t8140"), None);
        assert_eq!(product_marketing_name("iPad99,1"), None);
        assert_eq!(chip_for_product("iPhone18,3"), None);
        assert_eq!(chip_core_layout("M4"), None);
        assert_eq!(chip_name_for_platform(""), None);
        assert_eq!(product_family("Watch6,1"), None);
    }
}
