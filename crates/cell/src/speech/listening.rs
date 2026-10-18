use super::*;
use ::fiote::{
    assistant::{Conversation, Directory, EntryReceipt, EntryRequest},
    communication::decision::{Admission, Candidate, Information, Input},
};
use transport::fiote::Service as _;

mod addressed;
mod followup;

#[async_trait::async_trait]
trait Recognition: Send + Sync {
    fn present(&self) -> bool;
    async fn transcribe(
        &self,
        audio: ::fiote::config::Secret,
        cancelled: Arc<AtomicBool>,
    ) -> Result<String, String>;
}

#[async_trait::async_trait]
impl Recognition for LocalRecognition {
    fn present(&self) -> bool {
        self.0.cache.present()
    }
    async fn transcribe(
        &self,
        audio: ::fiote::config::Secret,
        cancelled: Arc<AtomicBool>,
    ) -> Result<String, String> {
        self.0
            .submit(audio, cancelled)?
            .await
            .map_err(|error| error.to_string())?
    }
}

struct LocalRecognition(Arc<speech::local::Recognizer>);

#[async_trait::async_trait]
trait Delivery: Send + Sync {
    async fn context(&self, previous: &EntryReceipt) -> Result<decision::Dialogue, String> {
        let _ = previous;
        Err("Conversation context is unavailable.".into())
    }
    async fn classify_followup(
        &self,
        conversation: &Conversation,
        dialogue: &decision::Dialogue,
        text: &str,
    ) -> Result<Admission, String> {
        let _ = (conversation, dialogue, text);
        Err("A voice continuation classifier is unavailable.".into())
    }
    async fn directory(&self) -> Result<Directory, String>;
    async fn title(&self, record: &str) -> Result<Option<String>, String> {
        Ok(self
            .directory()
            .await?
            .fiotes
            .into_iter()
            .find(|candidate| candidate.record == record)
            .map(|candidate| candidate.title))
    }
    async fn send(&self, request: EntryRequest) -> Result<EntryReceipt, String>;
    async fn continue_conversation(
        &self,
        conversation: Conversation,
        request: EntryRequest,
    ) -> Result<EntryReceipt, String> {
        let _ = (conversation, request);
        Err("This delivery connection cannot continue a Fiote conversation.".into())
    }
    async fn inspect(&self, request_id: &str) -> Result<Option<EntryReceipt>, String>;
}

#[async_trait::async_trait]
impl Delivery for crate::fiote::Host {
    async fn context(&self, previous: &EntryReceipt) -> Result<decision::Dialogue, String> {
        self.voice_context(previous).await
    }
    async fn classify_followup(
        &self,
        conversation: &Conversation,
        dialogue: &decision::Dialogue,
        text: &str,
    ) -> Result<Admission, String> {
        self.classify_voice_followup(conversation, dialogue, text)
            .await
    }
    async fn continue_conversation(
        &self,
        conversation: Conversation,
        request: EntryRequest,
    ) -> Result<EntryReceipt, String> {
        self.handle(::fiote::config::Request::AssistantContinue {
            conversation,
            request,
        })
        .await?
        .assistant_request
        .ok_or("The Fiote did not return a continuation receipt.".into())
    }
    async fn title(&self, record: &str) -> Result<Option<String>, String> {
        self.addressed_title(record).await
    }
    async fn directory(&self) -> Result<Directory, String> {
        self.discover(0, 30).await
    }
    async fn send(&self, request: EntryRequest) -> Result<EntryReceipt, String> {
        self.handle(::fiote::config::Request::AssistantSend { request })
            .await?
            .assistant_request
            .ok_or("The Fiote did not return an assistant receipt.".into())
    }
    async fn inspect(&self, request_id: &str) -> Result<Option<EntryReceipt>, String> {
        self.inspect_assistant_entry(request_id).await
    }
}

