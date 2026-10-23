use candle_core::{D, DType, Device, IndexOp, Result, Tensor};
use candle_nn::{Conv1d, Conv1dConfig, Embedding, LayerNorm, Linear, Module, VarBuilder};
use serde::Deserialize;
use std::{path::Path, sync::atomic::AtomicBool};
use tokenizers::Tokenizer;

pub const CODE_LICENSE: &str =
    include_str!("../../../assets/moonshine/LICENSE-CODE-Apache-2.0.txt");
pub const MODEL_LICENSE: &str = include_str!("../../../assets/moonshine/LICENSE-WEIGHTS-MIT.txt");
pub const NOTICE: &str = include_str!("../../../assets/moonshine/NOTICE");

#[derive(Clone, Deserialize)]
struct AttentionConfig {
    hidden_act: String,
    hidden_size: usize,
    head_dim: usize,
    num_attention_heads: usize,
    num_key_value_heads: usize,
    num_hidden_layers: usize,
    intermediate_size: usize,
    #[serde(default)]
    attention_bias: bool,
}

#[derive(Deserialize)]
struct EncoderConfig {
    #[serde(flatten)]
    attention: AttentionConfig,
    sliding_windows: Vec<[usize; 2]>,
    sample_rate: usize,
    frame_ms: f64,
}

#[derive(Deserialize)]
struct RotaryConfig {
    partial_rotary_factor: f64,
    rope_theta: f64,
    rope_type: String,
}

#[derive(Deserialize)]
struct Config {
    model_type: String,
    #[serde(flatten)]
    decoder: AttentionConfig,
    encoder_config: EncoderConfig,
    rope_parameters: RotaryConfig,
    vocab_size: usize,
    max_position_embeddings: usize,
    decoder_start_token_id: u32,
    eos_token_id: u32,
}

impl Config {
    fn validate(&self) -> std::result::Result<(), String> {
        if self.model_type != "moonshine_streaming"
            || self.encoder_config.sample_rate != 16_000
            || self.encoder_config.frame_ms != 5.0
            || self.rope_parameters.rope_type != "default"
            || !(0.0..=1.0).contains(&self.rope_parameters.partial_rotary_factor)
            || !self.rope_parameters.rope_theta.is_finite()
            || self.rope_parameters.rope_theta <= 0.0
            || self.vocab_size > 65_536
            || self.vocab_size < 3
            || self.max_position_embeddings > 8192
            || self.max_position_embeddings < 2
            || self.decoder_start_token_id as usize >= self.vocab_size
            || self.eos_token_id as usize >= self.vocab_size
        {
            return Err("Unsupported Moonshine Streaming configuration.".into());
        }
        for config in [&self.decoder, &self.encoder_config.attention] {
            if config.hidden_size == 0
                || config.hidden_size > 2048
                || config.head_dim == 0
                || config.head_dim > 256
                || config.num_attention_heads == 0
                || config.num_attention_heads > 32
                || config.num_key_value_heads != config.num_attention_heads
                || config.num_hidden_layers == 0
                || config.num_hidden_layers > 32
                || config.intermediate_size == 0
                || config.intermediate_size > 8192
                || config.attention_bias
            {
                return Err("Unsupported Moonshine attention dimensions.".into());
            }
        }
        if self.decoder.hidden_act != "silu" || self.encoder_config.attention.hidden_act != "gelu" {
            return Err("Unsupported Moonshine activation functions.".into());
        }
        let rotary =
            (self.decoder.head_dim as f64 * self.rope_parameters.partial_rotary_factor) as usize;
        if rotary == 0
            || rotary % 2 != 0
            || self.encoder_config.sliding_windows.len()
                != self.encoder_config.attention.num_hidden_layers
            || self
                .encoder_config
                .sliding_windows
                .iter()
                .any(|window| window[0] == 0 || window[0] > 4096 || window[1] > 4096)
        {
            return Err("Invalid Moonshine rotary dimensions or attention windows.".into());
        }
        Ok(())
    }
}

struct OffsetNorm {
    norm: LayerNorm,
    gamma: Tensor,
}

impl OffsetNorm {
    fn load(size: usize, vb: VarBuilder<'_>) -> Result<Self> {
        Ok(Self {
            norm: LayerNorm::new_no_bias(Tensor::ones(size, DType::F32, vb.device())?, 1e-5),
            gamma: (vb.get(size, "gamma")? + 1.0)?,
        })
    }

