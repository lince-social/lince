use super::*;
use ::fiote::communication::decision::{Choice, Model, Output};
use ::fiote::provider::{Message, Provider, Reply, ToolDefinition};
use base64::Engine as _;

struct RecognitionFixture {
    present: bool,
    text: String,
    calls: AtomicU64,
    permits: Option<Arc<tokio::sync::Semaphore>>,
}

#[async_trait::async_trait]
impl Recognition for RecognitionFixture {
    fn present(&self) -> bool {
        self.present
    }
    async fn transcribe(
        &self,
        _: ::fiote::config::Secret,
        cancelled: Arc<AtomicBool>,
    ) -> Result<String, String> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        if let Some(permits) = &self.permits {
            permits
                .acquire()
                .await
                .map_err(|error| error.to_string())?
                .forget();
        }
        ::fiote::communication::models::check_cancel(&cancelled)?;
        Ok(self.text.clone())
    }
}

struct DecisionFixture {
    admission: Option<Admission>,
    validated: bool,
    destination: Option<String>,
    wrong_generation: bool,
    seen: Option<Arc<std::sync::Mutex<Vec<Input>>>>,
}

#[async_trait::async_trait]
impl Model for DecisionFixture {
    fn information(&self) -> Information {
        Information {
            model: "fixture".into(),
            revision: "1".into(),
            required_bytes: 0,
            routing_supported: true,
            admission_validated: self.validated,
            routing_validated: false,
        }
    }
    fn present(&self) -> bool {
        true
    }
    fn loaded(&self) -> bool {
        true
    }
    async fn acquire(
        &self,
        _: Option<PathBuf>,
        _: Arc<AtomicBool>,
        _: Arc<AtomicU64>,
    ) -> Result<(), String> {
        Ok(())
    }
    fn decide(&self, input: &Input, cancelled: &AtomicBool) -> Result<Output, String> {
        ::fiote::communication::models::check_cancel(cancelled)?;
        if let Some(seen) = &self.seen {
            seen.lock().unwrap().push(input.clone());
        }
        Ok(Output {
            request_id: input.request_id.clone(),
            generation: input.generation + u64::from(self.wrong_generation),
            model: "fixture".into(),
            revision: "1".into(),
            admission: Some(Choice {
                value: self.admission,
                probability: None,
                margin: None,
                reason: "fixture".into(),
            }),
            destination: Some(Choice {
                value: self.destination.clone(),
                probability: None,
                margin: None,
                reason: "fixture".into(),
            }),
            cold_load_ms: 0,
            inference_ms: 0,
        })
    }
}

#[derive(Default)]
struct DeliveryFixture {
    requests: Mutex<Vec<EntryRequest>>,
    error: Option<String>,
    inspection_error: bool,
    inspected: Option<EntryReceipt>,
    candidates: Vec<::fiote::assistant::Candidate>,
    followup: Option<Admission>,
    followup_calls: AtomicU64,
    followup_permits: Option<Arc<tokio::sync::Semaphore>>,
}

#[async_trait::async_trait]
impl Delivery for DeliveryFixture {
    async fn context(&self, _: &EntryReceipt) -> Result<decision::Dialogue, String> {
        Ok(decision::Dialogue::bounded(
            "Plan my work",
            "Should I include the Rust project?",
        ))
    }
    async fn classify_followup(
        &self,
        _: &Conversation,
        _: &decision::Dialogue,
        _: &str,
    ) -> Result<Admission, String> {
        self.followup_calls.fetch_add(1, Ordering::AcqRel);
        if let Some(permits) = &self.followup_permits {
            permits
                .acquire()
                .await
                .map_err(|error| error.to_string())?
                .forget();
        }
        self.followup
            .ok_or("The fixture continuation classifier is unavailable.".into())
    }
    async fn continue_conversation(
        &self,
        conversation: Conversation,
        request: EntryRequest,
    ) -> Result<EntryReceipt, String> {
        assert_eq!(
            request.target.as_deref(),
            Some(conversation.target.as_str())
        );
        let mut receipt = self.send(request).await?;
        receipt.thread = Some(conversation.thread);
        Ok(receipt)
    }
    async fn inspect(&self, request_id: &str) -> Result<Option<EntryReceipt>, String> {
        if self.inspection_error {
            return Err("Receipt storage unavailable".into());
        }
        Ok(self.inspected.clone().map(|mut receipt| {
            receipt.request_id = request_id.into();
            receipt
        }))
    }
    async fn directory(&self) -> Result<Directory, String> {
        Ok(Directory {
            fiotes: self.candidates.clone(),
            next_offset: None,
            intake: None,
        })
    }
    async fn send(&self, request: EntryRequest) -> Result<EntryReceipt, String> {
        self.requests.lock().await.push(request.clone());
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        Ok(EntryReceipt {
            request_id: request.request_id,
            fingerprint: "fixture".into(),
            target: request.target.unwrap_or_else(|| "default".into()),
            thread: Some(nucleus::new_uid("r")),
            message: Some(nucleus::new_uid("r")),
            reply: Some(nucleus::new_uid("r")),
            state: "running".into(),
            detail: String::new(),
        })
    }
}

fn audio() -> ::fiote::config::Secret {
    ::fiote::config::Secret(
        base64::engine::general_purpose::STANDARD.encode(include_bytes!(
            "../../../../interface/tests/fixtures/fiote/audio.wav"
        )),
    )
}

fn recognition(permits: Option<Arc<tokio::sync::Semaphore>>) -> Arc<RecognitionFixture> {
    Arc::new(RecognitionFixture {
        present: true,
        text: "Assistant, please handle my request".into(),
        calls: AtomicU64::new(0),
        permits,
    })
}

struct QueuedRecognition(std::sync::Mutex<VecDeque<String>>);

#[async_trait::async_trait]
impl Recognition for QueuedRecognition {
    fn present(&self) -> bool {
        true
    }
    async fn transcribe(
        &self,
        _: ::fiote::config::Secret,
        cancelled: Arc<AtomicBool>,
    ) -> Result<String, String> {
        ::fiote::communication::models::check_cancel(&cancelled)?;
        self.0
            .lock()
            .unwrap()
            .pop_front()
            .ok_or("No fixture transcript remains.".into())
    }
}

