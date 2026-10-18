use super::{Admission, Candidate, Choice, Information, Input, Model, Output};
use crate::communication::models::{self, Artifact, Cache, Manifest};
use candle_core::{DType, Device, IndexOp, Tensor};
use candle_nn::{Linear, Module, VarBuilder};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Instant,
};
use tokenizers::Tokenizer;

mod modern;

pub const ID: &str = "english-nli";
pub const REVISION: &str = "c07f66d9cbf781191bee66edfe8ad7856f045781";
const REPOSITORY: &str = "MoritzLaurer/xtremedistil-l6-h256-zeroshot-v1.1-all-33";
const ARTIFACTS: &[Artifact] = &[
    Artifact {
        name: "model.safetensors",
        bytes: 25_513_196,
        sha256: "bf2d83426aa6487f5cc3dc5c71d1566f33a65d883ebdb80fcb4e477c09ff0150",
    },
    Artifact {
        name: "config.json",
        bytes: 882,
        sha256: "7056598267ea0cf462d3b35c5f42c1f0be2bcb06f5e301a1861d7d6f957ca228",
    },
    Artifact {
        name: "tokenizer.json",
        bytes: 711_396,
        sha256: "d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66",
    },
    Artifact {
        name: "tokenizer_config.json",
        bytes: 366,
        sha256: "9261e7d79b44c8195c1cada2b453e55b00aeb81e907a6664974b4d7776172ab3",
    },
];

struct Checkpoint {
    id: &'static str,
    revision: &'static str,
    repository: &'static str,
    license: &'static str,
    artifacts: &'static [Artifact],
    modern: bool,
}

const SMALL: Checkpoint = Checkpoint {
    id: ID,
    revision: REVISION,
    repository: REPOSITORY,
    license: "MIT",
    artifacts: ARTIFACTS,
    modern: false,
};

pub struct Driver {
    root: PathBuf,
    checkpoint: &'static Checkpoint,
    classifier: Mutex<Option<Classifier>>,
    loaded: AtomicBool,
    pub admission_threshold: f64,
    pub routing_threshold: f64,
    pub margin_threshold: f64,
}

struct Classifier {
    tokenizer: Tokenizer,
    graph: Graph,
    maximum_tokens: usize,
}

enum Graph {
    Bert {
        encoder: laya::bert::BertModel,
        pooler: Linear,
        head: Linear,
    },
    Modern {
        encoder: laya::modernbert::ModernBert,
        projection: Linear,
        norm: candle_nn::LayerNorm,
        head: Linear,
    },
}