    fn forward(&self, input: &Tensor) -> Result<Tensor> {
        self.norm.forward(input)?.broadcast_mul(&self.gamma)
    }
}

struct Attention {
    query: Linear,
    key: Linear,
    value: Linear,
    output: Linear,
    heads: usize,
    head_dim: usize,
}

impl Attention {
    fn load(config: &AttentionConfig, vb: VarBuilder<'_>) -> Result<Self> {
        let width = config.num_attention_heads * config.head_dim;
        let projection = |name| {
            candle_nn::linear_b(
                config.hidden_size,
                width,
                config.attention_bias,
                vb.pp(name),
            )
        };
        Ok(Self {
            query: projection("q_proj")?,
            key: projection("k_proj")?,
            value: projection("v_proj")?,
            output: candle_nn::linear_no_bias(width, config.hidden_size, vb.pp("o_proj"))?,
            heads: config.num_attention_heads,
            head_dim: config.head_dim,
        })
    }

    fn split(&self, input: Tensor) -> Result<Tensor> {
        let (batch, length, _) = input.dims3()?;
        input
            .reshape((batch, length, self.heads, self.head_dim))?
            .transpose(1, 2)?
            .contiguous()
    }

    fn key_value(&self, input: &Tensor) -> Result<(Tensor, Tensor)> {
        Ok((
            self.split(self.key.forward(input)?)?,
            self.split(self.value.forward(input)?)?,
        ))
    }

    fn attend(
        &self,
        query: &Tensor,
        key: &Tensor,
        value: &Tensor,
        mask: Option<&Tensor>,
    ) -> Result<Tensor> {
        let mut scores = (query.matmul(&key.t()?.contiguous()?)? / (self.head_dim as f64).sqrt())?;
        if let Some(mask) = mask {
            scores = scores.broadcast_add(mask)?;
        }
        let output = candle_nn::ops::softmax_last_dim(&scores)?
            .matmul(value)?
            .transpose(1, 2)?
            .contiguous()?;
        let (batch, length, _, _) = output.dims4()?;
        self.output
            .forward(&output.reshape((batch, length, self.heads * self.head_dim))?)
    }

    fn forward(&self, input: &Tensor, mask: &Tensor) -> Result<Tensor> {
        let query = self.split(self.query.forward(input)?)?;
        let (key, value) = self.key_value(input)?;
        self.attend(&query, &key, &value, Some(mask))
    }
}

struct EncoderLayer {
    attention: Attention,
    input_norm: OffsetNorm,
    output_norm: OffsetNorm,
    up: Linear,
    down: Linear,
    window: [usize; 2],
}

impl EncoderLayer {
    fn load(config: &AttentionConfig, window: [usize; 2], vb: VarBuilder<'_>) -> Result<Self> {
        Ok(Self {
            attention: Attention::load(config, vb.pp("self_attn"))?,
            input_norm: OffsetNorm::load(config.hidden_size, vb.pp("input_layernorm"))?,
            output_norm: OffsetNorm::load(config.hidden_size, vb.pp("post_attention_layernorm"))?,
            up: candle_nn::linear(
                config.hidden_size,
                config.intermediate_size,
                vb.pp("mlp.fc1"),
            )?,
            down: candle_nn::linear(
                config.intermediate_size,
                config.hidden_size,
                vb.pp("mlp.fc2"),
            )?,
            window,
        })
    }

    fn forward(&self, input: &Tensor) -> Result<Tensor> {
        let length = input.dim(1)?;
        let mask = window_mask(length, self.window, input.device())?;
        let hidden = (input
            + self
                .attention
                .forward(&self.input_norm.forward(input)?, &mask)?)?;
        let feedforward = self
            .up
            .forward(&self.output_norm.forward(&hidden)?)?
            .gelu_erf()?;
        hidden + self.down.forward(&feedforward)?
    }
}

fn window_mask(length: usize, window: [usize; 2], device: &Device) -> Result<Tensor> {
    let values: Vec<f32> = (0..length)
        .flat_map(|query| {
            (0..length).map(move |key| {
                if (key <= query && query - key < window[0])
                    || (key > query && key - query < window[1])
                {
                    0.0
                } else {
                    f32::NEG_INFINITY
                }
            })
        })
        .collect();
    Tensor::from_vec(values, (1, 1, length, length), device)
}

struct DecoderLayer {
    attention: Attention,
    cross: Attention,
    input_norm: LayerNorm,
    cross_norm: LayerNorm,
    output_norm: LayerNorm,
    up: Linear,
    down: Linear,
}

