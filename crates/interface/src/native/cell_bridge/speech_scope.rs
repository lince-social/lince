use cell::{ClientMessage, ServerMessage, Session, speech::Request};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[derive(Clone, PartialEq, Eq)]
enum Work {
    Listening(String),
    Job(String),
    Voice(String),
}

impl Work {
    fn request(&self) -> Request {
        match self {
            Self::Listening(owner) => Request::StopListening {
                owner: owner.clone(),
                generation: 0,
            },
            Self::Job(job) => Request::Cancel { job: job.clone() },
            Self::Voice(job) => Request::Voice {
                request: cell::fiote_communication::speech::voice::Request::Cancel {
                    job: job.clone(),
                },
            },
        }
    }
}

#[derive(Default)]
struct State {
    next: u64,
    pending: HashMap<u64, Work>,
    listening: Option<Work>,
    job: Option<Work>,
    voice: Option<Work>,
}

pub(super) struct Scope {
    state: Arc<Mutex<State>>,
    session: Option<Session>,
    runtime: tokio::runtime::Handle,
}

pub(super) struct Pending {
    state: Arc<Mutex<State>>,
    index: u64,
    work: Work,
}

impl Scope {
    pub(super) fn new(session: Session) -> Self {
        Self {
            state: Arc::new(Mutex::new(State::default())),
            session: Some(session),
            runtime: tokio::runtime::Handle::current(),
        }
    }

    pub(super) fn begin(&self, request: &Request) -> Result<Option<Pending>, String> {
        let work = match request {
            Request::StartListening { owner, .. } | Request::ResumeListening { owner, .. } => {
                Work::Listening(owner.clone())
            }
            Request::Voice {
                request:
                    cell::fiote_communication::speech::voice::Request::Install { job, .. }
                    | cell::fiote_communication::speech::voice::Request::Speak { job, .. },
            } => Work::Voice(job.clone()),
            Request::StartLocal { job, .. }
            | Request::StartEndpoint { job, .. }
            | Request::InstallLocal { job, .. }
            | Request::InstallDecision { job, .. }
            | Request::Decide { job, .. } => Work::Job(job.clone()),
            _ => return Ok(None),
        };
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.pending.len() >= 8 {
            return Err("Eight speech requests are already awaiting completion.".into());
        }
        state.next = state
            .next
            .checked_add(1)
            .ok_or("Speech request tracking is exhausted.")?;
        let index = state.next;
        state.pending.insert(index, work.clone());
        Ok(Some(Pending {
            state: self.state.clone(),
            index,
            work,
        }))
    }
}

impl Pending {
    pub(super) fn complete(self, messages: &[ServerMessage]) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        for message in messages {
            if let ServerMessage::Speech { status, .. } = message {
                match &self.work {
                    Work::Listening(owner)
                        if status.listening.enabled
                            && status.listening.owner.as_ref() == Some(owner) =>
                    {
                        state.listening = Some(self.work.clone());
                    }
                    Work::Job(job)
                        if status.job.as_ref().is_some_and(|result| &result.id == job) =>
                    {
                        state.job = Some(self.work.clone());
                    }
                    Work::Voice(job)
                        if status
                            .voice
                            .job
                            .as_ref()
                            .is_some_and(|result| &result.id == job) =>
                    {
                        state.voice = Some(self.work.clone());
                    }
                    _ => {}
                }
            }
        }
        state.pending.remove(&self.index);
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let mut work = state
            .pending
            .drain()
            .map(|(_, work)| work)
            .collect::<Vec<_>>();
        work.extend(state.listening.take());
        work.extend(state.job.take());
        work.extend(state.voice.take());
        drop(state);
        if work.is_empty() {
            return;
        }
        if let Some(mut session) = self.session.take() {
            self.runtime.spawn(async move {
                for work in work {
                    let _ = session
                        .handle(ClientMessage::Speech {
                            id: nucleus::new_uid("speech-close"),
                            request: work.request(),
                        })
                        .await;
                }
            });
        }
    }
}
