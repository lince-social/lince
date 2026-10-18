#[derive(Default)]
pub struct Quality {
    samples: usize,
    clipped: usize,
    sum: f64,
}

impl Quality {
    pub fn push(&mut self, rate: u32, samples: &[f32]) -> Result<(), String> {
        let window = (rate as usize / 2).max(1);
        for sample in samples {
            self.samples += 1;
            self.clipped += usize::from(sample.abs() >= 0.99);
            self.sum += f64::from(*sample);
            if self.samples == window {
                let biased = (self.sum / self.samples as f64).abs() > 0.25;
                let clipped = self.clipped as f64 / self.samples as f64 > 0.25;
                *self = Self::default();
                if biased && clipped {
                    return Err("Microphone input is badly distorted. Choose another microphone or check its input level and routing. This capture was refused before transcription; your draft is unchanged.".into());
                }
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct Startup {
    rate: Option<u32>,
    window: Vec<f32>,
    clean: Vec<f32>,
}

impl Startup {
    pub fn push(&mut self, rate: u32, samples: &[f32]) -> Result<Option<Vec<f32>>, String> {
        if !(8000..=192_000).contains(&rate)
            || self.rate.is_some_and(|previous| previous != rate)
            || samples.len() > rate as usize * 3
            || samples
                .iter()
                .any(|sample| !sample.is_finite() || sample.abs() > 1.0)
        {
            return Err("Microphone startup received invalid audio.".into());
        }
        self.rate = Some(rate);
        let window = rate as usize / 2;
        for (index, sample) in samples.iter().enumerate() {
            self.window.push(*sample);
            if self.window.len() != window {
                continue;
            }
            let frame = std::mem::take(&mut self.window);
            if Quality::default().push(rate, &frame).is_err() {
                self.clean.clear();
                continue;
            }
            self.clean.extend(frame);
            if self.clean.len() >= rate as usize {
                self.clean.extend_from_slice(&samples[index + 1..]);
                return Ok(Some(std::mem::take(&mut self.clean)));
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
        assert_eq!(ready, clean[..ready.len()]);
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