#[tokio::test]
async fn wake_mode_requires_initial_activation_then_continues_without_optional_models() {
    let root = tempfile::tempdir().unwrap();
    let controller = Controller::default();
    let owner = nucleus::new_uid("voice");
    let delivery = Arc::new(DeliveryFixture {
        followup: Some(Admission::AssistantRequest),
        ..Default::default()
    });
    let recognition = Arc::new(QueuedRecognition(std::sync::Mutex::new(
        [
            "Please plan my tasks",
            "check my notes",
            "one more detail",
            "include tomorrow",
        ]
        .map(String::from)
        .into(),
    )));
    let model = Arc::new(decision::Engine::local(root.path().join("unused")));
    controller
        .start_pipeline(
            ListeningMode::Wake,
            owner.clone(),
            None,
            true,
            recognition.clone(),
            model.clone(),
            delivery.clone(),
        )
        .await
        .unwrap();
    let generation = controller.status().await.generation;
    for (id, woken) in [(1, false), (2, true), (3, false), (4, true), (5, false)] {
        if woken {
            controller
                .push_woken(&owner, generation, id, audio())
                .await
                .unwrap();
        } else {
            controller
                .push(&owner, generation, id, audio())
                .await
                .unwrap();
        }
        idle(&controller).await;
        assert!(
            controller
                .push_woken(&owner, generation, id, audio())
                .await
                .unwrap_err()
                .contains("already")
        );
    }
    let requests = delivery.requests.lock().await;
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[0].text, "Please plan my tasks");
    assert_eq!(requests[1].text, "check my notes");
    assert!(requests[0].target.is_none());
    assert!(
        requests[1..]
            .iter()
            .all(|request| request.target.as_deref() == Some("default"))
    );
    assert!(recognition.0.lock().unwrap().is_empty());
    assert!(!model.loaded());
    drop(requests);
    controller.shutdown().await;
}

#[tokio::test]
async fn a_supported_unvalidated_router_selects_once_then_followups_and_speech_pauses_keep_the_thread()
 {
    let controller = Controller::default();
    let owner = nucleus::new_uid("voice");
    let target = nucleus::new_uid("r");
    let disabled = nucleus::new_uid("r");
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let model = Arc::new(decision::Engine::from_model(Arc::new(DecisionFixture {
        admission: Some(Admission::AssistantRequest),
        validated: false,
        destination: Some(target.clone()),
        wrong_generation: false,
        seen: Some(seen.clone()),
    })));
    assert!(model.information().routing_supported);
    assert!(!model.information().routing_validated);
    let delivery = Arc::new(DeliveryFixture {
        followup: Some(Admission::AssistantRequest),
        candidates: vec![
            ::fiote::assistant::Candidate {
                record: target.clone(),
                title: "Coding".into(),
                description: "Edit project files and help with Rust".into(),
                description_truncated: false,
                enabled: true,
            },
            ::fiote::assistant::Candidate {
                record: disabled.clone(),
                title: "Unavailable".into(),
                description: "Unavailable connection".into(),
                description_truncated: false,
                enabled: false,
            },
        ],
        ..Default::default()
    });
    let recognition = Arc::new(QueuedRecognition(std::sync::Mutex::new(
        [
            "Please help with my Rust project".to_string(),
            format!("@{disabled} This is still my follow-up"),
            "Yes, also include tests".into(),
            "Start another Rust task".into(),
        ]
        .into(),
    )));
    controller
        .start_pipeline(
            ListeningMode::Wake,
            owner.clone(),
            None,
            true,
            recognition.clone(),
            model.clone(),
            delivery.clone(),
        )
        .await
        .unwrap();
    let generation = controller.status().await.generation;
    controller
        .push(&owner, generation, 1, audio())
        .await
        .unwrap();
    idle(&controller).await;
    assert!(seen.lock().unwrap().is_empty());
    controller
        .push_woken(&owner, generation, 2, audio())
        .await
        .unwrap();
    idle(&controller).await;
    let conversation = controller.status().await.conversation.unwrap();
    assert_eq!(conversation.target, target);
    controller
        .push(&owner, generation, 3, audio())
        .await
        .unwrap();
    idle(&controller).await;
    assert_eq!(
        controller.status().await.conversation.as_ref(),
        Some(&conversation)
    );
    controller.pause(&owner, generation).await.unwrap();
    assert!(controller.status().await.paused);
    assert_eq!(
        controller.status().await.conversation.as_ref(),
        Some(&conversation)
    );
    assert!(
        controller
            .push(&owner, generation, 4, audio())
            .await
            .is_err()
    );
    controller.resume(&owner, generation).await.unwrap();
    let resumed = controller.status().await.generation;
    assert_ne!(resumed, generation);
    assert!(
        controller
            .push(&owner, generation, 4, audio())
            .await
            .is_err()
    );
    controller.push(&owner, resumed, 1, audio()).await.unwrap();
    idle(&controller).await;
    assert_eq!(
        controller.status().await.conversation.as_ref(),
        Some(&conversation)
    );
    let inputs = seen.lock().unwrap();
    assert_eq!(inputs.len(), 1);
    assert!(inputs[0].routing && !inputs[0].admission);
    assert_eq!(inputs[0].candidates.len(), 1);
    assert!(inputs[0].candidates[0].purpose.contains("Rust"));
    assert_eq!(delivery.followup_calls.load(Ordering::Acquire), 2);
    drop(inputs);
    assert!(
        delivery
            .requests
            .lock()
            .await
            .iter()
            .all(|request| request.target.as_deref() == Some(target.as_str()))
    );
    controller.stop(&owner, resumed).await.unwrap();
    assert!(controller.status().await.conversation.is_none());
    assert!(controller.resume(&owner, resumed).await.is_err());
    let next = nucleus::new_uid("voice");
    controller
        .start_pipeline(
            ListeningMode::Wake,
            next.clone(),
            None,
            true,
            recognition,
            model,
            delivery.clone(),
        )
        .await
        .unwrap();
    let generation = controller.status().await.generation;
    controller
        .push(&next, generation, 1, audio())
        .await
        .unwrap();
    idle(&controller).await;
    assert_eq!(seen.lock().unwrap().len(), 1);
    controller
        .push_woken(&next, generation, 2, audio())
        .await
        .unwrap();
    idle(&controller).await;
    assert_eq!(seen.lock().unwrap().len(), 2);
    assert_ne!(
        controller.status().await.conversation.unwrap().thread,
        conversation.thread
    );
    controller.shutdown().await;
}

