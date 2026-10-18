pub mod config;
pub mod bert;
pub mod model;
pub mod modernbert;
pub mod question;

use anyhow::{Context, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::Path;
use tokenizers::Tokenizer;

pub use config::{AgentConfig, EncoderConfig};
pub use question::{QType, Question};

use question::SpecialIds;

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub device: Option<Device>,
    pub dtype: Option<DType>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Answer {
    Choice {
        choice: String,
        probabilities: Map<String, Value>,
        confidence: Value,
        rl_agent: Meta,
    },
    Score {
        score: Value,
        legend: Map<String, Value>,
        probabilities: Map<String, Value>,
        confidence: Value,
        rl_agent: Meta,
    },
    Noul {
        noul: Value,
        rl_agent: Meta,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct Meta {
    pub act_probability: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub model: String,
    pub answers: Map<String, Value>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize)]
pub struct Usage {
    pub input_tokens: usize,
    pub output_tokens: usize,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Segment {
    Special,
    Head,
    Option,
    State,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenView {
    pub text: String,
    pub id: u32,
    pub segment: Segment,
    pub marker: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptView {
    pub tokens: Vec<TokenView>,
    pub options: Vec<String>,
    pub markers: Vec<usize>,
    pub total_tokens: usize,
    pub max_len: usize,
    pub state_truncated: bool,
}

pub struct Agent {
    model: model::DecisionModel,
    tokenizer: Tokenizer,
    specials: SpecialIds,
    pub cfg: AgentConfig,
}

impl Agent {
    pub fn validate_question_budget(&self, state: &Value, question: &Question) -> Result<()> {
        let mut option_tokens = 0usize;
        for option in question.render_options()? {
            let option = format!(" {}", option.replace("[MASK]", " "));
            let encoded = self
                .tokenizer
                .encode(option, false)
                .map_err(|error| anyhow::anyhow!("tokenizing option: {error}"))?;
            anyhow::ensure!(
                encoded.len() <= question::MAX_OPTION_TOKENS,
                "decision option exceeds the model token budget"
            );
            option_tokens += encoded.len() + 1;
        }
        anyhow::ensure!(
            option_tokens + 16 <= self.cfg.head_max_len,
            "decision options exceed the model prefix budget"
        );
        let head = format!(
            "{} question: {}",
            question.qtype.as_str(),
            question.instructions_text().replace("[MASK]", " ")
        );
        let encoded = self
            .tokenizer
            .encode(head, false)
            .map_err(|error| anyhow::anyhow!("tokenizing question: {error}"))?;
        anyhow::ensure!(
            encoded.len() <= self.cfg.head_max_len - option_tokens,
            "decision question exceeds the model token budget"
        );
        let prompt = self.render_prompt(state, question)?;
        anyhow::ensure!(
            !prompt.state_truncated,
            "decision state exceeds the model token budget"
        );
        Ok(())
    }

    pub fn from_dir(dir: impl AsRef<Path>, opts: Options) -> Result<Self> {
        let dir = dir.as_ref();
        let device = match opts.device {
            Some(d) => d,
            None => default_device(),
        };
        let dtype = opts.dtype.unwrap_or(DType::F32);
        if dtype == DType::F16 && matches!(device, Device::Cpu) {
            anyhow::bail!("f16 needs an accelerator: candle's CPU kernels have no f16 path");
        }

        let cfg = AgentConfig::load(dir.join("rl_agent_config.json"))?;
        let enc_cfg = EncoderConfig::load(dir.join("encoder/config.json"))?;

        let tok_path = dir.join("tokenizer/tokenizer.json");
        let tokenizer = Tokenizer::from_file(&tok_path)
            .map_err(|e| anyhow::anyhow!("loading tokenizer {}: {e}", tok_path.display()))?;
        let specials = resolve_specials(&tokenizer)?;

        let weights = dir.join("model.safetensors");
        let tensors = candle_core::safetensors::load(&weights, &device)
            .with_context(|| format!("loading weights {}", weights.display()))?;
        let tensors: HashMap<String, Tensor> = tensors
            .into_iter()
            .map(|(k, v)| match k.strip_prefix("encoder.") {
                Some(rest) => (format!("model.{rest}"), v),
                None => (k, v),
            })
            .collect();
        let vb = VarBuilder::from_tensors(tensors, dtype, &device);

        let head_layers = if cfg.head_layers == 0 {
            2
        } else {
            cfg.head_layers
        };
        let model = model::DecisionModel::load(vb, &enc_cfg, head_layers, 2, device, dtype)?;

        Ok(Self {
            model,
            tokenizer,
            specials,
            cfg,
        })
    }

    pub fn system_one(
        &self,
        state: &Value,
        questions: &Vec<(String, Question)>,
    ) -> Result<Response> {
        if questions.is_empty() {
            return Ok(Response {
                model: self.model_name(),
                answers: Map::new(),
                usage: Usage {
                    input_tokens: 0,
                    output_tokens: 0,
                },
            });
        }

        let mut encoded = Vec::with_capacity(questions.len());
        for (qid, q) in questions {
            let e = question::build_sequence(
                &self.tokenizer,
                &self.specials,
                state,
                q,
                self.cfg.max_len,
                self.cfg.head_max_len,
            )
            .with_context(|| format!("question {qid:?}"))?;
            encoded.push(e);
        }

        let n = encoded.len();
        let l = encoded.iter().map(|e| e.ids.len()).max().unwrap();
        let kmax = encoded.iter().map(|e| e.markers.len()).max().unwrap();

        let mut ids = vec![self.specials.pad; n * l];
        let mut att = vec![0u32; n * l];
        let mut mpos = vec![0u32; n * kmax];
        let mut mmask = vec![vec![false; kmax]; n];
        let mut qtypes = Vec::with_capacity(n);
        let mut input_tokens = 0usize;

        for (i, (e, (_, q))) in encoded.iter().zip(questions).enumerate() {
            ids[i * l..i * l + e.ids.len()].copy_from_slice(&e.ids);
            for j in 0..e.ids.len() {
                att[i * l + j] = 1;
            }
            input_tokens += e.ids.len();
            for (j, m) in e.markers.iter().enumerate() {
                mpos[i * kmax + j] = *m as u32;
                mmask[i][j] = true;
            }
            qtypes.push(q.qtype.index() as u32);
        }

        let dev = &self.model.device;
        let input_ids = Tensor::from_vec(ids, (n, l), dev)?;
        let attention_mask = Tensor::from_vec(att, (n, l), dev)?;
        let marker_pos = Tensor::from_vec(mpos, (n, kmax), dev)?;
        let qtype = Tensor::from_vec(qtypes, n, dev)?;

        let out = self
            .model
            .forward(&input_ids, &attention_mask, &marker_pos, &mmask, &qtype)?;

        let mut answers = Map::new();
        for (r, ((qid, q), e)) in questions.iter().zip(&encoded).enumerate() {
            let k = e.markers.len();
            let t = self.cfg.temperature_for(q.qtype.index(), k);
            let z: Vec<f32> = out.logits[r][..k].iter().map(|v| v / t).collect();
            let p = model::stable_softmax(&z);
            let meta = Meta {
                act_probability: json_f32(round4(&out.act[r][0])),
            };

            let answer = match q.qtype {
                QType::Choice => {
                    let keys = q.choice_keys()?;
                    let best = argmax(&p);
                    Answer::Choice {
                        choice: keys[best].clone(),
                        probabilities: keys
                            .iter()
                            .cloned()
                            .zip(p.iter().map(|v| json_f32(round4(v))))
                            .collect(),
                        confidence: json_f32(round4(&confidence_from_probs(&p, k))),
                        rl_agent: meta,
                    }
                }
                QType::Score => {
                    let score: f32 = p.iter().enumerate().map(|(i, v)| i as f32 * v).sum();
                    Answer::Score {
                        score: json_f32(round4(&score)),
                        legend: q
                            .score_levels()
                            .into_iter()
                            .enumerate()
                            .map(|(i, c)| (i.to_string(), Value::String(c)))
                            .collect(),
                        probabilities: p
                            .iter()
                            .enumerate()
                            .map(|(i, v)| (i.to_string(), json_f32(round4(v))))
                            .collect(),
                        confidence: json_f32(round4(&confidence_from_probs(&p, k))),
                        rl_agent: meta,
                    }
                }
                QType::Noul => Answer::Noul {
                    noul: json_f32(round4(&p[1])),
                    rl_agent: meta,
                },
            };
            answers.insert(qid.clone(), serde_json::to_value(answer)?);
        }

        Ok(Response {
            model: self.model_name(),
            answers,
            usage: Usage {
                input_tokens,
                output_tokens: 0,
            },
        })
    }

    pub fn render_prompt(&self, state: &Value, q: &Question) -> Result<PromptView> {
        let e = question::build_sequence(
            &self.tokenizer,
            &self.specials,
            state,
            q,
            self.cfg.max_len,
            self.cfg.head_max_len,
        )?;
        let options = q.render_options()?;

        let mut tokens = Vec::with_capacity(e.ids.len());
        for (i, id) in e.ids.iter().enumerate() {
            let text = self
                .tokenizer
                .id_to_token(*id)
                .unwrap_or_else(|| format!("<{id}>"))
                .replace('\u{0120}', " ")
                .replace('\u{010a}', "\\n");
            let marker = e.markers.binary_search(&i).is_ok();
            let segment =
                if marker || i == 0 || i == e.head_sep || i == e.opts_sep || i + 1 == e.ids.len() {
                    Segment::Special
                } else if i < e.head_sep {
                    Segment::Head
                } else if i < e.opts_sep {
                    Segment::Option
                } else {
                    Segment::State
                };
            let option = if i >= *e.markers.first().unwrap_or(&usize::MAX) && i < e.opts_sep {
                e.markers.iter().rposition(|m| *m <= i)
            } else {
                None
            };
            tokens.push(TokenView {
                text,
                id: *id,
                segment,
                marker,
                option,
            });
        }

        let state_truncated = e.ids.len() >= self.cfg.max_len;

        Ok(PromptView {
            total_tokens: e.ids.len(),
            tokens,
            options,
            markers: e.markers,
            max_len: self.cfg.max_len,
            state_truncated,
        })
    }

    pub fn device(&self) -> &Device {
        &self.model.device
    }

    fn model_name(&self) -> String {
        if self.cfg.model_name.is_empty() {
            "rl-agent".to_string()
        } else {
            self.cfg.model_name.clone()
        }
    }
}

fn default_device() -> Device {
    Device::Cpu
}

fn resolve_specials(tok: &Tokenizer) -> Result<SpecialIds> {
    let id = |t: &str| -> Result<u32> {
        tok.token_to_id(t)
            .ok_or_else(|| anyhow::anyhow!("tokenizer has no {t} token"))
    };
    Ok(SpecialIds {
        cls: id("[CLS]")?,
        sep: id("[SEP]")?,
        mask: id("[MASK]")?,
        pad: id("[PAD]")?,
        mask_text: "[MASK]".to_string(),
    })
}

fn argmax(p: &[f32]) -> usize {
    p.iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .map(|(i, _)| i)
        .unwrap_or(0)
}

fn json_f32(v: f32) -> Value {
    serde_json::Number::from_f64(format!("{v}").parse::<f64>().unwrap_or(v as f64))
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn round4(v: &f32) -> f32 {
    (v * 10_000.0).round() / 10_000.0
}

fn confidence_from_probs(p: &[f32], k: usize) -> f32 {
    if k < 2 {
        return 1.0;
    }
    let ent: f32 = -p[..k]
        .iter()
        .map(|v| v * v.clamp(1e-12, 1.0).ln())
        .sum::<f32>();
    1.0 - ent / (k as f32).ln()
}