impl Driver {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            checkpoint: &SMALL,
            classifier: Mutex::new(None),
            loaded: AtomicBool::new(false),
            admission_threshold: 0.9,
            routing_threshold: 0.85,
            margin_threshold: 0.15,
        }
    }

    pub fn modern(root: PathBuf) -> Self {
        Self {
            checkpoint: &modern::CHECKPOINT,
            ..Self::new(root)
        }
    }

    fn cache(&self, artifact: Artifact) -> Result<Cache, String> {
        Cache::new(
            self.root.clone(),
            Manifest {
                id: self.checkpoint.id,
                repository: self.checkpoint.repository,
                revision: self.checkpoint.revision,
                license: self.checkpoint.license,
                artifact,
            },
        )
    }

    pub fn directory(&self) -> Result<PathBuf, String> {
        Ok(self.cache(self.checkpoint.artifacts[0])?.directory())
    }

    fn load(&self) -> Result<Classifier, String> {
        for artifact in self.checkpoint.artifacts {
            self.cache(*artifact)?.verified_file()?;
        }
        let directory = self.directory()?;
        let config_bytes =
            std::fs::read(directory.join("config.json")).map_err(|error| error.to_string())?;
        let raw: serde_json::Value =
            serde_json::from_slice(&config_bytes).map_err(|error| error.to_string())?;
        if raw["id2label"]["0"] != "entailment" || raw["id2label"]["1"] != "not_entailment" {
            return Err("NLI checkpoint has an unsupported label mapping.".into());
        }
        if self.checkpoint.modern {
            return modern::load(&directory, raw);
        }
        let config: laya::bert::Config =
            serde_json::from_slice(&config_bytes).map_err(|error| error.to_string())?;
        let tokenizer = Tokenizer::from_file(directory.join("tokenizer.json"))
            .map_err(|error| error.to_string())?;
        let tensors =
            candle_core::safetensors::load(directory.join("model.safetensors"), &Device::Cpu)
                .map_err(|error| error.to_string())?;
        let builder = VarBuilder::from_tensors(tensors, DType::F32, &Device::Cpu);
        let encoder = laya::bert::BertModel::load(builder.pp("bert"), &config)
            .map_err(|error| error.to_string())?;
        let pooler = candle_nn::linear(
            config.hidden_size,
            config.hidden_size,
            builder.pp("bert.pooler.dense"),
        )
        .map_err(|error| error.to_string())?;
        let head = candle_nn::linear(config.hidden_size, 2, builder.pp("classifier"))
            .map_err(|error| error.to_string())?;
        Ok(Classifier {
            tokenizer,
            graph: Graph::Bert {
                encoder,
                pooler,
                head,
            },
            maximum_tokens: config.max_position_embeddings,
        })
    }

    fn run(&self, input: &Input, cancelled: &AtomicBool) -> Result<Output, String> {
        input.validate()?;
        models::check_cancel(cancelled)?;
        let mut classifier = self
            .classifier
            .lock()
            .map_err(|_| "Reload the NLI classifier after a failure.")?;
        let started = Instant::now();
        let cold_load_ms = if classifier.is_none() {
            *classifier = Some(self.load()?);
            self.loaded.store(true, Ordering::Release);
            started.elapsed().as_millis()
        } else {
            0
        };
        let classifier = classifier.as_ref().ok_or("NLI classifier did not load.")?;
        models::check_cancel(cancelled)?;
        let started = Instant::now();
        let premise = input.text.clone();
        let admission = if input.admission {
            let dialogue = input.dialogue()?;
            let admission_text = match &dialogue {
                Some(dialogue) => format!(
                    "Prior request: {}\nAssistant reply: {}\nLatest speech: {}",
                    dialogue.request, dialogue.reply, input.text
                ),
                None => input.admission_text()?,
            };
            let hypotheses = if dialogue.is_some() {
                [
                    "The latest speech continues the active assistant conversation, answering its question or adding a relevant request or detail.",
                    "The latest speech ends or dismisses the assistant conversation with a closing remark.",
                    "The latest speech quotes media or discusses unrelated background matters.",
                    "The latest speech addresses another human instead of continuing with the assistant.",
                    "It is unclear whether the latest speech continues the assistant conversation.",
                ]
            } else if self.checkpoint.modern {
                [
                    "The current speaker directly addresses their local AI assistant for a conversation, information, or an action now.",
                    "The current speaker describes events, makes a statement, or thinks aloud rather than addressing their AI assistant.",
                    "The current speaker reports, quotes, or describes someone else's commands or words instead of making a live request.",
                    "The current speaker addresses a human or someone other than their local AI assistant.",
                    "It is unclear whether the current speaker addresses their local AI assistant or makes a request.",
                ]
            } else {
                [
                    "The person asks the assistant to do something.",
                    "The person tells a story about something that happened.",
                    "The person is quoting or discussing somebody else's words.",
                    "The person is talking to somebody other than the assistant.",
                    "It is unclear who the person addresses or what they want.",
                ]
            };
            let scores = hypotheses
                .iter()
                .map(|hypothesis| classifier.score(&admission_text, hypothesis, cancelled))
                .collect::<Result<Vec<_>, _>>()?;
            let (best, probability, margin) = strongest(&scores)?;
            let clear = probability >= self.admission_threshold && margin >= self.margin_threshold;
            Some(Choice {
                value: clear.then_some(match best {
                    0 => Admission::AssistantRequest,
                    1..=3 => Admission::OrdinaryConversation,
                    _ => Admission::Unclear,
                }),
                probability: Some(probability),
                margin: Some(margin),
                reason: if clear {
                    "Clear local NLI judgment."
                } else {
                    "NLI judgment is uncertain; abstaining."
                }
                .into(),
            })
        } else {
            None
        };
        let destination = if input.routing {
            Some(self.route(classifier, &premise, &input.candidates, cancelled)?)
        } else {
            None
        };
        models::check_cancel(cancelled)?;
        Ok(Output {
            request_id: input.request_id.clone(),
            generation: input.generation,
            model: self.checkpoint.id.into(),
            revision: self.checkpoint.revision.into(),
            admission,
            destination,
            cold_load_ms,
            inference_ms: started.elapsed().as_millis(),
        })
    }

    fn route(
        &self,
        classifier: &Classifier,
        premise: &str,
        candidates: &[Candidate],
        cancelled: &AtomicBool,
    ) -> Result<Choice<String>, String> {
        if candidates.is_empty() {
            return Ok(Choice::abstain("No eligible destinations."));
        }
        let mut scores = Vec::new();
        for candidate in candidates {
            let hypothesis = format!(
                "The person needs help with {}: {}",
                candidate.title, candidate.purpose
            );
            scores.push(classifier.score(premise, &hypothesis, cancelled)?);
        }
        let (best, probability, margin) = strongest(&scores)?;
        let clear = probability >= self.routing_threshold && margin >= self.margin_threshold;
        Ok(Choice {
            value: clear.then(|| candidates[best].record.clone()),
            probability: Some(probability),
            margin: Some(margin),
            reason: if clear {
                "Clear local NLI destination."
            } else {
                "No clear eligible NLI destination; use the default assistant."
            }
            .into(),
        })
    }
}

