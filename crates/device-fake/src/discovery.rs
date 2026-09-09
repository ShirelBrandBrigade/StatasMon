//! `FakeDiscovery`: reports the configured devices and lets a
//! [`FakeHandle`] unplug and re-plug them.

use std::collections::BTreeMap;

use async_trait::async_trait;
use statasmon_device::{DeviceResult, DeviceSource as _, Discovery, DiscoverySink};
use statasmon_protocol::DeviceId;
use tokio::sync::mpsc;

use crate::config::{FakeConfig, FakeDevice};
use crate::profiles::Profile;
use crate::source::FakeSource;

#[derive(Debug)]
enum Control {
    Detach(DeviceId),
    Reattach(DeviceId),
}

/// Control handle: simulate unplugging and re-plugging a fake device.
#[derive(Debug, Clone)]
pub struct FakeHandle {
    tx: mpsc::UnboundedSender<Control>,
}

/// The discovery finished (its task ended), so controls no longer apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("fake discovery is no longer running")]
pub struct DiscoveryGone;

impl FakeHandle {
    /// Report `device_id` as detached.
    pub fn detach(&self, device_id: &str) -> Result<(), DiscoveryGone> {
        self.tx
            .send(Control::Detach(device_id.to_owned()))
            .map_err(|_| DiscoveryGone)
    }

    /// Report `device_id` as attached again (a fresh source, same profile).
    pub fn reattach(&self, device_id: &str) -> Result<(), DiscoveryGone> {
        self.tx
            .send(Control::Reattach(device_id.to_owned()))
            .map_err(|_| DiscoveryGone)
    }
}

/// Reports the fake devices from a [`FakeConfig`].
#[derive(Debug)]
pub struct FakeDiscovery {
    config: FakeConfig,
    tx: mpsc::UnboundedSender<Control>,
    rx: mpsc::UnboundedReceiver<Control>,
}

impl FakeDiscovery {
    /// A discovery for `config`.
    pub fn new(config: FakeConfig) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self { config, tx, rx }
    }

    /// Defaults with the scenario from the environment.
    pub fn from_env() -> Self {
        Self::new(FakeConfig::from_env())
    }

    /// The configuration in use.
    pub fn config(&self) -> &FakeConfig {
        &self.config
    }

    /// A control handle (clone it freely).
    pub fn handle(&self) -> FakeHandle {
        FakeHandle {
            tx: self.tx.clone(),
        }
    }

    /// Ids this discovery will report, in attach order.
    pub fn device_ids(&self) -> Vec<DeviceId> {
        self.config
            .devices
            .iter()
            .map(|k| Profile::new(*k).id)
            .collect()
    }

    fn build(&self, kind: FakeDevice, index: usize) -> FakeSource {
        let seed = self
            .config
            .seed
            .wrapping_mul(0x9E37_79B9)
            .wrapping_add(index as u64 * 7919);
        FakeSource::new(
            kind,
            self.config.scenario,
            seed,
            self.config.time_scale,
            self.config.log_lines_per_sec,
        )
    }
}

#[async_trait]
impl Discovery for FakeDiscovery {
    fn name(&self) -> &'static str {
        "fake"
    }

    async fn run(mut self: Box<Self>, sink: DiscoverySink) -> DeviceResult<()> {
        let mut kinds: BTreeMap<DeviceId, (FakeDevice, usize)> = BTreeMap::new();
        for (index, kind) in self.config.devices.clone().into_iter().enumerate() {
            let source = self.build(kind, index);
            kinds.insert(source.id(), (kind, index));
            if sink.attached(Box::new(source)).await.is_err() {
                return Ok(());
            }
        }
        // `tx` keeps the channel open even when every handle is dropped, so
        // the loop ends only when the engine drops the sink.
        loop {
            tokio::select! {
                ctrl = self.rx.recv() => match ctrl {
                    Some(Control::Detach(id)) => {
                        if kinds.contains_key(&id) {
                            if sink.detached(id).await.is_err() {
                                return Ok(());
                            }
                        } else {
                            tracing::warn!(device = %id, "detach: unknown fake device");
                        }
                    }
                    Some(Control::Reattach(id)) => match kinds.get(&id) {
                        Some((kind, index)) => {
                            let source = self.build(*kind, *index);
                            if sink.attached(Box::new(source)).await.is_err() {
                                return Ok(());
                            }
                        }
                        None => tracing::warn!(device = %id, "reattach: unknown fake device"),
                    },
                    None => return Ok(()),
                },
                _ = sink.closed() => return Ok(()),
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use statasmon_device::DiscoveryEvent;

    #[tokio::test]
    async fn reports_devices_then_honours_detach_and_reattach() {
        let disc = FakeDiscovery::new(FakeConfig::default());
        let handle = disc.handle();
        let ids = disc.device_ids();
        assert_eq!(ids, vec!["fake:iphone", "fake:ipad", "fake:android"]);
        let (sink, mut rx) = DiscoverySink::channel(8);
        let task = tokio::spawn(Box::new(disc).run(sink));
        for id in &ids {
            match rx.recv().await.unwrap() {
                DiscoveryEvent::Attached(s) => assert_eq!(&s.id(), id),
                other => panic!("unexpected {other:?}"),
            }
        }
        handle.detach("fake:ipad").unwrap();
        assert!(
            matches!(rx.recv().await.unwrap(), DiscoveryEvent::Detached(id) if id == "fake:ipad")
        );
        handle.reattach("fake:ipad").unwrap();
        assert!(
            matches!(rx.recv().await.unwrap(), DiscoveryEvent::Attached(s) if s.id() == "fake:ipad")
        );
        drop(rx);
        task.await.unwrap().unwrap();
        assert_eq!(handle.detach("fake:ipad"), Err(DiscoveryGone));
    }
}
