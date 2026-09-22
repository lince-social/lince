use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Distortion {
    #[default]
    Overdrive,
    Fuzz,
    BitCrush,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Effects {
    pub enabled: bool,
    pub distortion: Distortion,
    pub drive: f32,
    pub tone: f32,
    pub crush: f32,
    pub mix: f32,
    pub level: f32,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            enabled: false,
            distortion: Distortion::Overdrive,
            drive: 0.35,
            tone: 0.65,
            crush: 0.4,
            mix: 0.75,
            level: 0.8,
        }
    }
}

impl Effects {
    pub fn valid(&self) -> bool {
        [self.drive, self.tone, self.crush, self.mix, self.level]
            .iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
    }

    pub fn process(&self, samples: &[f32], sample_rate: u32) -> Vec<f32> {
        let settings = if self.valid() { *self } else { Self::default() };
        let cutoff = 200.0 * 80.0_f32.powf(settings.tone);
        let alpha = 1.0 - (-std::f32::consts::TAU * cutoff / sample_rate.max(1) as f32).exp();
        let gain = 1.0 + 49.0 * settings.drive;
        let steps = 2.0_f32.powf((16.0 - settings.crush * 14.0).round());
        let mut low = 0.0;
        samples
            .iter()
            .map(|sample| {
                let dry = if sample.is_finite() {
                    sample.clamp(-1.0, 1.0)
                } else {
                    0.0
                };
                if !settings.enabled {
                    return dry;
                }
                let wet = match settings.distortion {
                    Distortion::Overdrive => (dry * gain).tanh() / gain.tanh(),
                    Distortion::Fuzz => (dry * gain * 2.0).clamp(-1.0, 1.0),
                    Distortion::BitCrush => ((dry * gain).clamp(-1.0, 1.0) * steps).round() / steps,
                };
                low += alpha * (wet - low);
                ((dry * (1.0 - settings.mix) + low * settings.mix) * settings.level)
                    .clamp(-1.0, 1.0)
            })
            .collect()
    }
}
