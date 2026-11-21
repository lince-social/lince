//! Microphone quality checks and startup buffering before transcription.

use std::{fmt, mem::take};

/// Detects sustained clipping and bias over half-second windows.
#[derive(Default)]
pub struct Quality {
    samples: usize,
    clipped: usize,
    sum: f64,
}

impl fmt::Debug for Quality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Quality")
            .field("samples", &self.samples)
            .field("clipped", &self.clipped)
            .finish_non_exhaustive()
    }
}

impl Quality {
    /// Checks another block at the capture's sample rate.
    ///
    /// # Errors
    /// Returns a diagnostic when a complete window is both biased and clipped.
    pub fn push(&mut self, rate: u32, samples: &[f32]) -> Result<(), String> {
        if self.distorted(rate, samples) {
            return Err("Microphone input is badly distorted. Choose another microphone or check its input level and routing. This capture was refused before transcription; your draft is unchanged.".into());
        }
        Ok(())
    }

    fn distorted(&mut self, rate: u32, samples: &[f32]) -> bool {
        let window = (rate as usize / 2).max(1);
        for &sample in samples {
            self.samples += 1;
            self.clipped += usize::from(sample.abs() >= 0.99);
            self.sum += f64::from(sample);
            if self.samples == window {
                let count = self.samples as f64;
                let distorted =
                    (self.sum / count).abs() > 0.25 && self.clipped as f64 / count > 0.25;
                *self = Self::default();
                if distorted {
                    return true;
                }
            }
        }
        false
    }
}

/// Buffers microphone samples until a clean startup interval is available.
#[derive(Default)]
pub struct Startup {
    rate: Option<u32>,
    window: Vec<f32>,
    clean: Vec<f32>,
}

impl fmt::Debug for Startup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Startup")
            .field("rate", &self.rate)
            .field("window_len", &self.window.len())
            .field("clean_len", &self.clean.len())
            .finish()
    }
}

impl Startup {
    /// Returns the retained clean samples and the rest of this block once ready.
    ///
    /// # Errors
    /// Rejects unsupported or changing sample rates, oversized blocks, and invalid samples.
    pub fn push(&mut self, rate: u32, samples: &[f32]) -> Result<Option<Vec<f32>>, String> {
        const INVALID_AUDIO: &str = "Microphone startup received invalid audio.";
        let sample_rate = usize::try_from(rate).map_err(|_| INVALID_AUDIO)?;
        if !(8_000..=192_000).contains(&rate)
            || self.rate.is_some_and(|previous| previous != rate)
            || samples.len() > sample_rate.saturating_mul(3)
            || samples
                .iter()
                .any(|sample| !sample.is_finite() || sample.abs() > 1.0)
        {
            return Err(INVALID_AUDIO.into());
        }
        self.rate = Some(rate);
        let window = sample_rate / 2;
        let mut remaining = samples;
        while !remaining.is_empty() {
            // A partial window is retained only while its sample rate stays the same.
            let (part, rest) =
                remaining.split_at((window - self.window.len()).min(remaining.len()));
            self.window.extend_from_slice(part);
            remaining = rest;
            if self.window.len() < window {
                break;
            }
            if Quality::default().distorted(rate, &self.window) {
                self.clean.clear();
            } else {
                self.clean.extend_from_slice(&self.window);
            }
            self.window.clear();
            if self.clean.len() >= sample_rate {
                self.clean.extend_from_slice(remaining);
                return Ok(Some(take(&mut self.clean)));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_discards_biased_clipping_then_retains_every_clean_leading_sample() {
        let mut startup = Startup::default();
        assert!(startup.push(16_000, &vec![1.0; 8000]).unwrap().is_none());
        let mut clean = vec![0.02; 16_333];
        clean[7] = 0.4;
        clean[15_999] = -0.3;
        let mut ready = None;
        for part in clean.chunks(97) {
            if let Some(samples) = startup.push(16_000, part).unwrap() {
                ready = Some(samples);
                break;
            }
        }
        let ready = ready.unwrap();
        assert!(ready.len() >= 16_000);
        assert!(clean.starts_with(&ready));
        Quality::default().push(16_000, &ready).unwrap();
        assert!(Quality::default().push(16_000, &vec![-1.0; 8000]).is_err());
    }

    #[test]
    fn startup_needs_consecutive_clean_windows_and_rejects_invalid_formats() {
        let mut startup = Startup::default();
        for _ in 0..20 {
            assert!(startup.push(16_000, &vec![0.01; 8000]).unwrap().is_none());
            assert!(startup.push(16_000, &vec![-1.0; 8000]).unwrap().is_none());
            assert!(startup.clean.is_empty());
            assert!(startup.window.is_empty());
        }
        assert!(startup.push(16_000, &[f32::NAN]).is_err());
        assert!(startup.push(16_000, &[1.1]).is_err());
        assert!(startup.push(8000, &[0.0]).is_err());
        assert!(startup.push(16_000, &vec![0.0; 48_001]).is_err());
    }

    #[test]
    fn quality_keeps_brief_peaks_and_balanced_audio_across_arbitrary_chunks() {
        for samples in [
            [vec![-1.0; 160], vec![0.01; 7840]].concat(),
            [-1.0, 1.0].into_iter().cycle().take(8000).collect(),
            vec![0.1; 8000],
        ] {
            let mut quality = Quality::default();
            for part in samples.chunks(97) {
                quality.push(16_000, part).unwrap();
            }
        }
    }
}