#[derive(Clone)]
struct Pipeline {
    mode: ListeningMode,
    recognition: Arc<dyn Recognition>,
    decision: Arc<decision::Engine>,
    delivery: Arc<dyn Delivery>,
    information: Information,
    routing: bool,
    target: Option<String>,
}

struct Segment {
    owner: String,
    generation: u64,
    id: u64,
    audio: ::fiote::config::Secret,
    woken: bool,
}

struct Running {
    owner: String,
    generation: u64,
    id: u64,
    cancelled: Arc<AtomicBool>,
    _task: tokio::task::JoinHandle<()>,
}

#[derive(Default)]
struct State {
    status: ListeningStatus,
    pipeline: Option<Pipeline>,
    running: Option<Running>,
    pending: Option<Segment>,
    highest: u64,
    closed: VecDeque<String>,
    shutting_down: bool,
    last_activity: Option<Instant>,
}

#[derive(Default)]
pub(super) struct Controller {
    state: Arc<Mutex<State>>,
    active: Arc<AtomicBool>,
}

impl Drop for Controller {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.try_lock() {
            state.status.enabled = false;
            state.status.paused = false;
            state.status.conversation = None;
            state.pending = None;
            if let Some(running) = &state.running {
                running.cancelled.store(true, Ordering::Release);
            }
        }
    }
}

