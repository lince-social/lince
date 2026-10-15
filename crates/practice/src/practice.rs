use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Operation {
    OpenEdit,
    PlaceSand,
    ComposeCastle,
    UngroupCastle,
    PlaceArea,
    Attract,
    Repel,
    CreateProteinArea,
    PreviewProtein,
    PresentProperties,
    MatchRecord,
    EnterArea,
    LeaveArea,
    CreateWorkspace,
    FrameCanvas,
    MoveSand,
    EditText,
    SetAppearance,
    ResetAppearance,
    InspectShortcuts,
    ScaleArea,
    DisableAreaEffect,
    InspectArea,
    ReadRecord,
    ApplyAssertion,
    ReadVocabulary,
    OpenRecordViews,
    ReadFacts,
    ArrangeProtein,
    CompleteTask,
    UndoTask,
    MoveTask,
    StartTimer,
    StopTimer,
    OpenDatedRecord,
    SetOperation,
    ShowNotice,
    DismissNotice,
    CreateFrequency,
    PreviewRule,
    RunRule,
    PauseRule,
    PreviewHabit,
    ImportHabit,
    CompleteHabit,
    SaveCommand,
    RunCommand,
    StepSimulation,
    StopSimulation,
    InspectFiote,
    InspectOrgan,
    SwitchOrgan,
    PairContact,
    InspectAccess,
    ShareRecords,
    InspectDevices,
    InspectMail,
    SearchDiscovery,
    PrepareAnnouncement,
    OpenDiscoveryRequest,
    SendPracticeMessage,
    InspectCalls,
    CheckTransfer,
    AgreeTransfer,
    ActivateTransfer,
    PauseTransferRule,
    PreviewImportConflict,
    CancelImportPreview,
    PreviewCleanImport,
    ImportPreparedRecords,
    ExportPracticeFiles,
    EditPracticeFile,
    StopPracticeSync,
    SendPracticeCopy,
    AcceptPracticeCopy,
    InspectBackup,
    OpenPracticeFile,
    EditSavePracticeFile,
    InspectLanguageTools,
    NextDocumentPage,
    InspectTerminal,
    InspectFileChoices,
    CancelFileChoices,
    PreviewRecording,
    StopRecordingPlayback,
    EnableAreaSound,
    AssignAreaSound,
    PreviewAreaSound,
    AddShaderExample,
    SwitchSpatialView,
    ReturnFlatView,
    SaveCustomCastle,
    AddCustomCastle,
    InspectControl,
    InspectInformation,
    InspectSandCredits,
    InspectLaboratory,
}