#[tokio::test]
async fn unregistered_disabled_and_stale_router_results_fall_back_without_selecting_that_record() {
    let eligible = nucleus::new_uid("r");
    let disabled = nucleus::new_uid("r");
    for (destination, stale) in [
        (Some(nucleus::new_uid("r")), false),
        (Some(disabled.clone()), false),
        (Some(eligible.clone()), true),
        (None, false),
    ] {
        let controller = Controller::default();
        let owner = nucleus::new_uid("voice");
        let model = Arc::new(decision::Engine::from_model(Arc::new(DecisionFixture {
            admission: None,
            validated: false,
            destination,
            wrong_generation: stale,
            seen: None,
        })));
        let delivery = Arc::new(DeliveryFixture {
            candidates: vec![
                ::fiote::assistant::Candidate {
                    record: eligible.clone(),
                    title: "Eligible".into(),
                    description: "Help with the task".into(),
                    description_truncated: false,
                    enabled: true,
                },
                ::fiote::assistant::Candidate {
                    record: disabled.clone(),
                    title: "Disabled".into(),
                    description: "No connection".into(),
                    description_truncated: false,
                    enabled: false,
                },
            ],
            ..Default::default()
        });
        controller
            .start_pipeline(
                ListeningMode::Wake,
                owner.clone(),
                None,
                true,
                recognition(None),
                model,
                delivery.clone(),
            )
            .await
            .unwrap();
        let generation = controller.status().await.generation;
        controller
            .push_woken(&owner, generation, 1, audio())
            .await
            .unwrap();
        idle(&controller).await;
        assert!(delivery.requests.lock().await[0].target.is_none());
        controller.shutdown().await;
    }
}

#[tokio::test]
async fn closing_unrelated_uncertain_and_abstaining_followups_end_without_dispatch_or_replay() {
    for admission in [
        Some(Admission::OrdinaryConversation),
        Some(Admission::Unclear),
        None,
    ] {
        let controller = Controller::default();
        let owner = nucleus::new_uid("voice");
        let delivery = Arc::new(DeliveryFixture::default());
        let recognition = recognition(None);
        controller
            .start_pipeline(
                ListeningMode::Wake,
                owner.clone(),
                None,
                false,
                recognition.clone(),
                decision(admission),
                delivery.clone(),
            )
            .await
            .unwrap();
        let generation = controller.status().await.generation;
        controller
            .push_woken(&owner, generation, 1, audio())
            .await
            .unwrap();
        idle(&controller).await;
        let previous = controller.status().await.conversation.unwrap();
        controller
            .push(&owner, generation, 2, audio())
            .await
            .unwrap();
        idle(&controller).await;
        assert!(controller.status().await.enabled);
        assert!(controller.status().await.conversation.is_none());
        assert!(controller.status().await.unsent.is_none());
        assert_eq!(delivery.requests.lock().await.len(), 1);
        assert_eq!(
            delivery.followup_calls.load(Ordering::Acquire),
            u64::from(admission.is_none())
        );
        controller
            .push(&owner, generation, 3, audio())
            .await
            .unwrap();
        idle(&controller).await;
        assert_eq!(recognition.calls.load(Ordering::Acquire), 2);
        controller
            .push_woken(&owner, generation, 4, audio())
            .await
            .unwrap();
        idle(&controller).await;
        assert_eq!(delivery.requests.lock().await.len(), 2);
        assert_ne!(
            controller.status().await.conversation.unwrap().thread,
            previous.thread
        );
        controller.shutdown().await;
    }
}

#[tokio::test]
async fn an_unusable_local_judgment_escalates_but_explicit_uncertainty_and_closing_speech_do_not() {
    for (admission, stale, accepted) in [
        (None, false, true),
        (Some(Admission::Unclear), false, false),
        (Some(Admission::OrdinaryConversation), false, false),
        (Some(Admission::AssistantRequest), true, false),
    ] {
        let controller = Controller::default();
        let owner = nucleus::new_uid("voice");
        let delivery = Arc::new(DeliveryFixture {
            followup: Some(Admission::AssistantRequest),
            ..Default::default()
        });
        let model = Arc::new(decision::Engine::from_model(Arc::new(DecisionFixture {
            admission,
            validated: true,
            destination: None,
            wrong_generation: stale,
            seen: None,
        })));
        controller
            .start_pipeline(
                ListeningMode::Wake,
                owner.clone(),
                None,
                false,
                recognition(None),
                model,
                delivery.clone(),
            )
            .await
            .unwrap();
        let generation = controller.status().await.generation;
        controller
            .push_woken(&owner, generation, 1, audio())
            .await
            .unwrap();
        idle(&controller).await;
        let conversation = controller.status().await.conversation.unwrap();
        controller
            .push(&owner, generation, 2, audio())
            .await
            .unwrap();
        idle(&controller).await;
        assert_eq!(
            delivery.followup_calls.load(Ordering::Acquire),
            u64::from(accepted)
        );
        assert_eq!(
            delivery.requests.lock().await.len(),
            if accepted { 2 } else { 1 }
        );
        assert_eq!(
            controller.status().await.conversation.as_ref(),
            accepted.then_some(&conversation)
        );
        controller.shutdown().await;
    }
}

