//! Storage parsers against `fixtures/ios/**`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use statasmon_device_ios::storage::{DiskUsage, apps_total, parse_app_sizes};
use statasmon_protocol::MetricId;

fn load(name: &str) -> plist::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/ios/ipad13-18-17.4.1")
        .join(name);
    plist::from_file(path).unwrap()
}

#[test]
fn disk_usage_totals() {
    let u = DiskUsage::parse(load("lockdown_disk_usage.plist").as_dictionary().unwrap());
    let total = u.total.unwrap();
    assert!(total > 32 << 30, "total {total}");
    assert!(u.data_available.unwrap() < total);
    assert!(u.used().unwrap() + u.data_available.unwrap() <= total);
    assert!(u.system().unwrap() < total / 2);
    let ids: Vec<MetricId> = u.metrics().iter().map(|m| m.id).collect();
    assert_eq!(
        ids,
        vec![
            MetricId::DiskTotal,
            MetricId::DiskUsed,
            MetricId::DiskFree,
            MetricId::DiskSystem
        ]
    );
}

#[test]
fn per_app_sizes() {
    let list = load("installation_proxy_sizes.plist");
    let apps = parse_app_sizes(list.as_array().unwrap());
    assert!(!apps.is_empty());
    assert!(
        apps.windows(2).all(|w| w[0].total() >= w[1].total()),
        "sorted largest first"
    );
    assert!(apps.iter().all(|a| !a.bundle_id.is_empty()));
    assert!(apps_total(&apps) > 0);
}