impl Controller {
    pub(super) async fn shutdown(&self) {
        {
            let mut state = self.state.lock().await;
            state.shutting_down = true;
            state.status.enabled = false;
            state.status.paused = false;
            state.status.conversation = None;
            state.last_activity = None;
            state.status.generation = state.status.generation.saturating_add(1);
            state.status.stage = ListeningStage::Off;
            state.status.detail = "Listening stopped for application shutdown.".into();
            state.status.unsent = None;
            state.pending = None;
            state.pipeline = None;
            if let Some(running) = &state.running {
                running.cancelled.store(true, Ordering::Release);
            }
        }
        loop {
            let finished = {
                let mut state = self.state.lock().await;
                if state
                    .running
                    .as_ref()
                    .is_some_and(|running| running._task.is_finished())
                {
                    state.running = None;
                }
                state.running.is_none()
            };
            if finished {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        self.active.store(false, Ordering::Release);
    }

    pub(super) fn active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    pub(super) async fn status(&self) -> ListeningStatus {
        self.expire_conversation().await;
        let state = self.state.lock().await;
        let mut status = state.status.clone();
        status.pending = state.pending.is_some();
        status.finishing = !state.status.enabled && state.running.is_some();
        status
    }

    async fn expire_conversation(&self) {
        let check = {
            let state = self.state.lock().await;
            if !state.status.enabled
                || state.running.is_some()
                || state.status.conversation.is_none()
                || state
                    .last_activity
                    .is_none_or(|last| last.elapsed() < Duration::from_secs(90))
            {
                return;
            }
            (
                state.status.owner.clone(),
                state.status.generation,
                state.status.conversation.clone(),
                state.status.receipt.clone(),
                state.pipeline.clone(),
                state.last_activity,
            )
        };
        let busy = if let (Some(receipt), Some(pipeline)) = (&check.3, &check.4) {
            pipeline
                .delivery
                .inspect(&receipt.request_id)
                .await
                .ok()
                .flatten()
                .is_some_and(|receipt| matches!(receipt.state.as_str(), "running" | "accepted"))
        } else {
            false
        };
        let mut state = self.state.lock().await;
        if state.status.enabled
            && state.running.is_none()
            && state.status.owner == check.0
            && state.status.generation == check.1
            && state.status.conversation == check.2
            && state.last_activity == check.5
        {
            if busy {
                state.last_activity = Some(Instant::now());
            } else {
                followup::end(
                    &mut state,
                    "Voice conversation ended after ninety seconds of inactivity. Waiting for a new activation.",
                );
            }
        }
    }

    pub(super) async fn start(
        &self,
        mode: ListeningMode,
        owner: String,
        target: Option<String>,
        routing: bool,
        recognition: Arc<speech::local::Recognizer>,
        decision: Arc<decision::Engine>,
        delivery: Arc<crate::fiote::Host>,
    ) -> Result<(), String> {
        self.start_pipeline(
            mode,
            owner,
            target,
            routing,
            Arc::new(LocalRecognition(recognition)),
            decision,
            delivery,
        )
        .await
    }

    async fn start_pipeline(
        &self,
        mode: ListeningMode,
        owner: String,
        target: Option<String>,
        routing: bool,
        recognition: Arc<dyn Recognition>,
        decision: Arc<decision::Engine>,
        delivery: Arc<dyn Delivery>,
    ) -> Result<(), String> {
        if !nucleus::valid_uid(&owner, "voice")
            || target
                .as_ref()
                .is_some_and(|target| !nucleus::valid_uid(target, "r"))
        {
            return Err("Choose a valid listening owner and optional Fiote recipient.".into());
        }
        let information = decision.information();
        if mode == ListeningMode::Model && (!information.admission_validated || !decision.present())
        {
            return Err("General listening needs an installed local model with validated speech admission. Use Dictate and Send meanwhile.".into());
        }
        if !recognition.present() {
            return Err("Install local English speech before enabling listening.".into());
        }
        let mut state = self.state.lock().await;
        if state.closed.iter().any(|closed| closed == &owner) {
            return Err(
                "This listening owner was closed before activation. Start a new listening session."
                    .into(),
            );
        }
        if state.shutting_down {
            return Err("Listening is shutting down. No new session can start.".into());
        }
        if state.status.enabled && state.status.owner.as_deref() == Some(&owner) {
            if !state.pipeline.as_ref().is_some_and(|pipeline| {
                pipeline.mode == mode && pipeline.target == target && pipeline.routing == routing
            }) {
                return Err(
                    "Stop listening before changing its mode, recipient or routing.".into(),
                );
            }
            return Ok(());
        }
        if state.status.enabled || state.running.is_some() {
            return Err("Another listening session is active or finishing. Stop it and wait before starting another.".into());
        }
        state.status.generation = state
            .status
            .generation
            .checked_add(1)
            .ok_or("Listening generation is exhausted.")?;
        state.status.owner = Some(owner);
        state.status.mode = mode;
        state.status.enabled = true;
        state.status.paused = false;
        state.status.conversation = None;
        state.status.receipt = None;
        state.last_activity = None;
        state.status.stage = ListeningStage::Waiting;
        state.status.detail = match mode {
            ListeningMode::Wake => "Waiting for Hey Fiote using your local acoustic references. Start a request within ten seconds after activation, then pause when finished. Requests longer than twenty seconds are refused. Only woken requests are transcribed; activation grants no extra authority.",
            ListeningMode::Addressed => "Addressed listening enabled. Say Hey Fiote or clearly address the chosen/default Fiote by name. Speech is processed locally; listening stops when this view closes.",
            ListeningMode::Model => "General listening enabled for this application session; only locally admitted requests can be sent.",
        }.into();
        state.status.unsent = None;
        state.highest = 0;
        state.pipeline = Some(Pipeline {
            mode,
            recognition,
            decision,
            delivery,
            information,
            routing,
            target,
        });
        self.active.store(true, Ordering::Release);
        Ok(())
    }

    pub(super) async fn stop(&self, owner: &str, generation: u64) -> Result<(), String> {
        if !nucleus::valid_uid(owner, "voice") {
            return Err("Choose a valid listening owner.".into());
        }
        let mut state = self.state.lock().await;
        if generation == 0 {
            if !state.closed.iter().any(|closed| closed == owner) {
                state.closed.push_back(owner.into());
            }
            if state.closed.len() > 128 {
                state.closed.pop_front();
            }
            if state.status.owner.as_deref() != Some(owner) {
                return Ok(());
            }
        }
        if state.status.owner.as_deref() != Some(owner)
            || generation != 0 && state.status.generation != generation
        {
            return Err("This listening session is no longer current.".into());
        }
        state.status.enabled = false;
        state.status.paused = false;
        state.status.conversation = None;
        state.status.generation = state
            .status
            .generation
            .checked_add(1)
            .ok_or("Listening generation is exhausted.")?;
        state.status.stage = ListeningStage::Off;
        state.status.detail =
            "Listening stopped. Already delivered Fiote runs remain independently stoppable."
                .into();
        state.status.unsent = None;
        state.pending = None;
        if let Some(running) = &state.running {
            running.cancelled.store(true, Ordering::Release);
        }
        self.active
            .store(state.running.is_some(), Ordering::Release);
        Ok(())
    }

    pub(super) async fn pause(&self, owner: &str, generation: u64) -> Result<(), String> {
        let mut state = self.state.lock().await;
        if !state.status.enabled
            || state.status.owner.as_deref() != Some(owner)
            || state.status.generation != generation
        {
            return Err("This listening session is no longer current.".into());
        }
        state.status.enabled = false;
        state.status.paused = true;
        state.pending = None;
        state.status.stage = ListeningStage::Off;
        state.status.detail =
            "Listening paused for a spoken reply; the conversation is retained.".into();
        if let Some(running) = &state.running {
            running.cancelled.store(true, Ordering::Release);
        }
        self.active
            .store(state.running.is_some(), Ordering::Release);
        Ok(())
    }

    pub(super) async fn resume(&self, owner: &str, generation: u64) -> Result<(), String> {
        let mut state = self.state.lock().await;
        if state.shutting_down
            || !state.status.paused
            || state.status.owner.as_deref() != Some(owner)
            || state.status.generation != generation
            || state.running.is_some()
            || state.pipeline.is_none()
        {
            return Err(
                "This paused listening session cannot resume. Start a new session when ready."
                    .into(),
            );
        }
        state.status.generation = state
            .status
            .generation
            .checked_add(1)
            .ok_or("Listening generation is exhausted.")?;
        state.status.enabled = true;
        state.status.paused = false;
        state.status.stage = ListeningStage::Waiting;
        state.status.detail = "Listening resumed in the same conversation.".into();
        state.last_activity = Some(Instant::now());
        state.highest = 0;
        self.active.store(true, Ordering::Release);
        Ok(())
    }

    pub(super) async fn push(
        &self,
        owner: &str,
        generation: u64,
        id: u64,
        audio: ::fiote::config::Secret,
    ) -> Result<(), String> {
        self.push_segment(owner, generation, id, audio, false).await
    }

    pub(super) async fn push_woken(
        &self,
        owner: &str,
        generation: u64,
        id: u64,
        audio: ::fiote::config::Secret,
    ) -> Result<(), String> {
        self.push_segment(owner, generation, id, audio, true).await
    }

    async fn push_segment(
        &self,
        owner: &str,
        generation: u64,
        id: u64,
        audio: ::fiote::config::Secret,
        woken: bool,
    ) -> Result<(), String> {
        self.expire_conversation().await;
        let samples = speech::decode_audio(&audio.0)?;
        if samples.len() > 320_000 {
            return Err("General listening accepts complete utterances up to twenty seconds; no partial command was accepted.".into());
        }
        let mut state = self.state.lock().await;
        if !state.status.enabled
            || state.status.owner.as_deref() != Some(owner)
            || state.status.generation != generation
        {
            return Err("This listening segment belongs to a stopped or different session.".into());
        }
        if woken
            && state
                .pipeline
                .as_ref()
                .is_none_or(|pipeline| pipeline.mode != ListeningMode::Wake)
        {
            return Err(
                "An acoustic wake request requires the current wake listening session.".into(),
            );
        }
        if id == 0 || id <= state.highest {
            return Err("This listening segment has already been seen.".into());
        }
        state.highest = id;
        let segment = Segment {
            owner: owner.into(),
            generation,
            id,
            audio,
            woken,
        };
        if state.running.is_some() {
            if state.pending.is_some() {
                state.status.detail = "Listening is busy. The new utterance was not queued; use explicit dictation or wait.".into();
                return Err(state.status.detail.clone());
            }
            state.pending = Some(segment);
            return Ok(());
        }
        let pipeline = state
            .pipeline
            .clone()
            .ok_or("Listening pipeline is unavailable.")?;
        state.status.stage = ListeningStage::Transcribing;
        state.running = Some(spawn(
            self.state.clone(),
            self.active.clone(),
            pipeline,
            segment,
        ));
        Ok(())
    }
}

fn current(state: &State, segment: &Segment, cancelled: &AtomicBool) -> bool {
    state.status.enabled
        && state.status.owner.as_deref() == Some(&segment.owner)
        && state.status.generation == segment.generation
        && !cancelled.load(Ordering::Acquire)
}

struct Outcome {
    stage: ListeningStage,
    detail: String,
    receipt: Option<EntryReceipt>,
    unsent: Option<VoiceRequest>,
}

fn spawn(
    shared: Arc<Mutex<State>>,
    active: Arc<AtomicBool>,
    pipeline: Pipeline,
    segment: Segment,
) -> Running {
    let owner = segment.owner.clone();
    let generation = segment.generation;
    let id = segment.id;
    let cancelled = Arc::new(AtomicBool::new(false));
    let control = cancelled.clone();
    let task = tokio::spawn(async move {
        let outcome = run(&shared, &pipeline, &segment, control.clone()).await;
        let mut state = shared.lock().await;
        if !state.running.as_ref().is_some_and(|running| {
            running.owner == segment.owner
                && running.generation == segment.generation
                && running.id == segment.id
        }) {
            return;
        }
        if current(&state, &segment, &control) {
            match outcome {
                Ok(outcome) => {
                    state.status.stage = outcome.stage;
                    state.status.detail = outcome.detail;
                    if outcome.unsent.is_some() {
                        state.status.enabled = false;
                        state.pending = None;
                        state.status.receipt = None;
                        state.status.detail.push_str(" Listening paused; recover the admitted request before starting again.");
                    }
                    state.status.unsent = outcome.unsent;
                    if outcome.receipt.is_some() {
                        if let Some(receipt) = &outcome.receipt
                            && receipt.message.is_some()
                            && let Some(thread) = &receipt.thread
                        {
                            state.status.conversation = Some(Conversation {
                                target: receipt.target.clone(),
                                thread: thread.clone(),
                            });
                            state.last_activity = Some(Instant::now());
                        }
                        state.status.receipt = outcome.receipt;
                    }
                }
                Err(error) => {
                    if state.status.conversation.is_some() {
                        followup::end(
                            &mut state,
                            "Voice conversation ended after an interrupted utterance. Waiting for a new activation.",
                        );
                    }
                    state.status.stage = ListeningStage::Degraded;
                    state.status.detail = error;
                }
            }
        }
        state.running = None;
        if state.status.enabled
            && let Some(next) = state.pending.take()
        {
            state.status.stage = ListeningStage::Transcribing;
            state.running = Some(spawn(shared.clone(), active.clone(), pipeline, next));
        }
        active.store(
            state.status.enabled || state.running.is_some(),
            Ordering::Release,
        );
    });
    Running {
        owner,
        generation,
        id,
        cancelled,
        _task: task,
    }
}

async fn run(
    shared: &Arc<Mutex<State>>,
    pipeline: &Pipeline,
    segment: &Segment,
    cancelled: Arc<AtomicBool>,
) -> Result<Outcome, String> {
    let (conversation, previous) = {
        let state = shared.lock().await;
        if !current(&state, segment, &cancelled) {
            return Err("Listening stopped before transcription.".into());
        }
        (
            state.status.conversation.clone(),
            state.status.receipt.clone(),
        )
    };
    if pipeline.mode == ListeningMode::Wake && !segment.woken && conversation.is_none() {
        return Ok(Outcome {
            stage: ListeningStage::Waiting,
            detail:
                "Waiting for an acoustic Hey Fiote activation. No speech was transcribed or sent."
                    .into(),
            receipt: None,
            unsent: None,
        });
    }
    let text = pipeline
        .recognition
        .transcribe(segment.audio.clone(), cancelled.clone())
        .await?;
    if text.trim().is_empty() || text.len() > 65_536 {
        return Err("Local speech did not produce a bounded request. Nothing was sent.".into());
    }
    {
        let mut state = shared.lock().await;
        if !current(&state, segment, &cancelled) {
            return Err("Listening stopped before admission; nothing was sent.".into());
        }
        state.status.stage = ListeningStage::Deciding;
    }
    if let Some(conversation) = &conversation {
        let admitted = followup::admit(
            shared,
            pipeline,
            segment,
            &cancelled,
            conversation,
            previous.as_ref(),
            &text,
        )
        .await;
        if !matches!(admitted, Ok(Admission::AssistantRequest)) {
            let detail = match admitted {
                Ok(Admission::OrdinaryConversation) => "The speech closes or leaves the Fiote conversation. Waiting for a new activation; no message or action was created.".into(),
                Ok(_) => "It is unclear whether this speech continues the Fiote conversation. Waiting for a new activation; nothing was sent.".into(),
                Err(error) => format!("Voice continuation could not be checked: {error} Waiting for a new activation; nothing was sent."),
            };
            let mut state = shared.lock().await;
            if current(&state, segment, &cancelled) {
                followup::end(&mut state, &detail);
            }
            return Ok(Outcome {
                stage: ListeningStage::Ignored,
                detail,
                receipt: None,
                unsent: None,
            });
        }
    }
    let known = if conversation.is_none() {
        pipeline.delivery.directory().await.ok()
    } else {
        None
    };
    if pipeline.mode == ListeningMode::Addressed && conversation.is_none() {
        let title = match &pipeline.target {
            Some(target) => pipeline.delivery.title(target).await?,
            None => known
                .as_ref()
                .and_then(|directory| directory.intake.as_ref())
                .map(|intake| intake.title.clone()),
        };
        let mut names = vec!["Fiote".into(), "Assistant".into()];
        names.extend(title);
        if !addressed::matches(&text, &names) {
            return Ok(Outcome {
                stage: ListeningStage::Ignored,
                detail: "No clear address to the chosen/default Fiote was heard. Nothing was sent."
                    .into(),
                receipt: None,
                unsent: None,
            });
        }
    }
    let mut context = decision::Context {
        assistant_names: vec!["Assistant".into(), "Fiote".into()],
        ..Default::default()
    };
    if let Some(known) = &known {
        for title in known
            .intake
            .iter()
            .map(|intake| intake.title.as_str())
            .chain(
                known
                    .fiotes
                    .iter()
                    .map(|candidate| candidate.title.as_str()),
            )
        {
            if !title.trim().is_empty()
                && title.len() <= 256
                && !context
                    .assistant_names
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(title))
                && context.assistant_names.len() < 32
            {
                context.assistant_names.push(title.into());
                if context.encode().is_err() {
                    context.assistant_names.pop();
                }
            }
        }
    }
    let directory = known.filter(|directory| {
        pipeline.routing
            && pipeline.target.is_none()
            && (pipeline.information.routing_supported || pipeline.information.routing_validated)
            && pipeline.decision.present()
            && directory.next_offset.is_none()
    });
    let candidates = directory
        .as_ref()
        .map(|directory| {
            directory
                .fiotes
                .iter()
                .filter(|candidate| candidate.enabled && !candidate.description_truncated)
                .map(|candidate| Candidate {
                    record: candidate.record.clone(),
                    title: candidate.title.clone(),
                    purpose: candidate.description.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    let request_id = format!("{}-{}-{}", segment.owner, segment.generation, segment.id);
    let input = Input {
        request_id: request_id.clone(),
        generation: segment.generation,
        text: text.clone(),
        context: context.encode()?,
        candidates,
        admission: pipeline.mode == ListeningMode::Model && conversation.is_none(),
        routing: directory.is_some(),
    };
    let output = if input.admission || input.routing {
        let decided = async {
            let output = pipeline
                .decision
                .submit(input, cancelled.clone())?
                .await
                .map_err(|error| error.to_string())??;
            if output.request_id != request_id
                || output.generation != segment.generation
                || output.model != pipeline.information.model
                || output.revision != pipeline.information.revision
            {
                return Err("Local decision identity changed; nothing was sent.".into());
            }
            Ok(output)
        }
        .await;
        match decided {
            Ok(output) => Some(output),
            Err(_) if pipeline.mode != ListeningMode::Model => None,
            Err(error) => return Err(error),
        }
    } else {
        None
    };
    if pipeline.mode == ListeningMode::Model
        && conversation.is_none()
        && output
            .as_ref()
            .and_then(|output| output.admission.as_ref())
            .and_then(|choice| choice.value)
            != Some(Admission::AssistantRequest)
    {
        return Ok(Outcome {
            stage: ListeningStage::Ignored,
            detail: "Speech ignored locally; no Message or Fiote request was created.".into(),
            receipt: None,
            unsent: None,
        });
    }
    let target = conversation
        .as_ref()
        .map(|conversation| conversation.target.clone())
        .or(pipeline.target.clone())
        .or_else(|| {
            output
                .and_then(|output| output.destination)
                .and_then(|choice| choice.value)
                .filter(|target| {
                    directory.as_ref().is_some_and(|directory| {
                        directory.fiotes.iter().any(|candidate| {
                            &candidate.record == target
                                && candidate.enabled
                                && !candidate.description_truncated
                        })
                    })
                })
        });
    let request = EntryRequest {
        request_id,
        target,
        text,
        content: Vec::new(),
    };
    let state = shared.lock().await;
    if !current(&state, segment, &cancelled) {
        return Err("Listening stopped before delivery; nothing was sent.".into());
    }
    let submission = match conversation {
        Some(conversation) => {
            pipeline
                .delivery
                .continue_conversation(conversation, request.clone())
                .await
        }
        None => pipeline.delivery.send(request.clone()).await,
    };
    let delivered = match submission {
        Err(error) => match pipeline.delivery.inspect(&request.request_id).await {
            Ok(Some(receipt)) => Ok(receipt),
            Ok(None) => Err((error, false)),
            Err(_) => Err((error, true)),
        },
        Ok(receipt) => Ok(receipt),
    };
    drop(state);
    let recoverable = |uncertain| VoiceRequest {
        owner: segment.owner.clone(),
        generation: segment.generation,
        segment: segment.id,
        request: request.clone(),
        uncertain,
    };
    match delivered {
        Ok(receipt) if receipt.message.is_some() => Ok(Outcome {
            stage: ListeningStage::Working,
            detail: "Request delivered; inspect its Fiote conversation for the result.".into(),
            receipt: Some(receipt),
            unsent: None,
        }),
        Ok(receipt) => Ok(Outcome {
            stage: ListeningStage::Degraded,
            detail: receipt.detail.clone(),
            unsent: Some(recoverable(
                receipt.state != "failed" || receipt.thread.is_some(),
            )),
            receipt: Some(receipt),
        }),
        Err((error, uncertain)) => Ok(Outcome {
            stage: ListeningStage::Degraded,
            detail: error,
            receipt: None,
            unsent: Some(recoverable(uncertain)),
        }),
    }
}

#[cfg(test)]
mod tests;
