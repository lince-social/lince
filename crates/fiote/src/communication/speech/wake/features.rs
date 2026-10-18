use std::{
    collections::VecDeque,
    io::Cursor,
    sync::{Arc, OnceLock},
};
use tract_onnx::prelude::*;

pub(super) const STEP: usize = 1280;
pub(super) const WIDTH: usize = 96;
const CONTEXT: usize = 480;
const MEL_FRAMES: usize = 76;
const MEL_WIDTH: usize = 32;
const MODEL: &[u8] = include_bytes!("../../../../assets/wake/embedding-model.onnx");
const MEL: &[u8] = include_bytes!("../../../../assets/wake/melspectrogram.onnx");

struct Models {
    mel: Arc<TypedRunnableModel>,
    embedding: Arc<TypedRunnableModel>,
    silence: Vec<f32>,
    center: Vec<f32>,
}

fn models() -> Result<Arc<Models>, String> {
    static MODELS: OnceLock<Result<Arc<Models>, String>> = OnceLock::new();
    MODELS
        .get_or_init(|| {
            let load = |bytes: &[u8], shape: &[usize]| {
                tract_onnx::onnx()
                    .model_for_read(&mut Cursor::new(bytes))?
                    .with_input_fact(0, f32::fact(shape).into())?
                    .into_optimized()?
                    .into_runnable()
            };
            let mel = load(MEL, &[1, STEP + CONTEXT])
                .map_err(|error| format!("Cannot load bundled wake mel model: {error}"))?;
            let embedding = load(MODEL, &[1, MEL_FRAMES, MEL_WIDTH, 1])
                .map_err(|error| format!("Cannot load bundled wake feature model: {error}"))?;
            let input =
                Tensor::zero::<f32>(&[1, STEP + CONTEXT]).map_err(|error| error.to_string())?;
            let output = mel
                .run(tvec!(input.into()))
                .map_err(|error| error.to_string())?;
            let data = output[0]
                .try_as_plain_ram()
                .and_then(|value| value.as_slice::<f32>())
                .map_err(|error| error.to_string())?;
            if data.len() != 8 * MEL_WIDTH {
                return Err("The bundled wake mel shape is invalid.".into());
            }
            let silence = data[..MEL_WIDTH]
                .iter()
                .map(|value| value / 10.0 + 2.0)
                .collect::<Vec<f32>>();
            let quiet: Vec<f32> = silence
                .iter()
                .copied()
                .cycle()
                .take(MEL_FRAMES * MEL_WIDTH)
                .collect();
            let input = Tensor::from_shape(&[1, MEL_FRAMES, MEL_WIDTH, 1], &quiet)
                .map_err(|error| error.to_string())?;
            let output = embedding
                .run(tvec!(input.into()))
                .map_err(|error| error.to_string())?;
            let center = output[0]
                .try_as_plain_ram()
                .and_then(|value| value.as_slice::<f32>())
                .map_err(|error| error.to_string())?
                .to_vec();
            Ok(Arc::new(Models {
                mel,
                embedding,
                silence,
                center,
            }))
        })
        .clone()
}

pub(super) struct Frame {
    pub end: u64,
    pub values: Vec<f32>,
}

pub(super) struct Features {
    models: Arc<Models>,
    pending: VecDeque<f32>,
    previous: Vec<f32>,
    mel: VecDeque<f32>,
    processed: u64,
}

impl Features {
    pub fn reset(&mut self) {
        self.pending.clear();
        self.previous.fill(0.0);
        self.mel = self
            .models
            .silence
            .iter()
            .copied()
            .cycle()
            .take(MEL_FRAMES * MEL_WIDTH)
            .collect();
        self.processed = 0;
    }

    pub fn new() -> Result<Self, String> {
        let models = models()?;
        let mel = models
            .silence
            .iter()
            .copied()
            .cycle()
            .take(MEL_FRAMES * MEL_WIDTH)
            .collect();
        Ok(Self {
            models,
            pending: VecDeque::new(),
            previous: vec![0.0; CONTEXT],
            mel,
            processed: 0,
        })
    }

    pub fn push(&mut self, samples: &[f32]) -> Result<Vec<Frame>, String> {
        self.pending
            .extend(samples.iter().map(|sample| sample * 32768.0));
        let mut frames = Vec::new();
        while self.pending.len() >= STEP {
            let mut input = self.previous.clone();
            input.extend(self.pending.drain(..STEP));
            self.previous
                .copy_from_slice(&input[input.len() - CONTEXT..]);
            self.processed = self
                .processed
                .checked_add(STEP as u64)
                .ok_or("Wake audio position overflowed.")?;
            let tensor = Tensor::from_shape(&[1, STEP + CONTEXT], &input)
                .map_err(|error| error.to_string())?;
            let output = self
                .models
                .mel
                .run(tvec!(tensor.into()))
                .map_err(|error| format!("Wake mel processing failed: {error}"))?;
            let data = output[0]
                .try_as_plain_ram()
                .and_then(|value| value.as_slice::<f32>())
                .map_err(|error| error.to_string())?;
            if data.len() != 8 * MEL_WIDTH {
                return Err("Wake mel output has an invalid shape.".into());
            }
            self.mel.drain(..data.len());
            self.mel.extend(data.iter().map(|value| value / 10.0 + 2.0));
            let input: Vec<_> = self.mel.iter().copied().collect();
            let tensor = Tensor::from_shape(&[1, MEL_FRAMES, MEL_WIDTH, 1], &input)
                .map_err(|error| error.to_string())?;
            let output = self
                .models
                .embedding
                .run(tvec!(tensor.into()))
                .map_err(|error| format!("Wake feature processing failed: {error}"))?;
            let mut values = output[0]
                .try_as_plain_ram()
                .and_then(|value| value.as_slice::<f32>())
                .map_err(|error| error.to_string())?
                .to_vec();
            if values.len() != WIDTH
                || values
                    .iter()
                    .any(|value| !value.is_finite() || value.abs() > 10000.0)
            {
                return Err("Wake model returned invalid acoustic features.".into());
            }
            for (value, center) in values.iter_mut().zip(&self.models.center) {
                *value -= center;
            }
            frames.push(Frame {
                end: self.processed.saturating_sub(128),
                values,
            });
        }
        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_feature_assets_and_stream_shapes_are_stable() {
        assert_eq!(
            crate::communication::models::digest(MODEL),
            "70d164290c1d095d1d4ee149bc5e00543250a7316b59f31d056cff7bd3075c1f"
        );
        assert_eq!(
            crate::communication::models::digest(MEL),
            "ba2b0e0f8b7b875369a2c89cb13360ff53bac436f2895cced9f479fa65eb176f"
        );
        let mut features = Features::new().unwrap();
        assert!(features.push(&vec![0.0; 1000]).unwrap().is_empty());
        let frames = features.push(&vec![0.0; 280]).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].end, 1152);
        assert_eq!(frames[0].values.len(), WIDTH);
    }
}