#[tokio::test]
async fn unavailable_local_models_use_tool_free_delivery_classification_and_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    for admission in [
        Some(Admission::AssistantRequest),
        Some(Admission::OrdinaryConversation),
        Some(Admission::Unclear),
        None,
    ] {
        let controller = Controller::default();
        let owner = nucleus::new_uid("voice");
        let delivery = Arc::new(DeliveryFixture {
            followup: admission,
            ..Default::default()
        });
        controller
            .start_pipeline(
                ListeningMode::Wake,
                owner.clone(),
                None,
                false,
                recognition(None),
                Arc::new(decision::Engine::local(root.path().join("absent"))),
                delivery.clone(),
            )
            .await
            .unwrap();
        let generation = controller.status().await.generation;
        controller
            .push_woken(&owner, generation, 1, audio())
            .await
            .unwrap();
        idle(&controller).await;
        let conversation = controller.status().await.conversation.unwrap();
        controller
            .push(&owner, generation, 2, audio())
            .await
            .unwrap();
        idle(&controller).await;
        assert_eq!(delivery.followup_calls.load(Ordering::Acquire), 1);
        if admission == Some(Admission::AssistantRequest) {
            assert_eq!(
                controller.status().await.conversation.as_ref(),
                Some(&conversation)
            );
            assert_eq!(delivery.requests.lock().await.len(), 2);
        } else {
            assert!(controller.status().await.conversation.is_none());
            assert_eq!(delivery.requests.lock().await.len(), 1);
        }
        controller.shutdown().await;
    }
}

#[tokio::test]
async fn idle_expiry_rearms_wake_but_working_and_speaking_conversations_are_retained() {
    for mode in ["idle", "working", "speaking"] {
        let controller = Controller::default();
        let owner = nucleus::new_uid("voice");
        let delivery = Arc::new(DeliveryFixture {
            inspected: (mode == "working").then(|| EntryReceipt {
                request_id: String::new(),
                fingerprint: "fixture".into(),
                target: nucleus::new_uid("r"),
                thread: None,
                message: None,
                reply: None,
                state: "running".into(),
                detail: String::new(),
            }),
            ..Default::default()
        });
        controller
            .start_pipeline(
                ListeningMode::Wake,
                owner.clone(),
                None,
                false,
                recognition(None),
                decision(Some(Admission::AssistantRequest)),
                delivery.clone(),
            )
            .await
            .unwrap();
        let generation = controller.status().await.generation;
        controller
            .push_woken(&owner, generation, 1, audio())
            .await
            .unwrap();
        idle(&controller).await;
        let conversation = controller.status().await.conversation.unwrap();
        if mode == "speaking" {
            controller.pause(&owner, generation).await.unwrap();
        }
        controller.state.lock().await.last_activity =
            Some(Instant::now() - Duration::from_secs(91));
        let status = controller.status().await;
        if mode == "idle" {
            assert!(status.conversation.is_none());
            controller
                .push(&owner, generation, 2, audio())
                .await
                .unwrap();
            idle(&controller).await;
            assert_eq!(delivery.requests.lock().await.len(), 1);
        } else {
            assert_eq!(status.conversation.as_ref(), Some(&conversation));
        }
        controller.shutdown().await;
        assert!(controller.status().await.conversation.is_none());
    }
}

#[tokio::test]
async fn stopping_during_followup_classification_discards_it_and_any_pending_speech() {
    let root = tempfile::tempdir().unwrap();
    let controller = Controller::default();
    let owner = nucleus::new_uid("voice");
    let delivery = Arc::new(DeliveryFixture {
        followup: Some(Admission::AssistantRequest),
        followup_permits: Some(Arc::new(tokio::sync::Semaphore::new(0))),
        ..Default::default()
    });
    controller
        .start_pipeline(
            ListeningMode::Wake,
            owner.clone(),
            None,
            false,
            recognition(None),
            Arc::new(decision::Engine::local(root.path().join("absent"))),
            delivery.clone(),
        )
        .await
        .unwrap();
    let generation = controller.status().await.generation;
    controller
        .push_woken(&owner, generation, 1, audio())
        .await
        .unwrap();
    idle(&controller).await;
    controller
        .push(&owner, generation, 2, audio())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while delivery.followup_calls.load(Ordering::Acquire) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    controller
        .push(&owner, generation, 3, audio())
        .await
        .unwrap();
    controller.stop(&owner, generation).await.unwrap();
    idle(&controller).await;
    assert_eq!(delivery.requests.lock().await.len(), 1);
    assert!(controller.status().await.conversation.is_none());
    assert!(!controller.status().await.pending);
}

#[tokio::test]
async fn acoustic_requests_cannot_cross_stopped_generations_or_addressed_sessions() {
    let root = tempfile::tempdir().unwrap();
    let controller = Controller::default();
    let owner = nucleus::new_uid("voice");
    let delivery = Arc::new(DeliveryFixture::default());
    let recognition = Arc::new(QueuedRecognition(std::sync::Mutex::new(
        ["One authorized request"].map(String::from).into(),
    )));
    let model = Arc::new(decision::Engine::local(root.path().join("unused")));
    controller
        .start_pipeline(
            ListeningMode::Wake,
            owner.clone(),
            None,
            false,
            recognition.clone(),
            model.clone(),
            delivery.clone(),
        )
        .await
        .unwrap();
    let generation = controller.status().await.generation;
    controller
        .push(&owner, generation, 1, audio())
        .await
        .unwrap();
    idle(&controller).await;
    assert_eq!(recognition.0.lock().unwrap().len(), 1);
    controller
        .push_woken(&owner, generation, 2, audio())
        .await
        .unwrap();
    idle(&controller).await;
    controller.stop(&owner, generation).await.unwrap();
    assert!(
        controller
            .push_woken(&owner, generation, 3, audio())
            .await
            .is_err()
    );
    let next = nucleus::new_uid("voice");
    controller
        .start_pipeline(
            ListeningMode::Addressed,
            next.clone(),
            None,
            false,
            recognition,
            model,
            delivery.clone(),
        )
        .await
        .unwrap();
    let next_generation = controller.status().await.generation;
    assert!(
        controller
            .push_woken(&owner, generation, 3, audio())
            .await
            .is_err()
    );
    assert!(
        controller
            .push_woken(&next, next_generation, 1, audio())
            .await
            .unwrap_err()
            .contains("wake listening")
    );
    assert_eq!(delivery.requests.lock().await.len(), 1);
    controller.shutdown().await;
}

