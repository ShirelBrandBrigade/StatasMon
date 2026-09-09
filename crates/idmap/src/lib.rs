//! StatasMon `idmap` crate (docs/PLAN.md §3.3): platform identifiers →
//! marketing names. Pure tables, no I/O, no dependencies.
//!
//! - [`android`]: `ro.soc.model` / platform codenames → SoC marketing names.
//! - iOS ProductType / ChipID tables arrive in Phase 3 (step 3.2).

pub mod android;

pub use android::{model_marketing_name, soc_manufacturer_name, soc_marketing_name};