#[derive(Default)]
struct LayerCache {
    attention: Option<(Tensor, Tensor)>,
    cross: Option<(Tensor, Tensor)>,
}

impl DecoderLayer {
    fn load(config: &AttentionConfig, vb: VarBuilder<'_>) -> Result<Self> {
        Ok(Self {
            attention: Attention::load(config, vb.pp("self_attn"))?,
            cross: Attention::load(config, vb.pp("encoder_attn"))?,
            input_norm: candle_nn::layer_norm_no_bias(
                config.hidden_size,
                1e-5,
                vb.pp("input_layernorm"),
            )?,
            cross_norm: candle_nn::layer_norm_no_bias(
                config.hidden_size,
                1e-5,
                vb.pp("post_attention_layernorm"),
            )?,
            output_norm: candle_nn::layer_norm_no_bias(
                config.hidden_size,
                1e-5,
                vb.pp("final_layernorm"),
            )?,
            up: candle_nn::linear(
                config.hidden_size,
                config.intermediate_size * 2,
                vb.pp("mlp.fc1"),
            )?,
            down: candle_nn::linear(
                config.intermediate_size,
                config.hidden_size,
                vb.pp("mlp.fc2"),
            )?,
        })
    }

    fn forward(
        &self,
        input: &Tensor,
        encoded: &Tensor,
        rotary: &(Tensor, Tensor),
        cache: &mut LayerCache,
    ) -> Result<Tensor> {
        let normalized = self.input_norm.forward(input)?;
        let query = rotate(
            &self
                .attention
                .split(self.attention.query.forward(&normalized)?)?,
            rotary,
        )?;
        let (key, value) = self.attention.key_value(&normalized)?;
        let key = rotate(&key, rotary)?;
        let (key, value) = match &cache.attention {
            Some((previous_key, previous_value)) => (
                Tensor::cat(&[previous_key, &key], 2)?,
                Tensor::cat(&[previous_value, &value], 2)?,
            ),
            None => (key, value),
        };
        let hidden = (input + self.attention.attend(&query, &key, &value, None)?)?;
        cache.attention = Some((key, value));
        if cache.cross.is_none() {
            cache.cross = Some(self.cross.key_value(encoded)?);
        }
        let (key, value) = cache
            .cross
            .as_ref()
            .ok_or_else(|| candle_core::Error::Msg("Missing cross-attention cache.".into()))?;
        let query = self.cross.split(
            self.cross
                .query
                .forward(&self.cross_norm.forward(&hidden)?)?,
        )?;
        let hidden = (hidden + self.cross.attend(&query, key, value, None)?)?;
        let feedforward = self.up.forward(&self.output_norm.forward(&hidden)?)?;
        let halves = feedforward.chunk(2, D::Minus1)?;
        let feedforward = (candle_nn::ops::silu(&halves[1])? * &halves[0])?;
        hidden + self.down.forward(&feedforward)?
    }
}

fn rotate(input: &Tensor, rotary: &(Tensor, Tensor)) -> Result<Tensor> {
    let dim = rotary.0.dim(D::Minus1)?;
    let rotated = input.narrow(D::Minus1, 0, dim)?;
    let (batch, heads, length, _) = rotated.dims4()?;
    let pairs = rotated.reshape((batch, heads, length, dim / 2, 2))?;
    let even = pairs.narrow(4, 0, 1)?;
    let odd = pairs.narrow(4, 1, 1)?.neg()?;
    let swapped = Tensor::cat(&[&odd, &even], 4)?.reshape((batch, heads, length, dim))?;
    let rotated = (rotated.broadcast_mul(&rotary.0)? + swapped.broadcast_mul(&rotary.1)?)?;
    Tensor::cat(
        &[
            rotated,
            input.narrow(D::Minus1, dim, input.dim(D::Minus1)? - dim)?,
        ],
        D::Minus1,
    )
}

pub struct Recognizer {
    config: Config,
    frontend: Linear,
    compression: f32,
    conv1: Conv1d,
    conv2: Conv1d,
    encoder: Vec<EncoderLayer>,
    encoder_norm: OffsetNorm,
    positions: Embedding,
    projection: Option<Linear>,
    tokens: Embedding,
    decoder: Vec<DecoderLayer>,
    decoder_norm: LayerNorm,
    output: Linear,
    tokenizer: Tokenizer,
    device: Device,
}