#[tokio::test]
async fn addressed_mode_uses_chosen_names_preserves_text_and_needs_no_decision_assets() {
    let root = tempfile::tempdir().unwrap();
    let record = nucleus::new_uid("r");
    for (text, accepted) in [
        ("Hey Fiote, do not remove /tmp/notes-7.csv", true),
        ("Hi Tidal, please help", true),
        ("Tidal, please help", true),
        ("Tidal is my assistant", false),
        ("Hey someone else, help", false),
        ("\"Hey Fiote, remove it\"", false),
        ("Someone said hey Fiote", false),
    ] {
        let controller = Controller::default();
        let owner = nucleus::new_uid("voice");
        let model = Arc::new(decision::Engine::local(root.path().join("no-models")));
        let delivery = Arc::new(DeliveryFixture {
            candidates: vec![::fiote::assistant::Candidate {
                record: record.clone(),
                title: "Tidal".into(),
                description: "Chosen Fiote".into(),
                description_truncated: false,
                enabled: true,
            }],
            ..Default::default()
        });
        let recognition = Arc::new(RecognitionFixture {
            present: true,
            text: text.into(),
            calls: AtomicU64::new(0),
            permits: None,
        });
        controller
            .start_pipeline(
                ListeningMode::Addressed,
                owner.clone(),
                Some(record.clone()),
                true,
                recognition,
                model.clone(),
                delivery.clone(),
            )
            .await
            .unwrap();
        let generation = controller.status().await.generation;
        controller
            .push(&owner, generation, 1, audio())
            .await
            .unwrap();
        idle(&controller).await;
        let requests = delivery.requests.lock().await;
        assert_eq!(requests.len(), usize::from(accepted), "{text}");
        if accepted {
            assert_eq!(requests[0].text, text);
            assert_eq!(requests[0].target.as_ref(), Some(&record));
        }
        assert!(!model.loaded());
        assert!(!root.path().join("no-models").exists());
        controller.shutdown().await;
    }
}

#[tokio::test]
async fn addressed_requests_fall_back_once_when_optional_routing_returns_stale_identity() {
    let controller = Controller::default();
    let owner = nucleus::new_uid("voice");
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let target = nucleus::new_uid("r");
    let delivery = Arc::new(DeliveryFixture {
        candidates: vec![::fiote::assistant::Candidate {
            record: target.clone(),
            title: "Coding".into(),
            description: "Code work".into(),
            description_truncated: false,
            enabled: true,
        }],
        ..Default::default()
    });
    let model = Arc::new(decision::Engine::from_model(Arc::new(DecisionFixture {
        admission: None,
        validated: false,
        destination: Some(target),
        wrong_generation: true,
        seen: Some(seen.clone()),
    })));
    controller
        .start_pipeline(
            ListeningMode::Addressed,
            owner.clone(),
            None,
            true,
            recognition(None),
            model,
            delivery.clone(),
        )
        .await
        .unwrap();
    let generation = controller.status().await.generation;
    controller
        .push(&owner, generation, 1, audio())
        .await
        .unwrap();
    idle(&controller).await;
    let requests = delivery.requests.lock().await;
    assert_eq!(requests.len(), 1);
    assert!(requests[0].target.is_none());
    assert_eq!(requests[0].text, "Assistant, please handle my request");
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert!(!seen[0].admission);
    assert!(seen[0].routing);
}

fn decision(admission: Option<Admission>) -> Arc<decision::Engine> {
    Arc::new(decision::Engine::from_model(Arc::new(DecisionFixture {
        admission,
        validated: true,
        destination: None,
        wrong_generation: false,
        seen: None,
    })))
}

async fn idle(controller: &Controller) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if controller.state.lock().await.running.is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn unavailable_or_unvalidated_admission_never_starts_recognition_or_delivery() {
    let controller = Controller::default();
    let owner = nucleus::new_uid("voice");
    let recognition = recognition(None);
    let delivery = Arc::new(DeliveryFixture::default());
    let unvalidated = Arc::new(decision::Engine::from_model(Arc::new(DecisionFixture {
        admission: Some(Admission::AssistantRequest),
        validated: false,
        destination: None,
        wrong_generation: false,
        seen: None,
    })));
    assert!(
        controller
            .start_pipeline(
                ListeningMode::Model,
                owner,
                None,
                false,
                recognition.clone(),
                unvalidated,
                delivery.clone()
            )
            .await
            .is_err()
    );
    assert!(!controller.status().await.enabled);
    assert_eq!(recognition.calls.load(Ordering::Acquire), 0);
    assert!(delivery.requests.lock().await.is_empty());
}

#[tokio::test]
async fn closing_an_owner_before_start_rejects_its_late_activation_without_stopping_another_owner()
{
    let controller = Controller::default();
    let owner = nucleus::new_uid("voice");
    controller.stop(&owner, 0).await.unwrap();
    let delivery = Arc::new(DeliveryFixture::default());
    assert!(
        controller
            .start_pipeline(
                ListeningMode::Model,
                owner.clone(),
                None,
                false,
                recognition(None),
                decision(Some(Admission::AssistantRequest)),
                delivery.clone()
            )
            .await
            .is_err()
    );
    let other = nucleus::new_uid("voice");
    controller
        .start_pipeline(
            ListeningMode::Model,
            other.clone(),
            None,
            false,
            recognition(None),
            decision(Some(Admission::AssistantRequest)),
            delivery,
        )
        .await
        .unwrap();
    controller.stop(&owner, 0).await.unwrap();
    assert!(controller.status().await.enabled);
    assert_eq!(
        controller.status().await.owner.as_deref(),
        Some(other.as_str())
    );
}

