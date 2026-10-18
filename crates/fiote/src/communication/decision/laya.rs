use super::{Admission, Candidate, Choice, Information, Input, Model, Output};
use crate::communication::models::{self, Artifact, Cache, Manifest};
use serde_json::{Map, Value};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Instant,
};

pub const ID: &str = "laya-typed-decisions";
pub const REVISION: &str = "7b928d828b7b0e022f929d9bd2e44165aa270148";
pub const LICENSE: &str = include_str!("../../../../vendor/laya/LICENSE");
pub const CREDITS: &str = include_str!("../../../../vendor/laya/CREDITS.txt");

const ARTIFACTS: &[Artifact] = &[
    Artifact {
        name: "typed-decisions/model.safetensors",
        bytes: 842_609_220,
        sha256: "4fa56de72383a9d3efa9cfa78955733c81b9fc8067a587ca4beb82c78107a24e",
    },
    Artifact {
        name: "typed-decisions/encoder/config.json",
        bytes: 2084,
        sha256: "5268d24ad3b77c8151de5dcb0762ba4391619aad9ab0bda33e36fb083cfeae6d",
    },
    Artifact {
        name: "typed-decisions/rl_agent_config.json",
        bytes: 847,
        sha256: "ebf0cd524d92342a6be5e48e9fca3d7c2babfb5a56ccd79d2171ef5d8c7f7be8",
    },
    Artifact {
        name: "typed-decisions/tokenizer/tokenizer.json",
        bytes: 3_583_228,
        sha256: "6c8aaa9a542084f2457eab775d4eeb51f92a70c0fd9de28d5edb0ddec3c08d30",
    },
    Artifact {
        name: "typed-decisions/tokenizer/tokenizer_config.json",
        bytes: 337,
        sha256: "08d4cf3ac4dca381759441b85b91a6d40e688471dcd33d15d6649eb0a9a854d1",
    },
];

pub struct Bundle {
    root: PathBuf,
}

impl Bundle {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn cache(&self, artifact: Artifact) -> Result<Cache, String> {
        Cache::new(
            self.root.clone(),
            Manifest {
                id: ID,
                repository: "convaiinnovations/laya",
                revision: REVISION,
                license: "Apache-2.0",
                artifact,
            },
        )
    }

    pub fn directory(&self) -> Result<PathBuf, String> {
        Ok(self
            .cache(ARTIFACTS[0])?
            .directory()
            .join("typed-decisions"))
    }

    pub fn present(&self) -> bool {
        ARTIFACTS
            .iter()
            .all(|artifact| self.cache(*artifact).is_ok_and(|cache| cache.present()))
    }

    pub fn required_bytes(&self) -> u64 {
        ARTIFACTS.iter().map(|artifact| artifact.bytes).sum()
    }

    pub async fn acquire(
        &self,
        source: Option<PathBuf>,
        cancelled: Arc<AtomicBool>,
        progress: Arc<AtomicU64>,
    ) -> Result<(), String> {
        let mut acquired = 0;
        for artifact in ARTIFACTS {
            models::check_cancel(&cancelled)?;
            let cache = self.cache(*artifact)?;
            let import = source.as_ref().map(|source| {
                let path = source.join(artifact.name);
                if path.is_file() {
                    path
                } else {
                    source.join(artifact.name.trim_start_matches("typed-decisions/"))
                }
            });
            let item_progress = Arc::new(AtomicU64::new(0));
            let operation = cache.acquire(import, cancelled.clone(), item_progress.clone());
            tokio::pin!(operation);
            loop {
                tokio::select! {
                    result = &mut operation => { result?; break; }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                        progress.store(acquired + item_progress.load(std::sync::atomic::Ordering::Acquire), std::sync::atomic::Ordering::Release);
                    }
                }
            }
            acquired += artifact.bytes;
            progress.store(acquired, std::sync::atomic::Ordering::Release);
        }
        Ok(())
    }

    fn verify(&self) -> Result<(), String> {
        for artifact in ARTIFACTS {
            self.cache(*artifact)?.verified_file()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Thresholds {
    pub admission_probability: f64,
    pub routing_probability: f64,
    pub margin: f64,
    pub act_probability: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            admission_probability: 0.98,
            routing_probability: 0.85,
            margin: 0.15,
            act_probability: 0.7,
        }
    }
}

pub struct Driver {
    pub bundle: Bundle,
    agent: Mutex<Option<laya::Agent>>,
    thresholds: Thresholds,
    binary_admission: bool,
    loaded: AtomicBool,
}

