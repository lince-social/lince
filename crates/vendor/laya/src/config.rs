use crate::modernbert;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct EncoderConfig {
    pub vocab_size: usize,
    pub hidden_size: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    pub intermediate_size: usize,
    pub max_position_embeddings: usize,
    pub layer_norm_eps: f64,
    pub pad_token_id: u32,
    pub global_attn_every_n_layers: usize,
    pub local_attention: usize,
    #[serde(default)]
    pub rope_parameters: Option<RopeParameters>,
    #[serde(default)]
    pub global_rope_theta: Option<f64>,
    #[serde(default)]
    pub local_rope_theta: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct RopeParameters {
    pub full_attention: RopeSpec,
    pub sliding_attention: RopeSpec,
}

#[derive(Debug, Deserialize)]
pub struct RopeSpec {
    pub rope_theta: f64,
}

impl EncoderConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading encoder config {}", path.display()))?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn to_candle(&self) -> modernbert::Config {
        let (global, local) = match &self.rope_parameters {
            Some(r) => (r.full_attention.rope_theta, r.sliding_attention.rope_theta),
            None => (
                self.global_rope_theta.unwrap_or(160_000.0),
                self.local_rope_theta.unwrap_or(10_000.0),
            ),
        };
        modernbert::Config {
            vocab_size: self.vocab_size,
            hidden_size: self.hidden_size,
            num_hidden_layers: self.num_hidden_layers,
            num_attention_heads: self.num_attention_heads,
            intermediate_size: self.intermediate_size,
            max_position_embeddings: self.max_position_embeddings,
            layer_norm_eps: self.layer_norm_eps,
            pad_token_id: self.pad_token_id,
            global_attn_every_n_layers: self.global_attn_every_n_layers,
            global_rope_theta: global,
            local_attention: self.local_attention,
            local_rope_theta: local,
            classifier_config: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    pub max_len: usize,
    pub head_max_len: usize,
    #[serde(default)]
    pub head_layers: usize,
    #[serde(default = "default_temperature")]
    pub temperature: Vec<f32>,
    #[serde(default)]
    pub temperature_by_options: HashMap<String, f32>,
    #[serde(default)]
    pub model_name: String,
}

fn default_temperature() -> Vec<f32> {
    vec![1.0, 1.0, 1.0]
}

impl AgentConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading agent config {}", path.display()))?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn temperature_for(&self, qtype: usize, k: usize) -> f32 {
        let size = if k <= 2 {
            "2"
        } else if k <= 5 {
            "3-5"
        } else if k <= 10 {
            "6-10"
        } else {
            "11+"
        };
        let name = crate::question::QType::from_index(qtype).as_str();
        let key = format!("{name}:{size}");
        self.temperature_by_options
            .get(&key)
            .copied()
            .unwrap_or_else(|| self.temperature.get(qtype).copied().unwrap_or(1.0))
    }
}
