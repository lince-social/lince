use super::*;
use crate::runtime::latency::{Outputs, Stamp};
use lince_interface::host::performance::{self, Count, InputStage, Stage};
use std::time::Duration;

struct Pending {
    stamp: Stamp,
    minimum_version: u64,
    outputs: Outputs,
}

impl Pending {
    fn forwarded(&mut self, stamp: Stamp, version: u64) {
        self.stamp = if self.outputs.is_empty() {
            Stamp::earliest(Some(self.stamp), Some(stamp)).unwrap()
        } else {
            stamp
        };
        self.minimum_version = version;
        self.outputs = Outputs::default();
    }
}

#[derive(Default)]
pub(super) struct Applications {
    pending: HashMap<WlSurface, Pending>,
}

impl State {
    pub(super) fn input_forwarded(&mut self, surface: &WlSurface) {
        if self.lock.owner.is_some() || self.native_surface.as_ref() == Some(surface) {
            return;
        }
        let Some(stamp) = Stamp::current() else {
            return;
        };
        let Some(root) = self.input_root(surface) else {
            return;
        };
        if self.latency.pending.len() >= 128 && !self.latency.pending.contains_key(&root) {
            performance::count(Count::TimingCapacity);
            return;
        }
        stamp.record(Stage::ClientInputToForward);
        stamp.checkpoint(InputStage::ClientForwarded, 0, 0);
        performance::count(Count::ClientInputForwarded);
        self.latency
            .pending
            .entry(root)
            .and_modify(|pending| pending.forwarded(stamp, self.revision))
            .or_insert(Pending {
                stamp,
                minimum_version: self.revision,
                outputs: Outputs::default(),
            });
    }
}

impl Applications {
    pub(super) fn clear(&mut self) {
        self.pending.clear();
    }
    pub(super) fn retire(&mut self, surface: &WlSurface) {
        self.pending.remove(surface);
    }

    fn presented(
        &mut self,
        root: &WlSurface,
        version: u64,
        output: OutputId,
        time: Duration,
        hardware: bool,
    ) -> bool {
        let Some(pending) = self.pending.get_mut(root) else {
            return false;
        };
        if pending.stamp.age(time).is_none() {
            performance::count(Count::TimingRejected);
            return false;
        }
        if version < pending.minimum_version || !pending.outputs.visit(output) {
            return false;
        }
        if pending.stamp.record_at(
            if hardware {
                Stage::ClientInputToDrmBuffer
            } else {
                Stage::ClientInputToObservedBuffer
            },
            output,
            time,
        ) {
            performance::count(Count::ClientInputProxyPresented);
            pending.stamp.observed(
                if hardware {
                    InputStage::ClientProxyDrm
                } else {
                    InputStage::ClientProxyObserved
                },
                output,
                time,
            );
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::latency::stamp_at;

    #[test]
    fn a_burst_keeps_its_oldest_origin_until_a_newer_buffer_was_observed() {
        let mut pending = Pending {
            stamp: stamp_at(Duration::from_millis(10)),
            minimum_version: 1,
            outputs: Outputs::default(),
        };
        pending.forwarded(stamp_at(Duration::from_millis(30)), 4);
        assert_eq!(
            pending.stamp.age(Duration::from_millis(50)),
            Some(Duration::from_millis(40))
        );
        assert_eq!(pending.minimum_version, 4);
        assert!(pending.outputs.visit(OutputId(1)));
        pending.forwarded(stamp_at(Duration::from_millis(60)), 8);
        assert_eq!(
            pending.stamp.age(Duration::from_millis(80)),
            Some(Duration::from_millis(20))
        );
        assert_eq!(pending.minimum_version, 8);
        assert!(pending.outputs.is_empty());
    }
}

impl Server {
    pub(crate) fn observe_input_presentation(
        &mut self,
        output: OutputId,
        nodes: &[SurfaceNode],
        time: Duration,
        hardware: bool,
    ) -> usize {
        if !performance::enabled() || self.locked() {
            return 0;
        }
        let mut count = 0;
        for node in nodes {
            if let Some(timing) = &node.content.latency {
                timing.presented(output, time, hardware);
            }
            if let Some(root) = self.state.input_root(&node.surface) {
                count += usize::from(self.state.latency.presented(
                    &root,
                    node.content.version,
                    output,
                    time,
                    hardware,
                ));
            }
        }
        count
    }
}