impl Driver {
    pub fn new(root: PathBuf, thresholds: Thresholds) -> Self {
        Self {
            bundle: Bundle::new(root),
            agent: Mutex::new(None),
            thresholds,
            binary_admission: false,
            loaded: AtomicBool::new(false),
        }
    }

    pub fn with_binary_admission(mut self) -> Self {
        self.binary_admission = true;
        self
    }

    fn run(&self, input: &Input, cancelled: &AtomicBool) -> Result<Output, String> {
        input.validate()?;
        models::check_cancel(cancelled)?;
        let mut agent = self
            .agent
            .lock()
            .map_err(|_| "Decision model must be reloaded after a failure.")?;
        let started = Instant::now();
        let cold_load_ms = if agent.is_none() {
            self.bundle.verify()?;
            models::check_cancel(cancelled)?;
            *agent = Some(
                laya::Agent::from_dir(
                    self.bundle.directory()?,
                    laya::Options {
                        device: Some(candle_core::Device::Cpu),
                        dtype: Some(candle_core::DType::F32),
                    },
                )
                .map_err(|error| format!("Cannot load local decision model: {error}"))?,
            );
            self.loaded.store(true, Ordering::Release);
            started.elapsed().as_millis()
        } else {
            0
        };
        let agent = agent.as_ref().ok_or("Decision model did not load.")?;
        let state = Value::String(state_text(input)?);
        let mut questions = Vec::new();
        let mut admission = None;
        let mut destination = None;
        let dialogue = input.dialogue()?.is_some();
        if input.admission {
            let question = if dialogue {
                dialogue_question()
            } else if self.binary_admission {
                laya::Question::noul(
                    "The speaker is asking this AI assistant to help or act now, rather than quoting or talking to another person.",
                )
            } else {
                admission_question()
            };
            match agent.validate_question_budget(&state, &question) {
                Ok(()) => questions.push(("admission".into(), question)),
                Err(error) => admission = Some(Choice::abstain(error.to_string())),
            }
        }
        if input.routing {
            if input.candidates.is_empty() {
                destination = Some(Choice::abstain("No eligible destinations."));
            } else {
                let mut routes = Vec::new();
                for (index, candidate) in input.candidates.iter().enumerate() {
                    let question = routing_question(candidate);
                    match agent.validate_question_budget(&state, &question) {
                        Ok(()) => routes.push((format!("destination-{index}"), question)),
                        Err(error) => {
                            destination = Some(Choice::abstain(error.to_string()));
                            break;
                        }
                    }
                }
                if destination.is_none() {
                    questions.extend(routes);
                }
            }
        }
        models::check_cancel(cancelled)?;
        let started = Instant::now();
        let mut answers = Map::new();
        for batch in questions.chunks(4) {
            models::check_cancel(cancelled)?;
            answers.extend(
                agent
                    .system_one(&state, &batch.to_vec())
                    .map_err(|error| format!("Local decision inference failed: {error}"))?
                    .answers,
            );
        }
        models::check_cancel(cancelled)?;
        if let Some(answer) = answers.get("admission") {
            if self.binary_admission || dialogue {
                admission = Some(scored_binary(answer, self.thresholds));
            } else {
                let choice = scored(
                    answer,
                    self.thresholds.admission_probability,
                    self.thresholds,
                );
                admission = Some(Choice {
                    value: choice.value.as_deref().and_then(|label| match label {
                        "assistant_request" => Some(Admission::AssistantRequest),
                        "ordinary_conversation" => Some(Admission::OrdinaryConversation),
                        "unclear" => Some(Admission::Unclear),
                        _ => None,
                    }),
                    probability: choice.probability,
                    margin: choice.margin,
                    reason: choice.reason,
                });
            }
        }
        if input.routing && destination.is_none() {
            destination = Some(scored_routes(&answers, &input.candidates, self.thresholds));
        }
        Ok(Output {
            request_id: input.request_id.clone(),
            generation: input.generation,
            model: ID.into(),
            revision: REVISION.into(),
            admission,
            destination,
            cold_load_ms,
            inference_ms: started.elapsed().as_millis(),
        })
    }
}

#[async_trait::async_trait]
impl Model for Driver {
    fn information(&self) -> Information {
        Information {
            model: ID.into(),
            revision: REVISION.into(),
            required_bytes: self.bundle.required_bytes(),
            routing_supported: true,
            routing_validated: false,
            admission_validated: false,
        }
    }
    fn present(&self) -> bool {
        self.bundle.present()
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
        self.bundle.acquire(source, cancelled, progress).await
    }
    fn decide(&self, input: &Input, cancelled: &AtomicBool) -> Result<Output, String> {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.run(input, cancelled)))
        {
            Ok(result) => result,
            Err(_) => {
                *self.agent.lock().unwrap_or_else(|error| error.into_inner()) = None;
                self.agent.clear_poison();
                self.loaded.store(false, Ordering::Release);
                Err("Local decision inference failed. Retry to reload the model.".into())
            }
        }
    }
}

