use super::*;
use crate::runtime::latency::{Outputs, Stamp};
use lince_interface::host::performance::{self, Count, Stage};
use std::time::Duration;

struct Pending {
    stamp: Stamp,
    minimum_version: u64,
    outputs: Outputs,
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
        performance::count(Count::ClientInputForwarded);
        self.latency.pending.insert(
            root,
            Pending {
                stamp,
                minimum_version: self.revision,
                outputs: Outputs::default(),
            },
        );
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
            return true;
        }
        false
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