impl Classifier {
    fn score(
        &self,
        premise: &str,
        hypothesis: &str,
        cancelled: &AtomicBool,
    ) -> Result<f64, String> {
        models::check_cancel(cancelled)?;
        let encoded = self
            .tokenizer
            .encode((premise, hypothesis), true)
            .map_err(|error| error.to_string())?;
        if encoded.len() > self.maximum_tokens {
            return Err("NLI input exceeds its token limit; no text was truncated.".into());
        }
        let ids = Tensor::new(encoded.get_ids(), &Device::Cpu)
            .and_then(|tensor| tensor.unsqueeze(0))
            .map_err(|error| error.to_string())?;
        let types = Tensor::new(encoded.get_type_ids(), &Device::Cpu)
            .and_then(|tensor| tensor.unsqueeze(0))
            .map_err(|error| error.to_string())?;
        let mask = Tensor::new(encoded.get_attention_mask(), &Device::Cpu)
            .and_then(|tensor| tensor.unsqueeze(0))
            .map_err(|error| error.to_string())?;
        let logits = match &self.graph {
            Graph::Bert {
                encoder,
                pooler,
                head,
            } => {
                let encoded = encoder
                    .forward(&ids, &types, Some(&mask))
                    .map_err(|error| error.to_string())?;
                let pooled = pooler
                    .forward(&encoded.i((.., 0, ..)).map_err(|error| error.to_string())?)
                    .and_then(|tensor| tensor.tanh())
                    .map_err(|error| error.to_string())?;
                head.forward(&pooled).map_err(|error| error.to_string())?
            }
            Graph::Modern {
                encoder,
                projection,
                norm,
                head,
            } => {
                let mask = mask
                    .to_dtype(DType::F32)
                    .map_err(|error| error.to_string())?;
                let encoded = encoder
                    .forward(&ids, &mask)
                    .map_err(|error| error.to_string())?;
                let pooled = encoded
                    .broadcast_mul(&mask.unsqueeze(2).map_err(|error| error.to_string())?)
                    .and_then(|tensor| tensor.sum(1))
                    .and_then(|tensor| tensor.broadcast_div(&mask.sum_keepdim(1)?))
                    .map_err(|error| error.to_string())?;
                let hidden = pooled
                    .apply(projection)
                    .and_then(|tensor| tensor.gelu_erf())
                    .and_then(|tensor| tensor.apply(norm))
                    .map_err(|error| error.to_string())?;
                head.forward(&hidden).map_err(|error| error.to_string())?
            }
        };
        let probabilities = candle_nn::ops::softmax_last_dim(&logits)
            .and_then(|tensor| tensor.to_vec2::<f32>())
            .map_err(|error| error.to_string())?;
        models::check_cancel(cancelled)?;
        let probability = probabilities[0][0] as f64;
        if !probability.is_finite() {
            return Err("NLI classifier returned an invalid score.".into());
        }
        Ok(probability)
    }
}

fn strongest(scores: &[f64]) -> Result<(usize, f64, f64), String> {
    let mut ranked = scores.iter().copied().enumerate().collect::<Vec<_>>();
    if ranked.is_empty() || ranked.iter().any(|(_, score)| !score.is_finite()) {
        return Err("NLI classifier returned no finite choice.".into());
    }
    ranked.sort_by(|left, right| right.1.total_cmp(&left.1));
    Ok((
        ranked[0].0,
        ranked[0].1,
        ranked[0].1 - ranked.get(1).map_or(0.0, |entry| entry.1),
    ))
}

#[async_trait::async_trait]
impl Model for Driver {
    fn information(&self) -> Information {
        Information {
            model: self.checkpoint.id.into(),
            revision: self.checkpoint.revision.into(),
            required_bytes: self
                .checkpoint
                .artifacts
                .iter()
                .map(|artifact| artifact.bytes)
                .sum(),
            routing_supported: true,
            routing_validated: false,
            admission_validated: false,
        }
    }
    fn present(&self) -> bool {
        self.checkpoint
            .artifacts
            .iter()
            .all(|artifact| self.cache(*artifact).is_ok_and(|cache| cache.present()))
    }
    fn loaded(&self) -> bool {
        self.loaded.load(Ordering::Acquire)
    }
    async fn acquire(
        &self,
        source: Option<PathBuf>,
        cancelled: Arc<AtomicBool>,
        progress: Arc<AtomicU64>,
    ) -> Result<(), String> {
        let mut acquired = 0;
        for artifact in self.checkpoint.artifacts {
            models::check_cancel(&cancelled)?;
            let cache = self.cache(*artifact)?;
            let item_progress = Arc::new(AtomicU64::new(0));
            let operation = cache.acquire(
                source.as_ref().map(|source| source.join(artifact.name)),
                cancelled.clone(),
                item_progress.clone(),
            );
            tokio::pin!(operation);
            loop {
                tokio::select! {
                    result = &mut operation => { result?; break; }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                        progress.store(acquired + item_progress.load(Ordering::Acquire), Ordering::Release);
                    }
                }
            }
            acquired += artifact.bytes;
            progress.store(acquired, Ordering::Release);
        }
        Ok(())
    }
    fn decide(&self, input: &Input, cancelled: &AtomicBool) -> Result<Output, String> {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.run(input, cancelled)))
        {
            Ok(result) => result,
            Err(_) => {
                *self
                    .classifier
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = None;
                self.classifier.clear_poison();
                self.loaded.store(false, Ordering::Release);
                Err("NLI inference failed. Retry to reload its model.".into())
            }
        }
    }
}