impl Recognizer {
    pub fn load(directory: &Path) -> std::result::Result<Self, String> {
        Self::load_inner(directory).map_err(|error| error.to_string())
    }

    fn load_inner(directory: &Path) -> Result<Self> {
        let config: Config = serde_json::from_slice(&std::fs::read(directory.join("config.json"))?)
            .map_err(candle_core::Error::wrap)?;
        config.validate().map_err(candle_core::Error::Msg)?;
        let device = Device::Cpu;
        let weights = candle_core::safetensors::load(directory.join("model.safetensors"), &device)?;
        let vb = VarBuilder::from_tensors(weights, DType::F32, &device);
        let encoder = vb.pp("model.encoder");
        let decoder = vb.pp("model.decoder");
        let embedder = encoder.pp("embedder");
        let encoder_config = &config.encoder_config.attention;
        let hidden = encoder_config.hidden_size;
        let conv_config = Conv1dConfig {
            stride: 2,
            ..Default::default()
        };
        Ok(Self {
            frontend: candle_nn::linear_no_bias(80, hidden, embedder.pp("linear"))?,
            compression: embedder.get((), "comp.log_k")?.to_scalar::<f32>()?.exp(),
            conv1: candle_nn::conv1d(hidden, hidden * 2, 5, conv_config, embedder.pp("conv1"))?,
            conv2: candle_nn::conv1d(hidden * 2, hidden, 5, conv_config, embedder.pp("conv2"))?,
            encoder: config
                .encoder_config
                .sliding_windows
                .iter()
                .enumerate()
                .map(|(index, window)| {
                    EncoderLayer::load(
                        encoder_config,
                        *window,
                        encoder.pp(format!("layers.{index}")),
                    )
                })
                .collect::<Result<_>>()?,
            encoder_norm: OffsetNorm::load(hidden, encoder.pp("final_norm"))?,
            positions: candle_nn::embedding(
                config.max_position_embeddings,
                hidden,
                decoder.pp("pos_emb"),
            )?,
            projection: if hidden == config.decoder.hidden_size {
                None
            } else {
                Some(candle_nn::linear_no_bias(
                    hidden,
                    config.decoder.hidden_size,
                    decoder.pp("proj"),
                )?)
            },
            tokens: candle_nn::embedding(
                config.vocab_size,
                config.decoder.hidden_size,
                decoder.pp("embed_tokens"),
            )?,
            decoder: (0..config.decoder.num_hidden_layers)
                .map(|index| {
                    DecoderLayer::load(&config.decoder, decoder.pp(format!("layers.{index}")))
                })
                .collect::<Result<_>>()?,
            decoder_norm: candle_nn::layer_norm_no_bias(
                config.decoder.hidden_size,
                1e-5,
                decoder.pp("norm"),
            )?,
            output: candle_nn::linear_no_bias(
                config.decoder.hidden_size,
                config.vocab_size,
                vb.pp("proj_out"),
            )?,
            tokenizer: Tokenizer::from_file(directory.join("tokenizer.json"))
                .map_err(|error| candle_core::Error::Msg(error.to_string()))?,
            config,
            device,
        })
    }

    fn encode(&self, samples: &[f32], cancelled: &AtomicBool) -> Result<Tensor> {
        let mut framed = samples.to_vec();
        framed.resize(samples.len().div_ceil(80) * 80, 0.0);
        for frame in framed.chunks_exact_mut(80) {
            let mean = frame.iter().sum::<f32>() / 80.0;
            let rms = (frame
                .iter()
                .map(|sample| (sample - mean).powi(2))
                .sum::<f32>()
                / 80.0
                + 1e-6)
                .sqrt();
            for sample in frame {
                *sample = (self.compression * (*sample - mean) / rms).asinh();
            }
        }
        let input = Tensor::from_vec(framed, (1, samples.len().div_ceil(80), 80), &self.device)?;
        let hidden = candle_nn::ops::silu(&self.frontend.forward(&input)?)?
            .transpose(1, 2)?
            .contiguous()?;
        let hidden = candle_nn::ops::silu(&self.conv1.forward(&hidden.pad_with_zeros(2, 4, 0)?)?)?;
        let mut hidden = self
            .conv2
            .forward(&hidden.pad_with_zeros(2, 4, 0)?)?
            .transpose(1, 2)?
            .contiguous()?;
        for layer in &self.encoder {
            super::super::models::check_cancel(cancelled).map_err(candle_core::Error::Msg)?;
            hidden = layer.forward(&hidden)?;
        }
        let hidden = self.encoder_norm.forward(&hidden)?;
        let length = hidden.dim(1)?;
        let positions = Tensor::arange(0u32, length as u32, &self.device)?;
        let hidden = hidden.broadcast_add(&self.positions.forward(&positions)?)?;
        match &self.projection {
            Some(projection) => projection.forward(&hidden),
            None => Ok(hidden),
        }
    }