#[tokio::test]
async fn ordinary_unclear_and_abstaining_speech_create_no_delivery() {
    for admission in [
        Some(Admission::OrdinaryConversation),
        Some(Admission::Unclear),
        None,
    ] {
        let controller = Controller::default();
        let owner = nucleus::new_uid("voice");
        let delivery = Arc::new(DeliveryFixture::default());
        controller
            .start_pipeline(
                ListeningMode::Model,
                owner.clone(),
                None,
                false,
                recognition(None),
                decision(admission),
                delivery.clone(),
            )
            .await
            .unwrap();
        let generation = controller.status().await.generation;
        controller
            .push(&owner, generation, 1, audio())
            .await
            .unwrap();
        idle(&controller).await;
        assert_eq!(controller.status().await.stage, ListeningStage::Ignored);
        assert!(controller.status().await.unsent.is_none());
        assert!(delivery.requests.lock().await.is_empty());
    }
}

#[tokio::test]
async fn admitted_requests_use_one_default_delivery_or_the_explicit_recipient() {
    for target in [None, Some(nucleus::new_uid("r"))] {
        let controller = Controller::default();
        let owner = nucleus::new_uid("voice");
        let delivery = Arc::new(DeliveryFixture::default());
        controller
            .start_pipeline(
                ListeningMode::Model,
                owner.clone(),
                target.clone(),
                false,
                recognition(None),
                decision(Some(Admission::AssistantRequest)),
                delivery.clone(),
            )
            .await
            .unwrap();
        let generation = controller.status().await.generation;
        controller
            .push(&owner, generation, 1, audio())
            .await
            .unwrap();
        idle(&controller).await;
        assert!(
            controller
                .push(&owner, generation, 1, audio())
                .await
                .is_err()
        );
        let requests = delivery.requests.lock().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].target, target);
        assert_eq!(controller.status().await.stage, ListeningStage::Working);
    }
}

#[tokio::test]
async fn stop_invalidates_active_and_pending_segments_without_stopping_a_delivered_run() {
    let controller = Controller::default();
    let owner = nucleus::new_uid("voice");
    let permits = Arc::new(tokio::sync::Semaphore::new(0));
    let recognition = recognition(Some(permits.clone()));
    let delivery = Arc::new(DeliveryFixture::default());
    controller
        .start_pipeline(
            ListeningMode::Model,
            owner.clone(),
            None,
            false,
            recognition.clone(),
            decision(Some(Admission::AssistantRequest)),
            delivery.clone(),
        )
        .await
        .unwrap();
    let generation = controller.status().await.generation;
    controller
        .push(&owner, generation, 1, audio())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while recognition.calls.load(Ordering::Acquire) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    controller
        .push(&owner, generation, 2, audio())
        .await
        .unwrap();
    assert!(controller.status().await.pending);
    assert!(
        controller
            .push(&owner, generation, 3, audio())
            .await
            .is_err()
    );
    assert!(
        controller
            .stop(&nucleus::new_uid("voice"), generation)
            .await
            .is_err()
    );
    controller.stop(&owner, generation).await.unwrap();
    assert!(!controller.status().await.pending);
    assert!(
        controller
            .push(&owner, generation, 4, audio())
            .await
            .is_err()
    );
    permits.add_permits(1);
    idle(&controller).await;
    assert_eq!(recognition.calls.load(Ordering::Acquire), 1);
    assert!(delivery.requests.lock().await.is_empty());
    assert!(!controller.active());
    controller
        .start_pipeline(
            ListeningMode::Model,
            owner.clone(),
            None,
            false,
            self::recognition(None),
            decision(Some(Admission::AssistantRequest)),
            delivery.clone(),
        )
        .await
        .unwrap();
    let generation = controller.status().await.generation;
    controller
        .push(&owner, generation, 1, audio())
        .await
        .unwrap();
    idle(&controller).await;
    controller.stop(&owner, generation).await.unwrap();
    assert_eq!(delivery.requests.lock().await.len(), 1);
    assert!(controller.status().await.receipt.is_some());
}

