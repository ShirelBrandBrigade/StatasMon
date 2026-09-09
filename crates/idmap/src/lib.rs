//! StatasMon `idmap` crate (docs/PLAN.md §3.3): platform identifiers →
//! marketing names. Pure tables, no I/O, no dependencies.
//!
//! - [`android`]: `ro.soc.model` / platform codenames → SoC marketing names.
//! - [`apple`]: lockdown `ProductType` / `HardwarePlatform` → device and
//!   chip marketing names, core layouts (step 3.2).

pub mod android;
pub mod apple;

pub use android::{model_marketing_name, soc_manufacturer_name, soc_marketing_name};
pub use apple::{
    CoreLayout, chip_core_layout, chip_for_product, chip_name_for_platform, product_family,
    product_marketing_name,
};
