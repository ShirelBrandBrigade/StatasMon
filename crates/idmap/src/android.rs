//! Android SoC identifiers → marketing names.
//!
//! `ro.soc.model` (API 31+) carries the vendor part number ("SM8450",
//! "s5e9925", "MT6989"); older builds only have `ro.hardware` /
//! `ro.board.platform` codenames ("taro", "zuma"). Both shapes are looked
//! up here, case-insensitively. Unknown identifiers return `None` so the
//! caller shows the raw identifier instead of a guess.

/// The marketing name for a SoC identifier, if we know it.
pub fn soc_marketing_name(identifier: &str) -> Option<&'static str> {
    let id = identifier.trim();
    if id.is_empty() {
        return None;
    }
    let upper = id.to_ascii_uppercase();
    SOCS.iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(&upper))
        .map(|(_, name)| *name)
}

/// A friendlier SoC vendor name for `ro.soc.manufacturer` values.
pub fn soc_manufacturer_name(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_uppercase().as_str() {
        "QTI" | "QUALCOMM" => Some("Qualcomm"),
        "SAMSUNG" => Some("Samsung"),
        "MEDIATEK" | "MTK" => Some("MediaTek"),
        "GOOGLE" => Some("Google"),
        "UNISOC" | "SPREADTRUM" => Some("Unisoc"),
        "HISILICON" | "HUAWEI" => Some("HiSilicon"),
        _ => None,
    }
}