#[tokio::test]
async fn shutdown_drains_current_speech_discards_pending_segments_and_cannot_reactivate() {
    let controller = Arc::new(Controller::default());
    let owner = nucleus::new_uid("voice");
    let permits = Arc::new(tokio::sync::Semaphore::new(0));
    let recognition = recognition(Some(permits.clone()));
    let delivery = Arc::new(DeliveryFixture::default());
    controller
        .start_pipeline(
            ListeningMode::Model,
            owner.clone(),
            None,
            false,
            recognition.clone(),
            decision(Some(Admission::AssistantRequest)),
            delivery.clone(),
        )
        .await
        .unwrap();
    let generation = controller.status().await.generation;
    controller
        .push(&owner, generation, 1, audio())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while recognition.calls.load(Ordering::Acquire) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    controller
        .push(&owner, generation, 2, audio())
        .await
        .unwrap();
    let shutdown = {
        let controller = controller.clone();
        tokio::spawn(async move { controller.shutdown().await })
    };
    tokio::time::timeout(Duration::from_secs(2), async {
        while controller.status().await.enabled {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!controller.status().await.pending);
    assert!(controller.active());
    assert!(!shutdown.is_finished());
    assert!(
        controller
            .start_pipeline(
                ListeningMode::Model,
                nucleus::new_uid("voice"),
                None,
                false,
                recognition.clone(),
                decision(Some(Admission::AssistantRequest)),
                delivery.clone()
            )
            .await
            .unwrap_err()
            .contains("shutting down")
    );
    let second = {
        let controller = controller.clone();
        tokio::spawn(async move { controller.shutdown().await })
    };
    tokio::task::yield_now().await;
    assert!(!second.is_finished());
    permits.add_permits(1);
    tokio::time::timeout(Duration::from_secs(2), shutdown)
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), second)
        .await
        .unwrap()
        .unwrap();
    assert!(!controller.active());
    assert_eq!(recognition.calls.load(Ordering::Acquire), 1);
    assert!(delivery.requests.lock().await.is_empty());
    assert!(
        controller
            .push(&owner, generation, 3, audio())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn wrong_decision_generation_and_unavailable_delivery_do_not_lose_authorized_requests() {
    let controller = Controller::default();
    let owner = nucleus::new_uid("voice");
    let delivery = Arc::new(DeliveryFixture {
        error: Some("Configure the chosen Fiote".into()),
        ..Default::default()
    });
    let model = Arc::new(decision::Engine::from_model(Arc::new(DecisionFixture {
        admission: Some(Admission::AssistantRequest),
        validated: true,
        destination: None,
        wrong_generation: true,
        seen: None,
    })));
    controller
        .start_pipeline(
            ListeningMode::Model,
            owner.clone(),
            None,
            false,
            recognition(None),
            model,
            delivery.clone(),
        )
        .await
        .unwrap();
    let generation = controller.status().await.generation;
    controller
        .push(&owner, generation, 1, audio())
        .await
        .unwrap();
    idle(&controller).await;
    assert!(delivery.requests.lock().await.is_empty());
    assert_eq!(controller.status().await.stage, ListeningStage::Degraded);
    controller.stop(&owner, generation).await.unwrap();
    controller
        .start_pipeline(
            ListeningMode::Model,
            owner.clone(),
            None,
            false,
            recognition(None),
            decision(Some(Admission::AssistantRequest)),
            delivery.clone(),
        )
        .await
        .unwrap();
    let generation = controller.status().await.generation;
    controller
        .push(&owner, generation, 1, audio())
        .await
        .unwrap();
    idle(&controller).await;
    assert_eq!(
        controller
            .status()
            .await
            .unsent
            .as_ref()
            .map(|unsent| unsent.request.text.as_str()),
        Some("Assistant, please handle my request")
    );
    let unsent = controller.status().await.unsent.unwrap();
    assert_eq!(unsent.owner, owner);
    assert_eq!(unsent.generation, generation);
    assert_eq!(unsent.segment, 1);
    assert!(!unsent.uncertain);
    assert!(!controller.status().await.enabled);
    assert_eq!(
        unsent.request.request_id,
        delivery.requests.lock().await[0].request_id
    );
    assert_eq!(delivery.requests.lock().await.len(), 1);
}

#[tokio::test]
async fn lost_delivery_acknowledgment_inspects_once_and_preserves_uncertainty_without_redispatch() {
    for committed in [false, true] {
        let controller = Controller::default();
        let owner = nucleus::new_uid("voice");
        let delivery = Arc::new(DeliveryFixture {
            error: Some("Delivery acknowledgment lost".into()),
            inspection_error: !committed,
            inspected: committed.then(|| EntryReceipt {
                request_id: String::new(),
                fingerprint: "committed".into(),
                target: nucleus::new_uid("r"),
                thread: Some(nucleus::new_uid("r")),
                message: Some(nucleus::new_uid("r")),
                reply: Some(nucleus::new_uid("r")),
                state: "completed".into(),
                detail: String::new(),
            }),
            ..Default::default()
        });
        controller
            .start_pipeline(
                ListeningMode::Model,
                owner.clone(),
                None,
                false,
                recognition(None),
                decision(Some(Admission::AssistantRequest)),
                delivery.clone(),
            )
            .await
            .unwrap();
        let generation = controller.status().await.generation;
        controller
            .push(&owner, generation, 1, audio())
            .await
            .unwrap();
        idle(&controller).await;
        let status = controller.status().await;
        assert_eq!(delivery.requests.lock().await.len(), 1);
        if committed {
            assert!(status.unsent.is_none());
            assert!(status.receipt.unwrap().message.is_some());
        } else {
            assert!(!status.enabled);
            assert!(status.unsent.unwrap().uncertain);
            assert!(status.receipt.is_none());
        }
        controller.stop(&owner, generation).await.unwrap();
    }
}

#[tokio::test]
async fn known_names_are_separate_context_and_never_rewrite_the_delivered_request() {
    let controller = Controller::default();
    let owner = nucleus::new_uid("voice");
    let target = nucleus::new_uid("r");
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let original = "Tidal, do not remove /tmp/notes-2042.txt.";
    let recognition = Arc::new(RecognitionFixture {
        present: true,
        text: original.into(),
        calls: AtomicU64::new(0),
        permits: None,
    });
    let delivery = Arc::new(DeliveryFixture {
        candidates: vec![::fiote::assistant::Candidate {
            record: nucleus::new_uid("r"),
            title: "Tidal".into(),
            description: "Authorized local Fiote".into(),
            description_truncated: false,
            enabled: true,
        }],
        ..Default::default()
    });
    let model = Arc::new(decision::Engine::from_model(Arc::new(DecisionFixture {
        admission: Some(Admission::AssistantRequest),
        validated: true,
        destination: None,
        wrong_generation: false,
        seen: Some(seen.clone()),
    })));
    controller
        .start_pipeline(
            ListeningMode::Model,
            owner.clone(),
            Some(target.clone()),
            false,
            recognition,
            model,
            delivery.clone(),
        )
        .await
        .unwrap();
    let generation = controller.status().await.generation;
    controller
        .push(&owner, generation, 1, audio())
        .await
        .unwrap();
    idle(&controller).await;
    let input = seen.lock().unwrap()[0].clone();
    let context: decision::Context = serde_json::from_str(&input.context).unwrap();
    assert!(context.assistant_names.iter().any(|name| name == "Tidal"));
    assert_eq!(
        input.admission_text().unwrap(),
        "AI assistant, do not remove /tmp/notes-2042.txt."
    );
    let requests = delivery.requests.lock().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].text, original);
    assert_eq!(requests[0].target.as_deref(), Some(target.as_str()));
    drop(requests);
    controller.stop(&owner, generation).await.unwrap();
}

struct ProviderFixture {
    calls: AtomicU64,
}

#[async_trait::async_trait]
impl Provider for ProviderFixture {
    async fn complete(
        &self,
        _: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<Reply, String> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        if let Some(Message::User(text) | Message::RichUser { text, .. }) = messages.last()
            && text == "Create Hello.txt with world"
        {
            assert!(tools.iter().any(|tool| tool.name == "create_file"));
            return Ok(Reply {
                calls: vec![::fiote::provider::ToolCall {
                    id: "wake-file-action".into(),
                    name: "create_file".into(),
                    arguments: serde_json::json!({"path":"Hello.txt","content":"world"}),
                    signatures: None,
                }],
                ..Default::default()
            });
        }
        if let Some(Message::Tool { name, result, .. }) = messages.last() {
            assert_eq!(name, "create_file");
            assert_eq!(result["ok"], true, "{result}");
        }
        Ok(Reply {
            text: "Hello from the chosen intake Fiote".into(),
            ..Default::default()
        })
    }
}

