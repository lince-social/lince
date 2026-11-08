use super::{Distribution, Reading};
use serde::Serialize;
use std::time::Duration;

#[derive(Clone, Copy)]
#[repr(usize)]
pub enum Phase {
    Unspecified,
    Warmup,
    Quiet,
    Typing,
    Scroll,
    Pan,
    OpenReveal,
    GpuFull,
    GpuPixel,
    Styling,
    Resize,
    Description,
    LocalSave,
    Reopen,
    Active,
    ReturnQuiet,
}

impl Phase {
    pub const ALL: &'static [Self] = &[
        Self::Unspecified,
        Self::Warmup,
        Self::Quiet,
        Self::Typing,
        Self::Scroll,
        Self::Pan,
        Self::OpenReveal,
        Self::GpuFull,
        Self::GpuPixel,
        Self::Styling,
        Self::Resize,
        Self::Description,
        Self::LocalSave,
        Self::Reopen,
        Self::Active,
        Self::ReturnQuiet,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Unspecified => "unspecified",
            Self::Warmup => "warmup",
            Self::Quiet => "quiet",
            Self::Typing => "typing",
            Self::Scroll => "scroll",
            Self::Pan => "pan",
            Self::OpenReveal => "open-reveal",
            Self::GpuFull => "gpu-full",
            Self::GpuPixel => "gpu-pixel",
            Self::Styling => "styling",
            Self::Resize => "resize",
            Self::Description => "description",
            Self::LocalSave => "local-save",
            Self::Reopen => "reopen",
            Self::Active => "active",
            Self::ReturnQuiet => "return-quiet",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TimingKind {
    OwnerCpu,
    AwaitElapsed,
    QueueElapsed,
    PresentationInterval,
    InclusiveElapsed,
}

impl TimingKind {
    pub(super) fn for_label(label: &str) -> Self {
        if label.ends_with("-owner-cpu")
            || label == "system-owner-cpu"
            || label == "system-exclusive-owner-cpu"
        {
            Self::OwnerCpu
        } else if label.ends_with("-await") {
            Self::AwaitElapsed
        } else if label.contains("queue-wait")
            || label.contains("lock-wait")
            || label == "gpu-submission-to-completion"
        {
            Self::QueueElapsed
        } else if label == "presentation-interval" {
            Self::PresentationInterval
        } else {
            Self::InclusiveElapsed
        }
    }
}

#[derive(Serialize)]
pub struct SystemReading {
    pub system: String,
    pub wall: Reading,
    pub owner_cpu: Reading,
    pub exclusive_owner_cpu: Reading,
    pub exclusive_scope: &'static str,
}

#[derive(Default)]
pub(super) struct SystemSample {
    wall: Distribution,
    cpu: Distribution,
    exclusive_cpu: Distribution,
}

impl SystemSample {
    pub(super) fn record(&mut self, wall: Duration, cpu: Duration, exclusive_cpu: Duration) {
        self.wall.record(wall);
        self.cpu.record(cpu);
        self.exclusive_cpu.record(exclusive_cpu);
    }

    pub(super) fn take(&mut self, name: &str) -> Option<SystemReading> {
        if self.wall.count == 0 {
            return None;
        }
        Some(SystemReading {
            system: name.to_owned(),
            wall: std::mem::take(&mut self.wall).report("system-wall"),
            owner_cpu: std::mem::take(&mut self.cpu).report("system-owner-cpu"),
            exclusive_owner_cpu: std::mem::take(&mut self.exclusive_cpu)
                .report("system-exclusive-owner-cpu"),
            exclusive_scope: "nested-observed-system-spans-on-the-same-thread",
        })
    }
}

#[derive(Serialize)]
pub struct Coverage {
    pub signal: &'static str,
    pub status: &'static str,
    pub reason: &'static str,
}

pub(super) fn coverage(systems: bool, gpu: bool) -> Vec<Coverage> {
    vec![
        Coverage {
            signal: "bevy-system-bodies",
            status: if systems { "observed" } else { "unobserved" },
            reason: if systems {
                "entered-system-spans-with-owner-thread-cpu"
            } else {
                "no-entered-system-samples-in-this-window"
            },
        },
        Coverage {
            signal: "hardware-gpu-execution",
            status: if gpu {
                "observed-aggregate"
            } else {
                "unobserved"
            },
            reason: "asynchronous-bevy-pass-values; exact-source-frame-not-available; completion-age-is-not-execution",
        },
        Coverage {
            signal: "physical-input-to-visible-pixels",
            status: "unobserved",
            reason: "input-traces-require-an-independent-visible-pixel-oracle",
        },
        Coverage {
            signal: "aggregate-window-boundaries",
            status: "non-atomic-stage-drains",
            reason: "concurrent-producers-can-cross-a-drain-boundary; use-span-and-input-timestamps-for-causality",
        },
        Coverage {
            signal: "critical-path-total",
            status: "not-derived",
            reason: "inclusive-and-concurrent-spans-must-not-be-summed",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intervals_and_awaits_cannot_be_reported_as_cpu() {
        assert_eq!(
            TimingKind::for_label("record-canonical-commit-await"),
            TimingKind::AwaitElapsed
        );
        assert_eq!(
            TimingKind::for_label("gpu-submission-to-completion"),
            TimingKind::QueueElapsed
        );
        assert_eq!(
            TimingKind::for_label("native-phase-update-owner-cpu"),
            TimingKind::OwnerCpu
        );
        assert_eq!(
            TimingKind::for_label("presentation-interval"),
            TimingKind::PresentationInterval
        );
    }

    #[test]
    fn system_counts_survive_windows_and_empty_is_not_an_observation() {
        let mut sample = SystemSample::default();
        assert!(sample.take("bevy_ui::layout").is_none());
        sample.record(
            Duration::from_micros(20),
            Duration::from_micros(12),
            Duration::from_micros(7),
        );
        let reading = sample.take("bevy_ui::layout").unwrap();
        assert_eq!(reading.wall.count, 1);
        assert_eq!(reading.owner_cpu.total_us, 12);
        assert_eq!(reading.exclusive_owner_cpu.total_us, 7);
        assert!(sample.take("bevy_ui::layout").is_none());
        assert_eq!(coverage(false, false)[0].status, "unobserved");
    }
}