/// (identifier, marketing name). Identifiers are stored upper-case.
const SOCS: &[(&str, &str)] = &[
    // Qualcomm Snapdragon 8 series
    ("SM8850", "Snapdragon 8 Elite Gen 5"),
    ("SM8750", "Snapdragon 8 Elite"),
    ("SM8735", "Snapdragon 8s Gen 4"),
    ("SM8650", "Snapdragon 8 Gen 3"),
    ("SM8635", "Snapdragon 8s Gen 3"),
    ("SM8550", "Snapdragon 8 Gen 2"),
    ("SM8475", "Snapdragon 8+ Gen 1"),
    ("SM8450", "Snapdragon 8 Gen 1"),
    ("SM8350", "Snapdragon 888"),
    ("SM8250", "Snapdragon 865"),
    ("SM8150", "Snapdragon 855"),
    // Qualcomm Snapdragon 7 series
    ("SM7675", "Snapdragon 7+ Gen 3"),
    ("SM7635", "Snapdragon 7s Gen 3"),
    ("SM7550", "Snapdragon 7 Gen 3"),
    ("SM7475", "Snapdragon 7+ Gen 2"),
    ("SM7450", "Snapdragon 7 Gen 1"),
    ("SM7435", "Snapdragon 7s Gen 2"),
    ("SM7350", "Snapdragon 780G"),
    ("SM7325", "Snapdragon 778G"),
    ("SM7250", "Snapdragon 765G"),
    ("SM7225", "Snapdragon 750G"),
    ("SM7150", "Snapdragon 730"),
    // Qualcomm Snapdragon 6 and 4 series
    ("SM6475", "Snapdragon 6s Gen 3"),
    ("SM6450", "Snapdragon 6 Gen 1"),
    ("SM6375", "Snapdragon 695"),
    ("SM6350", "Snapdragon 690"),
    ("SM6225", "Snapdragon 680"),
    ("SM6150", "Snapdragon 675"),
    ("SM6125", "Snapdragon 665"),
    ("SM4450", "Snapdragon 4 Gen 2"),
    ("SM4375", "Snapdragon 4 Gen 1"),
    ("SM4350", "Snapdragon 480"),
    // Qualcomm platform codenames (ro.board.platform)
    ("TARO", "Snapdragon 8 Gen 1"),
    ("KALAMA", "Snapdragon 8 Gen 2"),
    ("PINEAPPLE", "Snapdragon 8 Gen 3"),
    ("SUN", "Snapdragon 8 Elite"),
    ("LAHAINA", "Snapdragon 888"),
    ("KONA", "Snapdragon 865"),
    ("MSMNILE", "Snapdragon 855"),
    // Samsung Exynos (ro.soc.model is the s5e part number on newer builds)
    ("S5E9955", "Exynos 2500"),
    ("S5E9945", "Exynos 2400"),
    ("S5E9925", "Exynos 2200"),
    ("EXYNOS2100", "Exynos 2100"),
    ("EXYNOS990", "Exynos 990"),
    ("EXYNOS9825", "Exynos 9825"),
    ("EXYNOS9820", "Exynos 9820"),
    ("EXYNOS9810", "Exynos 9810"),
    ("S5E8855", "Exynos 1580"),
    ("S5E8845", "Exynos 1480"),
    ("S5E8835", "Exynos 1380"),
    ("S5E8825", "Exynos 1280"),
    ("EXYNOS1080", "Exynos 1080"),
    ("EXYNOS850", "Exynos 850"),
    ("UNIVERSAL9611", "Exynos 9611"),
    // MediaTek Dimensity / Helio
    ("MT6991", "Dimensity 9400"),
    ("MT6989", "Dimensity 9300"),
    ("MT6985", "Dimensity 9200"),
    ("MT6983", "Dimensity 9000"),
    ("MT6899", "Dimensity 8400"),
    ("MT6897", "Dimensity 8300"),
    ("MT6896", "Dimensity 8200"),
    ("MT6895", "Dimensity 8100"),
    ("MT6893", "Dimensity 1200"),
    ("MT6891", "Dimensity 1100"),
    ("MT6889", "Dimensity 1000+"),
    ("MT6886", "Dimensity 7200"),
    ("MT6878", "Dimensity 7050"),
    ("MT6877", "Dimensity 900"),
    ("MT6873", "Dimensity 800"),
    ("MT6853", "Dimensity 720"),
    ("MT6833", "Dimensity 700"),
    ("MT6789", "Helio G99"),
    ("MT6785", "Helio G90T"),
    ("MT6769", "Helio G80"),
    ("MT6768", "Helio P65"),
    ("MT6765", "Helio P35"),
    ("MT6762", "Helio P22"),
    // Google Tensor (ro.soc.model on Pixels, plus ro.hardware codenames)
    ("GS101", "Tensor"),
    ("GS201", "Tensor G2"),
    ("TENSOR G3", "Tensor G3"),
    ("TENSOR G4", "Tensor G4"),
    ("TENSOR G5", "Tensor G5"),
    ("ZUMA", "Tensor G3"),
    ("ZUMAPRO", "Tensor G4"),
    ("LAGUNA", "Tensor G5"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_socs_map_to_marketing_names() {
        assert_eq!(soc_marketing_name("SM8450"), Some("Snapdragon 8 Gen 1"));
        assert_eq!(soc_marketing_name("sm8550"), Some("Snapdragon 8 Gen 2"));
        assert_eq!(soc_marketing_name("SM8650"), Some("Snapdragon 8 Gen 3"));
        assert_eq!(soc_marketing_name("SM8750"), Some("Snapdragon 8 Elite"));
        assert_eq!(soc_marketing_name("s5e9925"), Some("Exynos 2200"));
        assert_eq!(soc_marketing_name("MT6989"), Some("Dimensity 9300"));
        assert_eq!(soc_marketing_name("Tensor G3"), Some("Tensor G3"));
        assert_eq!(soc_marketing_name("zumapro"), Some("Tensor G4"));
        assert_eq!(soc_marketing_name(" taro "), Some("Snapdragon 8 Gen 1"));
    }

    #[test]
    fn unknown_and_empty_return_none() {
        assert_eq!(soc_marketing_name("SM9999"), None);
        assert_eq!(soc_marketing_name(""), None);
        assert_eq!(soc_marketing_name("   "), None);
    }

    #[test]
    fn manufacturer_names() {
        assert_eq!(soc_manufacturer_name("QTI"), Some("Qualcomm"));
        assert_eq!(soc_manufacturer_name("Mediatek"), Some("MediaTek"));
        assert_eq!(soc_manufacturer_name("acme"), None);
    }
}