#[tokio::test]
async fn listening_pipeline_uses_normal_fiote_delivery_and_ignored_speech_creates_no_messages_or_provider_calls()
 {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let root = tempfile::tempdir().unwrap();
    let record = engine
        .act(
            engine::actions::Action::CreateAgent {
                head: "Listening intake".into(),
                operated_by: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let provider = Arc::new(ProviderFixture {
        calls: AtomicU64::new(0),
    });
    let host = Arc::new(
        crate::fiote::Host::open(engine.clone(), root.path().join("fiote"))
            .await
            .unwrap()
            .with_provider(provider.clone()),
    );
    let status = host
        .handle(::fiote::config::Request::Inspect {
            record: record.clone(),
        })
        .await
        .unwrap();
    let selected = status
        .providers
        .iter()
        .find(|provider| provider.auth_methods[0].kind == ::fiote::adapters::AuthKind::ApiKey)
        .unwrap();
    host.handle(::fiote::config::Request::Configure {
        record: record.clone(),
        settings: ::fiote::config::Settings {
            enabled: true,
            provider: selected.id.clone(),
            model: "fixture".into(),
            directory: root.path().into(),
            ..Default::default()
        },
        api_key: Some(::fiote::config::Secret("fixture-key".into())),
        password: Some(::fiote::config::Secret("fixture-password".into())),
    })
    .await
    .unwrap();
    let controller = Controller::default();
    let owner = nucleus::new_uid("voice");
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    controller
        .start_pipeline(
            ListeningMode::Model,
            owner.clone(),
            Some(record.clone()),
            false,
            recognition(None),
            decision(Some(Admission::OrdinaryConversation)),
            host.clone(),
        )
        .await
        .unwrap();
    let generation = controller.status().await.generation;
    controller
        .push(&owner, generation, 1, audio())
        .await
        .unwrap();
    idle(&controller).await;
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(provider.calls.load(Ordering::Acquire), 0);
    controller.stop(&owner, generation).await.unwrap();
    controller
        .start_pipeline(
            ListeningMode::Model,
            owner.clone(),
            Some(record.clone()),
            false,
            recognition(None),
            decision(Some(Admission::AssistantRequest)),
            host.clone(),
        )
        .await
        .unwrap();
    let generation = controller.status().await.generation;
    controller
        .push(&owner, generation, 1, audio())
        .await
        .unwrap();
    idle(&controller).await;
    let receipt = controller.status().await.receipt.unwrap();
    assert_eq!(receipt.target, record);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let inspected = host
                .handle(::fiote::config::Request::AssistantInspect {
                    request_id: receipt.request_id.clone(),
                })
                .await
                .unwrap()
                .assistant_request
                .unwrap();
            if inspected.state == "completed" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(provider.calls.load(Ordering::Acquire), 1);
    assert_eq!(
        store::records::get(&engine.store.pool, receipt.reply.as_ref().unwrap())
            .await
            .unwrap()
            .unwrap()
            .body,
        "Hello from the chosen intake Fiote"
    );
    controller.stop(&owner, generation).await.unwrap();
    let missing_model = Arc::new(decision::Engine::local(
        root.path().join("missing-decision"),
    ));
    let spoken = "Hey Listening intake, do not remove /tmp/notes-7.csv";
    controller
        .start_pipeline(
            ListeningMode::Addressed,
            owner.clone(),
            Some(record.clone()),
            true,
            Arc::new(RecognitionFixture {
                present: true,
                text: spoken.into(),
                calls: AtomicU64::new(0),
                permits: None,
            }),
            missing_model.clone(),
            host.clone(),
        )
        .await
        .unwrap();
    let generation = controller.status().await.generation;
    controller
        .push(&owner, generation, 1, audio())
        .await
        .unwrap();
    idle(&controller).await;
    let addressed = controller.status().await.receipt.unwrap();
    assert_eq!(addressed.target, record);
    assert_eq!(
        store::records::get(&engine.store.pool, addressed.message.as_ref().unwrap())
            .await
            .unwrap()
            .unwrap()
            .body,
        spoken
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while provider.calls.load(Ordering::Acquire) != 2 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    assert!(!missing_model.loaded());
    assert!(!root.path().join("missing-decision").exists());
    controller.stop(&owner, generation).await.unwrap();
    controller
        .start_pipeline(
            ListeningMode::Wake,
            owner.clone(),
            Some(record.clone()),
            true,
            Arc::new(RecognitionFixture {
                present: true,
                text: "Create Hello.txt with world".into(),
                calls: AtomicU64::new(0),
                permits: None,
            }),
            missing_model.clone(),
            host.clone(),
        )
        .await
        .unwrap();
    let generation = controller.status().await.generation;
    controller
        .push_woken(&owner, generation, 1, audio())
        .await
        .unwrap();
    idle(&controller).await;
    let receipt = controller.status().await.receipt.unwrap();
    assert_eq!(receipt.target, record);
    let completed = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let inspected = host
                .handle(::fiote::config::Request::AssistantInspect {
                    request_id: receipt.request_id.clone(),
                })
                .await
                .unwrap();
            if inspected.assistant_request.as_ref().unwrap().state == "completed" {
                break inspected;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(root.path().join("Hello.txt")).unwrap(),
        "world"
    );
    assert!(completed.assistant_reply.is_some());
    let stored = store::records::get(&engine.store.pool, receipt.message.as_ref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.body, "Create Hello.txt with world");
    assert!(
        controller
            .push_woken(&owner, generation, 1, audio())
            .await
            .is_err()
    );
    assert_eq!(provider.calls.load(Ordering::Acquire), 4);
    assert!(!missing_model.loaded());
    host.stop_all().await;
    controller.shutdown().await;
}