    fn rotary(&self, position: usize) -> Result<(Tensor, Tensor)> {
        let dimensions = (self.config.decoder.head_dim as f64
            * self.config.rope_parameters.partial_rotary_factor) as usize;
        let mut cos = Vec::with_capacity(dimensions);
        let mut sin = Vec::with_capacity(dimensions);
        for index in (0..dimensions).step_by(2) {
            let frequency = position as f64
                / self
                    .config
                    .rope_parameters
                    .rope_theta
                    .powf(index as f64 / dimensions as f64);
            cos.extend([frequency.cos() as f32; 2]);
            sin.extend([frequency.sin() as f32; 2]);
        }
        Ok((
            Tensor::from_vec(cos, (1, 1, 1, dimensions), &self.device)?,
            Tensor::from_vec(sin, (1, 1, 1, dimensions), &self.device)?,
        ))
    }

    pub fn transcribe(
        &self,
        samples: &[f32],
        cancelled: &AtomicBool,
    ) -> std::result::Result<String, String> {
        if samples.is_empty()
            || samples.len() > 480_000
            || samples.iter().any(|sample| !sample.is_finite())
        {
            return Err("Moonshine evaluation accepts finite 16 kHz mono audio up to 30 seconds per segment.".into());
        }
        super::super::models::check_cancel(cancelled)?;
        self.transcribe_inner(samples, cancelled)
            .map_err(|error| error.to_string())
    }

    fn transcribe_inner(&self, samples: &[f32], cancelled: &AtomicBool) -> Result<String> {
        let encoded = self.encode(samples, cancelled)?;
        let mut caches: Vec<_> = self.decoder.iter().map(|_| LayerCache::default()).collect();
        let mut output = Vec::new();
        let mut token = self.config.decoder_start_token_id;
        let limit = ((samples.len() as f64 / 16_000.0 * 6.5).ceil() as usize + 16)
            .min(self.config.max_position_embeddings);
        for position in 0..limit {
            super::super::models::check_cancel(cancelled).map_err(candle_core::Error::Msg)?;
            let input = Tensor::new(&[[token]], &self.device)?;
            let mut hidden = self.tokens.forward(&input)?;
            let rotary = self.rotary(position)?;
            for (layer, cache) in self.decoder.iter().zip(&mut caches) {
                hidden = layer.forward(&hidden, &encoded, &rotary, cache)?;
            }
            let logits = self.output.forward(&self.decoder_norm.forward(&hidden)?)?;
            token = logits.i((0, 0))?.argmax(0)?.to_scalar::<u32>()?;
            if token == self.config.eos_token_id {
                return self
                    .tokenizer
                    .decode(&output, true)
                    .map_err(|error| candle_core::Error::Msg(error.to_string()));
            }
            output.push(token);
        }
        candle_core::bail!("Moonshine reached its decoding limit without an end token.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sliding_window_keeps_only_the_reference_context() -> Result<()> {
        let mask = window_mask(5, [2, 2], &Device::Cpu)?
            .flatten_all()?
            .to_vec1::<f32>()?;
        assert_eq!(
            &mask[10..15],
            &[f32::NEG_INFINITY, 0.0, 0.0, 0.0, f32::NEG_INFINITY]
        );
        assert_eq!(
            &mask[..5],
            &[
                0.0,
                0.0,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY
            ]
        );
        Ok(())
    }

    #[test]
    fn partial_rotary_rotates_pairs_and_preserves_the_tail() -> Result<()> {
        let device = Device::Cpu;
        let input = Tensor::from_vec(vec![1f32, 2., 3., 4., 5., 6.], (1, 1, 1, 6), &device)?;
        let rotary = (
            Tensor::zeros((1, 1, 1, 4), DType::F32, &device)?,
            Tensor::ones((1, 1, 1, 4), DType::F32, &device)?,
        );
        assert_eq!(
            rotate(&input, &rotary)?.flatten_all()?.to_vec1::<f32>()?,
            vec![-2., 1., -4., 3., 5., 6.]
        );
        Ok(())
    }
}
