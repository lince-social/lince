use super::ShaderInputs;
use bevy::prelude::*;

#[derive(Clone)]
struct Frame {
    program: String,
    inputs: ShaderInputs,
    ready: bool,
}

#[derive(Default)]
pub(super) struct Snapshot {
    last: Option<Frame>,
    frozen: Option<Frame>,
}

impl Snapshot {
    pub(super) fn synchronize(&mut self, freeze: bool, painted: u8) {
        if let Some(frame) = &mut self.last {
            frame.ready |= painted >= 3;
        }
        if !freeze {
            self.frozen = None;
        } else if self.frozen.is_none() && self.last.as_ref().is_some_and(|frame| frame.ready) {
            self.frozen = self.last.clone();
        }
    }

    pub(super) fn frozen(&self) -> bool {
        self.frozen.is_some()
    }

    pub(super) fn program(&self) -> Option<&str> {
        self.frozen.as_ref().map(|frame| frame.program.as_str())
    }

    pub(super) fn inputs(&self, size: Vec2) -> Option<ShaderInputs> {
        let frame = self.frozen.as_ref()?;
        let mut inputs = frame.inputs;
        inputs.mouse.x *= size.x / inputs.size.x.max(1.0);
        inputs.mouse.y *= size.y / inputs.size.y.max(1.0);
        inputs.size = size;
        Some(inputs)
    }

    pub(super) fn painted(&mut self, program: impl Into<String>, inputs: ShaderInputs) {
        self.last = Some(Frame {
            program: program.into(),
            inputs,
            ready: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repaint_progress_does_not_replace_a_frozen_frame_or_its_normalized_pointer() {
        let mut snapshot = Snapshot::default();
        let original = ShaderInputs {
            size: Vec2::new(320.0, 180.0),
            mouse: Vec4::new(80.0, 135.0, 1.0, 1.0),
            time: 12.0,
            delta: 0.016,
            quantity: -2.0,
            quantity_present: 1,
            numeric0: Vec4::new(9.0, 3.0, 2.0, 1.0),
            present0: UVec4::ONE,
            ..default()
        };
        snapshot.painted("negative", original);
        snapshot.synchronize(true, 2);
        assert!(!snapshot.frozen());
        snapshot.synchronize(true, 3);
        assert_eq!(snapshot.program(), Some("negative"));
        let mut resized = original;
        resized.size = Vec2::new(960.0, 720.0);
        resized.mouse = Vec4::new(240.0, 540.0, 1.0, 1.0);
        assert_eq!(snapshot.inputs(resized.size), Some(resized));
        snapshot.painted("positive", ShaderInputs::default());
        for progress in 0..4 {
            snapshot.synchronize(true, progress);
            assert_eq!(snapshot.program(), Some("negative"));
            assert_eq!(snapshot.inputs(resized.size), Some(resized));
        }
        snapshot.synchronize(false, 3);
        assert!(!snapshot.frozen());
        assert_eq!(snapshot.program(), None);
        assert_eq!(snapshot.inputs(resized.size), None);
        snapshot.synchronize(true, 3);
        assert_eq!(snapshot.program(), Some("positive"));
    }

    #[test]
    fn freezing_before_the_first_successful_paint_waits_for_a_real_frame() {
        let mut snapshot = Snapshot::default();
        snapshot.synchronize(true, u8::MAX);
        assert!(!snapshot.frozen());
        snapshot.painted("ready", ShaderInputs::default());
        snapshot.synchronize(true, 3);
        assert_eq!(snapshot.program(), Some("ready"));
    }

    #[test]
    fn freezing_and_resizing_together_capture_the_last_ready_frame_before_repainting() {
        let mut snapshot = Snapshot::default();
        let inputs = ShaderInputs {
            size: Vec2::splat(256.0),
            quantity: 3.0,
            time: 7.0,
            ..default()
        };
        snapshot.painted("previous", inputs);
        snapshot.synchronize(false, 3);
        assert!(!snapshot.frozen());
        snapshot.synchronize(true, 0);
        assert_eq!(snapshot.program(), Some("previous"));
        assert_eq!(snapshot.inputs(inputs.size), Some(inputs));
    }
}