impl Operation {
    pub fn needs_cell(self) -> bool {
        matches!(
            self,
            Self::PlaceArea
                | Self::Attract
                | Self::Repel
                | Self::CreateProteinArea
                | Self::PreviewProtein
                | Self::PresentProperties
                | Self::MatchRecord
                | Self::EnterArea
                | Self::LeaveArea
                | Self::ReadRecord
                | Self::ApplyAssertion
                | Self::ReadVocabulary
                | Self::OpenRecordViews
                | Self::ReadFacts
                | Self::ArrangeProtein
                | Self::CompleteTask
                | Self::UndoTask
                | Self::MoveTask
                | Self::OpenDatedRecord
                | Self::SetOperation
                | Self::CreateFrequency
                | Self::PreviewRule
                | Self::RunRule
                | Self::PauseRule
                | Self::PreviewHabit
                | Self::ImportHabit
                | Self::CompleteHabit
                | Self::SaveCommand
                | Self::RunCommand
                | Self::StepSimulation
                | Self::StopSimulation
                | Self::InspectFiote
                | Self::InspectOrgan
                | Self::SwitchOrgan
                | Self::PairContact
                | Self::InspectAccess
                | Self::ShareRecords
                | Self::InspectDevices
                | Self::InspectMail
                | Self::SearchDiscovery
                | Self::PrepareAnnouncement
                | Self::OpenDiscoveryRequest
                | Self::SendPracticeMessage
                | Self::InspectCalls
                | Self::CheckTransfer
                | Self::AgreeTransfer
                | Self::ActivateTransfer
                | Self::PauseTransferRule
                | Self::PreviewImportConflict
                | Self::CancelImportPreview
                | Self::PreviewCleanImport
                | Self::ImportPreparedRecords
                | Self::ExportPracticeFiles
                | Self::EditPracticeFile
                | Self::StopPracticeSync
                | Self::SendPracticeCopy
                | Self::AcceptPracticeCopy
                | Self::InspectBackup
                | Self::OpenPracticeFile
                | Self::EditSavePracticeFile
                | Self::InspectLanguageTools
                | Self::NextDocumentPage
                | Self::InspectTerminal
                | Self::InspectFileChoices
                | Self::CancelFileChoices
                | Self::PreviewRecording
                | Self::StopRecordingPlayback
                | Self::EnableAreaSound
                | Self::AssignAreaSound
                | Self::PreviewAreaSound
                | Self::AddShaderExample
                | Self::SaveCustomCastle
                | Self::AddCustomCastle
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub slug: &'static str,
    pub subject: &'static str,
    pub operation: Option<Operation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lesson {
    pub subject: &'static str,
    pub prerequisites: &'static [&'static str],
    pub steps: &'static [Step],
}

pub const LESSONS: &[Lesson] = &[
    Lesson {
        subject: "sands",
        prerequisites: &["interface"],
        steps: &[
            Step {
                slug: "step-interface-open-edit",
                subject: "sands",
                operation: Some(Operation::OpenEdit),
            },
            Step {
                slug: "step-interface-place-sand",
                subject: "sands",
                operation: Some(Operation::PlaceSand),
            },
            Step {
                slug: "step-interface-move-sand",
                subject: "sands",
                operation: Some(Operation::MoveSand),
            },
            Step {
                slug: "step-interface-compose-castle",
                subject: "castles",
                operation: Some(Operation::ComposeCastle),
            },
        ],
    },
    Lesson {
        subject: "areas-of-influence",
        prerequisites: &["sands"],
        steps: &[
            Step {
                slug: "step-area-place",
                subject: "areas-of-influence",
                operation: Some(Operation::PlaceArea),
            },
            Step {
                slug: "step-area-attract",
                subject: "areas-of-influence",
                operation: Some(Operation::Attract),
            },
            Step {
                slug: "step-area-repel",
                subject: "areas-of-influence",
                operation: Some(Operation::Repel),
            },
        ],
    },
    Lesson {
        subject: "protein",
        prerequisites: &["record", "assertion", "areas-of-influence"],
        steps: &[
            Step {
                slug: "step-protein-create-spawn-area",
                subject: "protein",
                operation: Some(Operation::CreateProteinArea),
            },
            Step {
                slug: "step-protein-filter-preview",
                subject: "protein",
                operation: Some(Operation::PreviewProtein),
            },
            Step {
                slug: "step-protein-present-properties",
                subject: "protein",
                operation: Some(Operation::PresentProperties),
            },
        ],
    },
    Lesson {
        subject: "area-record-actions",
        prerequisites: &["protein", "assertion", "areas-of-influence"],
        steps: &[
            Step {
                slug: "step-area-match-record",
                subject: "area-record-actions",
                operation: Some(Operation::MatchRecord),
            },
            Step {
                slug: "step-area-change-on-entry",
                subject: "area-record-actions",
                operation: Some(Operation::EnterArea),
            },
            Step {
                slug: "step-area-change-on-exit",
                subject: "area-record-actions",
                operation: Some(Operation::LeaveArea),
            },
        ],
    },
];

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Target {
    pub window: u64,
    pub workspace: u64,
    pub owner: String,
    pub role: String,
}

pub fn resolve<T: Copy>(
    target: &Target,
    candidates: impl IntoIterator<Item = (Target, T)>,
) -> Result<T, Resolution> {
    let mut matches = candidates
        .into_iter()
        .filter(|(candidate, _)| candidate == target);
    let Some((_, entity)) = matches.next() else {
        return Err(Resolution::Missing);
    };
    if matches.next().is_some() {
        return Err(Resolution::Ambiguous);
    }
    Ok(entity)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    Missing,
    Ambiguous,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Free,
    Assisted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Progress {
    Visited,
    Practiced,
    Skipped,
    Unavailable,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearningProgress(pub BTreeMap<String, Progress>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub session: u64,
    pub generation: u64,
    pub serial: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Ready,
    Waiting {
        ticket: Ticket,
        deadline: u64,
    },
    Failed {
        ticket: Option<Ticket>,
        message: String,
    },
    Unavailable(String),
    Complete,
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Observation {
    Waiting,
    Complete,
    Viewed,
    Failed(String),
    Unavailable(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    Execute {
        ticket: Ticket,
        operation: Operation,
    },
    Advanced,
    Finished,
}

#[derive(Clone, Debug)]
pub struct Runner {
    pub session: u64,
    pub mode: Mode,
    pub lesson: &'static Lesson,
    pub step: usize,
    pub phase: Phase,
    pub progress: LearningProgress,
    generation: u64,
    serial: u64,
    target_ready: bool,
}

impl Runner {
    pub fn start(
        session: u64,
        lesson: &'static Lesson,
        mode: Mode,
        progress: LearningProgress,
    ) -> Self {
        let mut runner = Self {
            session,
            lesson,
            mode,
            step: 0,
            phase: Phase::Ready,
            progress,
            generation: 0,
            serial: 0,
            target_ready: false,
        };
        runner.visit();
        runner
    }

    pub fn current(&self) -> Option<&'static Step> {
        if matches!(self.phase, Phase::Closed | Phase::Complete) {
            return None;
        }
        self.lesson.steps.get(self.step)
    }

    fn visit(&mut self) {
        if let Some(step) = self.current() {
            self.progress
                .0
                .entry(step.slug.into())
                .or_insert(Progress::Visited);
        }
    }

    pub fn set_target(&mut self, resolution: Result<(), Resolution>) {
        self.target_ready = resolution.is_ok();
    }

    pub fn restricted(&self) -> bool {
        self.mode == Mode::Assisted && self.target_ready && matches!(self.phase, Phase::Ready)
    }

    pub fn permits(
        &self,
        target: &Target,
        intended: &Target,
        recovery: bool,
        navigation: bool,
    ) -> bool {
        recovery
            || !self.restricted()
            || target.window != intended.window
            || target.workspace != intended.workspace
            || target == intended
            || navigation
    }

    pub fn switch_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    fn invalidate(&mut self) {
        self.generation += 1;
        self.target_ready = false;
        self.phase = Phase::Ready;
    }

    pub fn select(&mut self, lesson: &'static Lesson, step: usize) -> bool {
        if step >= lesson.steps.len() || matches!(self.phase, Phase::Closed) {
            return false;
        }
        self.invalidate();
        self.lesson = lesson;
        self.step = step;
        self.visit();
        true
    }

    fn advance(&mut self, progress: Progress) -> Effect {
        let Some(current) = self.current() else {
            return Effect::None;
        };
        let saved = self
            .progress
            .0
            .entry(current.slug.into())
            .or_insert(progress);
        if *saved != Progress::Practiced {
            *saved = progress;
        }
        self.invalidate();
        self.step += 1;
        if self.step >= self.lesson.steps.len() {
            self.phase = Phase::Complete;
            Effect::Finished
        } else {
            self.visit();
            Effect::Advanced
        }
    }

    pub fn skip(&mut self) -> Effect {
        let progress = if matches!(self.phase, Phase::Unavailable(_)) {
            Progress::Unavailable
        } else {
            Progress::Skipped
        };
        self.advance(progress)
    }

    pub fn next(&mut self, observation: Observation, now: u64, timeout: u64) -> Effect {
        let Some(current) = self.current() else {
            return Effect::None;
        };
        if matches!(observation, Observation::Viewed) {
            return self.advance(Progress::Visited);
        }
        if matches!(observation, Observation::Complete) {
            return self.advance(if current.operation.is_some() {
                Progress::Practiced
            } else {
                Progress::Visited
            });
        }
        match observation {
            Observation::Failed(message) => {
                self.phase = Phase::Failed {
                    ticket: self.ticket(),
                    message,
                };
                return Effect::None;
            }
            Observation::Unavailable(message) => {
                self.phase = Phase::Unavailable(message);
                return Effect::None;
            }
            _ => {}
        }
        if matches!(self.phase, Phase::Waiting { .. } | Phase::Unavailable(_)) {
            return Effect::None;
        }
        let Some(operation) = current.operation else {
            return self.advance(Progress::Visited);
        };
        let ticket = self.ticket().unwrap_or_else(|| {
            self.serial += 1;
            Ticket {
                session: self.session,
                generation: self.generation,
                serial: self.serial,
            }
        });
        self.phase = Phase::Waiting {
            ticket,
            deadline: now.saturating_add(timeout),
        };
        Effect::Execute { ticket, operation }
    }

    fn ticket(&self) -> Option<Ticket> {
        match self.phase {
            Phase::Waiting { ticket, .. }
            | Phase::Failed {
                ticket: Some(ticket),
                ..
            } => Some(ticket),
            _ => None,
        }
    }

    pub fn response(&mut self, ticket: Ticket, observation: Observation) -> Effect {
        if self.ticket() != Some(ticket) {
            return Effect::None;
        }
        match observation {
            Observation::Complete => self.advance(Progress::Practiced),
            Observation::Viewed => self.advance(Progress::Visited),
            Observation::Failed(message) => {
                self.phase = Phase::Failed {
                    ticket: Some(ticket),
                    message,
                };
                Effect::None
            }
            Observation::Unavailable(message) => {
                self.phase = Phase::Unavailable(message);
                Effect::None
            }
            Observation::Waiting => Effect::None,
        }
    }

    pub fn tick(&mut self, now: u64) -> bool {
        if let Phase::Waiting { ticket, deadline } = self.phase
            && now >= deadline
        {
            self.phase = Phase::Failed {
                ticket: Some(ticket),
                message: "The action has not been confirmed. Retry, Skip, switch to Free or Close."
                    .into(),
            };
            return true;
        }
        false
    }

    pub fn close(&mut self) {
        self.invalidate();
        self.phase = Phase::Closed;
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ownership {
    created: BTreeSet<String>,
    pending: BTreeSet<String>,
}

impl Ownership {
    pub fn created(&mut self, uid: impl Into<String>) {
        self.created.insert(uid.into());
    }
    pub fn pending(&mut self, uid: impl Into<String>) {
        self.pending.insert(uid.into());
    }
    pub fn confirmed(&mut self, request: &str, uid: impl Into<String>) {
        self.pending.remove(request);
        self.created(uid);
    }
    pub fn discard(&mut self) -> Vec<String> {
        std::mem::take(&mut self.created).into_iter().collect()
    }
    pub fn resources(&self) -> impl Iterator<Item = &str> {
        self.created.iter().map(String::as_str)
    }
    pub fn outstanding(&self) -> impl Iterator<Item = &str> {
        self.pending.iter().map(String::as_str)
    }
}