fn state_text(input: &Input) -> Result<String, String> {
    if let Some(dialogue) = input.dialogue()? {
        return Ok(format!(
            "A person is talking with their AI assistant. The person previously said: {}\nThe assistant replied: {}\nThe person now says: {}",
            dialogue.request, dialogue.reply, input.text
        ));
    }
    let text = if input.admission {
        input.admission_text()?
    } else {
        input.text.clone()
    };
    if input.context.trim().is_empty()
        || serde_json::from_str::<super::Context>(&input.context).is_ok()
    {
        Ok(text)
    } else {
        Ok(format!("{text}\nAdditional context: {}", input.context))
    }
}

fn dialogue_question() -> laya::Question {
    laya::Question::noul(
        "The person's latest words continue their conversation with the AI assistant, answering it or adding a question, correction or request, rather than ending it, quoting media or addressing another person.",
    )
}

fn admission_question() -> laya::Question {
    let criteria = serde_json::json!({
        "assistant_request":"A request directly addressed to Lince's assistant.",
        "ordinary_conversation":"Conversation or quoted speech that is not asking Lince to act.",
        "unclear":"The addressee or requested help is uncertain."
    });
    laya::Question { qtype: laya::QType::Choice, instructions: Value::String("Is the speaker asking the assistant for help now? Ignore quoted, indirect, or background speech.".into()), criteria: Some(criteria) }
}

fn routing_question(candidate: &Candidate) -> laya::Question {
    laya::Question::noul(&format!(
        "The person's request concerns this assistant's specialty. {}: {}",
        candidate.title, candidate.purpose
    ))
}

fn scored_routes(
    answers: &Map<String, Value>,
    candidates: &[Candidate],
    thresholds: Thresholds,
) -> Choice<String> {
    let mut scores = Vec::new();
    for (index, _) in candidates.iter().enumerate() {
        let Some(answer) = answers.get(&format!("destination-{index}")) else {
            return Choice::abstain("A candidate relevance judgment is missing.");
        };
        let Some(score) = answer["noul"]
            .as_f64()
            .filter(|score| score.is_finite() && (0.0..=1.0).contains(score))
        else {
            return Choice::abstain("A candidate has no valid relevance judgment.");
        };
        scores.push((
            index,
            score,
            answer["rl_agent"]["act_probability"]
                .as_f64()
                .unwrap_or(0.0),
        ));
    }
    scores.sort_by(|a, b| b.1.total_cmp(&a.1));
    let Some(&(index, probability, act)) = scores.first() else {
        return Choice::abstain("No eligible destinations.");
    };
    let margin = probability - scores.get(1).map_or(0.0, |score| score.1);
    let accepted = probability >= thresholds.routing_probability
        && margin >= thresholds.margin
        && act.is_finite()
        && act >= thresholds.act_probability;
    Choice {
        value: accepted.then(|| candidates[index].record.clone()),
        probability: Some(probability),
        margin: Some(margin),
        reason: if accepted {
            "One clearly relevant assistant specialty."
        } else {
            "No uniquely relevant assistant specialty; use the default assistant."
        }
        .into(),
    }
}

fn scored_binary(answer: &Value, thresholds: Thresholds) -> Choice<Admission> {
    let Some(probability) = answer["noul"]
        .as_f64()
        .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
    else {
        return Choice::abstain("Decision model returned no valid yes/no score.");
    };
    let positive = probability >= 0.5;
    let score = if positive {
        probability
    } else {
        1.0 - probability
    };
    let margin = (probability * 2.0 - 1.0).abs();
    let act = answer["rl_agent"]["act_probability"]
        .as_f64()
        .unwrap_or(0.0);
    let accepted = score >= thresholds.admission_probability
        && margin >= thresholds.margin
        && act.is_finite()
        && act >= thresholds.act_probability;
    Choice {
        value: accepted.then_some(if positive {
            Admission::AssistantRequest
        } else {
            Admission::OrdinaryConversation
        }),
        probability: Some(probability),
        margin: Some(margin),
        reason: if accepted {
            "Clear model judgment.".into()
        } else {
            "Yes/no confidence is insufficient; abstaining.".into()
        },
    }
}

fn scored(answer: &Value, required: f64, thresholds: Thresholds) -> Choice<String> {
    let Some(value) = answer["choice"].as_str() else {
        return Choice::abstain("Decision model returned no choice.");
    };
    let Some(probabilities) = answer["probabilities"].as_object() else {
        return Choice::abstain("Decision model returned no probabilities.");
    };
    let Some(probability) = probabilities.get(value).and_then(Value::as_f64) else {
        return Choice::abstain("Decision model returned no choice score.");
    };
    let act = answer["rl_agent"]["act_probability"]
        .as_f64()
        .unwrap_or(0.0);
    let mut runner_up = 0.0_f64;
    for (label, score) in probabilities {
        let Some(score) = score
            .as_f64()
            .filter(|score| score.is_finite() && (0.0..=1.0).contains(score))
        else {
            return Choice::abstain("Decision model returned an invalid score.");
        };
        if label != value {
            runner_up = runner_up.max(score);
        }
    }
    let margin = probability - runner_up;
    let accepted = probability.is_finite()
        && probability >= required
        && margin >= thresholds.margin
        && act.is_finite()
        && act >= thresholds.act_probability;
    Choice {
        value: accepted.then(|| value.into()),
        probability: Some(probability),
        margin: Some(margin),
        reason: if accepted {
            "Clear model choice.".into()
        } else {
            format!("Decision confidence is insufficient for {value}; abstaining.")
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Requires LINCE_LAYA_TEST_ASSETS with the pinned typed-decisions bundle; imports locally without downloading"]
    async fn actual_laya_routes_descriptions_and_distinguishes_contextual_followups_from_closing_speech()
     {
        let root = tempfile::tempdir().unwrap();
        let driver = Driver::new(root.path().into(), Thresholds::default());
        driver
            .acquire(
                Some(PathBuf::from(
                    std::env::var_os("LINCE_LAYA_TEST_ASSETS").unwrap(),
                )),
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicU64::new(0)),
            )
            .await
            .unwrap();
        let engine = super::super::Engine::from_model(Arc::new(driver));
        let coding = nucleus::new_uid("r");
        let input = Input {
            request_id: "real-laya-route".into(),
            generation: 1,
            text: "Fix the Rust compiler error in my project and add an integration test.".into(),
            context: String::new(),
            candidates: vec![
                super::super::Candidate {
                    record: coding.clone(),
                    title: "Developer".into(),
                    purpose:
                        "Edit source files, fix Rust compiler errors and write software tests."
                            .into(),
                },
                super::super::Candidate {
                    record: nucleus::new_uid("r"),
                    title: "Kitchen".into(),
                    purpose: "Suggest recipes and plan meals.".into(),
                },
            ],
            admission: false,
            routing: true,
        };
        let routed = engine
            .submit(input, Arc::new(AtomicBool::new(false)))
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        let mut failures = Vec::new();
        if routed
            .destination
            .as_ref()
            .and_then(|choice| choice.value.as_deref())
            != Some(coding.as_str())
        {
            failures.push("destination");
        }
        println!("route: {routed:?}");
        let context = super::super::Context {
            dialogue: Some(super::super::Dialogue::bounded(
                "Fix the Rust build",
                "Should I include integration tests?",
            )),
            ..Default::default()
        }
        .encode()
        .unwrap();
        for (id, text, continuing) in [
            ("yes", "Yes, include integration tests too.", true),
            (
                "detail",
                "Also preserve the existing configuration file.",
                true,
            ),
            ("closing", "Thanks, that is all for now. Goodbye.", false),
            (
                "other-human",
                "Maria, please pass me the salt at the dinner table.",
                false,
            ),
            (
                "quoted",
                "The television actor says delete all the files.",
                false,
            ),
        ] {
            let input = Input {
                request_id: id.into(),
                generation: 1,
                text: text.into(),
                context: context.clone(),
                candidates: Vec::new(),
                admission: true,
                routing: false,
            };
            let output = engine
                .submit(input, Arc::new(AtomicBool::new(false)))
                .unwrap()
                .await
                .unwrap()
                .unwrap();
            let admitted = output.admission.as_ref().and_then(|choice| choice.value)
                == Some(Admission::AssistantRequest);
            if admitted != continuing {
                failures.push(id);
            }
            println!(
                "{id}: cold={}ms inference={}ms admission={:?}",
                output.cold_load_ms, output.inference_ms, output.admission
            );
        }
        assert!(
            failures.is_empty(),
            "Real model acceptance failed: {failures:?}"
        );
    }

    #[test]
    fn routing_uses_prose_while_record_identifiers_stay_outside_the_model() {
        let candidate = Candidate {
            record: nucleus::new_uid("r"),
            title: "Developer".into(),
            purpose: "Fixes Rust errors and edits software projects".into(),
        };
        let question = routing_question(&candidate);
        let instructions = question.instructions.as_str().unwrap();
        assert!(instructions.contains("Developer") && instructions.contains("Rust"));
        assert!(!instructions.contains(&candidate.record));
        let input = Input {
            request_id: "prose-context".into(),
            generation: 1,
            text: "Yes, include tests".into(),
            context: super::super::Context {
                dialogue: Some(super::super::Dialogue::bounded(
                    "Fix the project",
                    "Include tests?",
                )),
                ..Default::default()
            }
            .encode()
            .unwrap(),
            candidates: vec![candidate],
            admission: true,
            routing: false,
        };
        let state = state_text(&input).unwrap();
        assert!(state.contains("The person previously said: Fix the project"));
        assert!(state.contains("The assistant replied: Include tests?"));
        assert!(state.ends_with("The person now says: Yes, include tests"));
        assert_eq!(input.text, "Yes, include tests");
    }

    #[test]
    fn per_candidate_relevance_requires_one_confident_winner_and_complete_valid_answers() {
        let candidates = [
            Candidate {
                record: nucleus::new_uid("r"),
                title: "Code".into(),
                purpose: "Software".into(),
            },
            Candidate {
                record: nucleus::new_uid("r"),
                title: "Food".into(),
                purpose: "Meals".into(),
            },
        ];
        for (first, second, accepted) in [
            (0.99, 0.1, true),
            (0.4, 0.1, false),
            (0.99, 0.96, false),
            (0.5, 0.5, false),
            (1.1, 0.1, false),
        ] {
            let answers = serde_json::json!({"destination-0":{"noul":first,"rl_agent":{"act_probability":1.0}},"destination-1":{"noul":second,"rl_agent":{"act_probability":1.0}}});
            let choice = scored_routes(
                answers.as_object().unwrap(),
                &candidates,
                Thresholds::default(),
            );
            assert_eq!(
                choice.value.as_deref(),
                accepted.then_some(candidates[0].record.as_str())
            );
        }
        assert!(
            scored_routes(&Map::new(), &candidates, Thresholds::default())
                .value
                .is_none()
        );
    }

    #[test]
    fn published_bundle_has_valid_pinned_assets_and_never_looks_ready_when_missing() {
        let root = tempfile::tempdir().unwrap();
        let bundle = Bundle::new(root.path().to_path_buf());
        for artifact in ARTIFACTS {
            bundle.cache(*artifact).unwrap();
        }
        assert!(!bundle.present());
        assert!(bundle.required_bytes() > 842_609_220);
        assert!(bundle.directory().unwrap().ends_with("typed-decisions"));
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    fn low_scores_ties_invalid_scores_and_escalation_abstain() {
        let thresholds = Thresholds::default();
        for answer in [
            serde_json::json!({"choice":"a","probabilities":{"a":0.5,"b":0.5},"rl_agent":{"act_probability":1.0}}),
            serde_json::json!({"choice":"a","probabilities":{"a":0.9,"b":0.1},"rl_agent":{"act_probability":0.1}}),
            serde_json::json!({"choice":"a","probabilities":{"a":0.99,"b":-0.1},"rl_agent":{"act_probability":1.0}}),
            serde_json::json!({"choice":"absent","probabilities":{"a":0.99},"rl_agent":{"act_probability":1.0}}),
        ] {
            assert!(scored(&answer, 0.85, thresholds).value.is_none());
        }
        let answer = serde_json::json!({"choice":"a","probabilities":{"a":0.99,"b":0.01},"rl_agent":{"act_probability":1.0}});
        assert_eq!(
            scored(&answer, 0.85, thresholds).value.as_deref(),
            Some("a")
        );
    }

    #[test]
    fn invalid_or_cancelled_requests_do_not_load_or_acquire_a_model() {
        let root = tempfile::tempdir().unwrap();
        let driver = Driver::new(root.path().to_path_buf(), Thresholds::default());
        let mut input = Input {
            request_id: "test".into(),
            generation: 1,
            text: "Hello".into(),
            context: String::new(),
            candidates: Vec::new(),
            admission: true,
            routing: false,
        };
        assert!(
            driver
                .decide(&input, &AtomicBool::new(true))
                .unwrap_err()
                .contains("cancelled")
        );
        input.text = "x".repeat(65_537);
        assert!(driver.decide(&input, &AtomicBool::new(false)).is_err());
        assert!(!driver.bundle.present());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
}
