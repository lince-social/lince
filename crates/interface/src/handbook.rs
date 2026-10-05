use crate::practice::Step;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Page {
    pub slug: &'static str,
    pub chapter: &'static str,
    pub reference: Option<&'static str>,
}

pub fn foundations(subject: &str) -> &'static [&'static str] {
    match subject {
        "canvas" | "edit-mode" => &["workspaces"],
        "sands" => &["edit-mode", "canvas"],
        "text-sands" | "castles" | "appearance" | "shortcuts" => &["sands"],
        "learn-areas-of-influence" => &["sands", "canvas"],
        "area-forces" | "area-effects" | "area-inspection" => &["learn-areas-of-influence"],
        "learn-assertion" | "facts" => &["learn-record"],
        "learn-ontology" => &["learn-assertion"],
        "record-castles" => &["learn-assertion", "learn-ontology"],
        "protein" => &[
            "learn-record",
            "learn-assertion",
            "learn-areas-of-influence",
        ],
        "protein-presentation" | "protein-arrangement" => &["protein", "castles"],
        "area-record-actions" => &["protein", "learn-areas-of-influence"],
        "todo" | "operation" => &["protein", "facts"],
        "kanban" => &["todo", "area-record-actions"],
        "time" => &["facts"],
        "calendar" => &["facts", "protein"],
        "notifications" => &["facts"],
        "frequency" => &["calendar"],
        "learn-karma" => &["frequency", "facts"],
        "habits" => &["todo", "frequency", "learn-karma"],
        "commands" => &["learn-karma"],
        "simulation" => &["learn-karma", "facts"],
        "learn-fiote" => &["record-castles", "learn-karma"],
        "learn-organ" => &["learn-record", "protein"],
        "contacts" | "access-control" => &["learn-organ"],
        "organ-sync" => &["contacts", "access-control"],
        "devices" => &["organ-sync", "learn-karma"],
        "mail" => &["organ-sync", "devices"],
        "discovery" => &["learn-assertion", "contacts", "mail"],
        "conversations" => &["mail", "discovery"],
        "calls" => &["conversations"],
        "learn-transfer" => &["learn-record", "facts", "organ-sync"],
        "transfer-automation" => &["learn-karma", "learn-transfer"],
        "instinct-import" => &["learn-record", "learn-ontology"],
        "learn-sync" => &["protein", "instinct-import"],
        "blob-sync" => &["organ-sync", "learn-sync"],
        "backup" => &["learn-sync", "blob-sync"],
        "ide" => &["text-sands", "learn-sync"],
        "language-tools" | "documents" | "terminal" => &["ide"],
        "external-files" => &["ide", "documents"],
        "recorder" => &["sands", "facts"],
        "area-sound" => &["learn-areas-of-influence", "recorder"],
        "shaders" => &["record-castles"],
        "topology" => &["canvas", "sands", "learn-areas-of-influence"],
        "custom-castles" => &["castles", "protein-presentation", "organ-sync"],
        "freedoom" => &["sands", "shortcuts"],
        "inspection" => &["area-inspection", "facts"],
        "information" => &["learn-installation", "sands"],
        "laboratory" => &["simulation", "inspection"],
        _ => &[],
    }
}

pub const PAGES: &[Page] = &[
    Page {
        slug: "learn-philosophy",
        chapter: "instinct-welcome",
        reference: Some("philosophy"),
    },
    Page {
        slug: "learn-installation",
        chapter: "instinct-welcome",
        reference: Some("installation"),
    },
    Page {
        slug: "instinct",
        chapter: "instinct-welcome",
        reference: None,
    },
    Page {
        slug: "workspaces",
        chapter: "instinct-interface",
        reference: None,
    },
    Page {
        slug: "canvas",
        chapter: "instinct-interface",
        reference: None,
    },
    Page {
        slug: "edit-mode",
        chapter: "instinct-interface",
        reference: None,
    },
    Page {
        slug: "sands",
        chapter: "instinct-interface",
        reference: None,
    },
    Page {
        slug: "text-sands",
        chapter: "instinct-interface",
        reference: None,
    },
    Page {
        slug: "castles",
        chapter: "instinct-interface",
        reference: None,
    },
    Page {
        slug: "appearance",
        chapter: "instinct-interface",
        reference: None,
    },
    Page {
        slug: "shortcuts",
        chapter: "instinct-interface",
        reference: None,
    },
    Page {
        slug: "learn-areas-of-influence",
        chapter: "instinct-areas",
        reference: Some("areas-of-influence"),
    },
    Page {
        slug: "area-forces",
        chapter: "instinct-areas",
        reference: None,
    },
    Page {
        slug: "area-effects",
        chapter: "instinct-areas",
        reference: None,
    },
    Page {
        slug: "area-inspection",
        chapter: "instinct-areas",
        reference: None,
    },
    Page {
        slug: "learn-record",
        chapter: "instinct-records",
        reference: Some("record"),
    },
    Page {
        slug: "learn-assertion",
        chapter: "instinct-records",
        reference: Some("assertion"),
    },
    Page {
        slug: "learn-ontology",
        chapter: "instinct-records",
        reference: Some("ontology"),
    },
    Page {
        slug: "record-castles",
        chapter: "instinct-records",
        reference: None,
    },
    Page {
        slug: "facts",
        chapter: "instinct-records",
        reference: None,
    },
    Page {
        slug: "protein",
        chapter: "instinct-protein",
        reference: None,
    },
    Page {
        slug: "protein-presentation",
        chapter: "instinct-protein",
        reference: None,
    },
    Page {
        slug: "area-record-actions",
        chapter: "instinct-protein",
        reference: None,
    },
    Page {
        slug: "protein-arrangement",
        chapter: "instinct-protein",
        reference: None,
    },
    Page {
        slug: "todo",
        chapter: "instinct-work",
        reference: None,
    },
    Page {
        slug: "kanban",
        chapter: "instinct-work",
        reference: None,
    },
    Page {
        slug: "time",
        chapter: "instinct-work",
        reference: None,
    },
    Page {
        slug: "calendar",
        chapter: "instinct-work",
        reference: None,
    },
    Page {
        slug: "operation",
        chapter: "instinct-work",
        reference: None,
    },
    Page {
        slug: "notifications",
        chapter: "instinct-work",
        reference: None,
    },
    Page {
        slug: "frequency",
        chapter: "instinct-automation",
        reference: None,
    },
    Page {
        slug: "learn-karma",
        chapter: "instinct-automation",
        reference: Some("karma"),
    },
    Page {
        slug: "habits",
        chapter: "instinct-automation",
        reference: None,
    },
    Page {
        slug: "commands",
        chapter: "instinct-automation",
        reference: None,
    },
    Page {
        slug: "simulation",
        chapter: "instinct-automation",
        reference: None,
    },
    Page {
        slug: "learn-fiote",
        chapter: "instinct-automation",
        reference: Some("fiote"),
    },
    Page {
        slug: "learn-organ",
        chapter: "instinct-sharing",
        reference: Some("organ"),
    },
    Page {
        slug: "contacts",
        chapter: "instinct-sharing",
        reference: None,
    },
    Page {
        slug: "access-control",
        chapter: "instinct-sharing",
        reference: None,
    },
    Page {
        slug: "organ-sync",
        chapter: "instinct-sharing",
        reference: None,
    },
    Page {
        slug: "devices",
        chapter: "instinct-sharing",
        reference: None,
    },
    Page {
        slug: "mail",
        chapter: "instinct-sharing",
        reference: None,
    },
    Page {
        slug: "discovery",
        chapter: "instinct-sharing",
        reference: None,
    },
    Page {
        slug: "conversations",
        chapter: "instinct-sharing",
        reference: None,
    },
    Page {
        slug: "calls",
        chapter: "instinct-sharing",
        reference: None,
    },
    Page {
        slug: "learn-transfer",
        chapter: "instinct-transfers",
        reference: Some("transfer"),
    },
    Page {
        slug: "transfer-automation",
        chapter: "instinct-transfers",
        reference: None,
    },
    Page {
        slug: "instinct-import",
        chapter: "instinct-disk",
        reference: None,
    },
    Page {
        slug: "learn-sync",
        chapter: "instinct-disk",
        reference: Some("sync"),
    },
    Page {
        slug: "blob-sync",
        chapter: "instinct-disk",
        reference: None,
    },
    Page {
        slug: "backup",
        chapter: "instinct-disk",
        reference: None,
    },
    Page {
        slug: "ide",
        chapter: "instinct-files",
        reference: None,
    },
    Page {
        slug: "language-tools",
        chapter: "instinct-files",
        reference: None,
    },
    Page {
        slug: "documents",
        chapter: "instinct-files",
        reference: None,
    },
    Page {
        slug: "terminal",
        chapter: "instinct-files",
        reference: None,
    },
    Page {
        slug: "external-files",
        chapter: "instinct-files",
        reference: None,
    },
    Page {
        slug: "recorder",
        chapter: "instinct-creative",
        reference: None,
    },
    Page {
        slug: "area-sound",
        chapter: "instinct-creative",
        reference: None,
    },
    Page {
        slug: "shaders",
        chapter: "instinct-creative",
        reference: None,
    },
    Page {
        slug: "topology",
        chapter: "instinct-creative",
        reference: None,
    },
    Page {
        slug: "custom-castles",
        chapter: "instinct-creative",
        reference: None,
    },
    Page {
        slug: "freedoom",
        chapter: "instinct-creative",
        reference: None,
    },
    Page {
        slug: "inspection",
        chapter: "instinct-maintaining",
        reference: None,
    },
    Page {
        slug: "information",
        chapter: "instinct-maintaining",
        reference: None,
    },
    Page {
        slug: "laboratory",
        chapter: "instinct-maintaining",
        reference: None,
    },
];

pub const READING: &[crate::practice::Lesson] = &[
    crate::practice::Lesson {
        subject: "learn-philosophy",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "learn-philosophy",
            subject: "learn-philosophy",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "learn-installation",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "learn-installation",
            subject: "learn-installation",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "instinct",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "instinct",
            subject: "instinct",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "workspaces",
        prerequisites: &[],
        steps: &[Step {
            slug: "step-workspaces-create",
            subject: "workspaces",
            operation: Some(crate::practice::Operation::CreateWorkspace),
        }],
    },
    crate::practice::Lesson {
        subject: "canvas",
        prerequisites: &[],
        steps: &[Step {
            slug: "step-canvas-frame",
            subject: "canvas",
            operation: Some(crate::practice::Operation::FrameCanvas),
        }],
    },
    crate::practice::Lesson {
        subject: "edit-mode",
        prerequisites: &[],
        steps: &[Step {
            slug: "step-edit-mode-open",
            subject: "edit-mode",
            operation: Some(crate::practice::Operation::OpenEdit),
        }],
    },
    crate::practice::Lesson {
        subject: "sands",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "sands",
            subject: "sands",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "text-sands",
        prerequisites: &[],
        steps: &[Step {
            slug: "step-text-sands-edit",
            subject: "text-sands",
            operation: Some(crate::practice::Operation::EditText),
        }],
    },
    crate::practice::Lesson {
        subject: "castles",
        prerequisites: &[],
        steps: &[
            crate::practice::Step {
                slug: "step-interface-compose-castle",
                subject: "castles",
                operation: Some(crate::practice::Operation::ComposeCastle),
            },
            Step {
                slug: "step-interface-ungroup-castle",
                subject: "castles",
                operation: Some(crate::practice::Operation::UngroupCastle),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "appearance",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-appearance-change",
                subject: "appearance",
                operation: Some(crate::practice::Operation::SetAppearance),
            },
            Step {
                slug: "step-appearance-reset",
                subject: "appearance",
                operation: Some(crate::practice::Operation::ResetAppearance),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "shortcuts",
        prerequisites: &[],
        steps: &[Step {
            slug: "step-shortcuts-inspect",
            subject: "shortcuts",
            operation: Some(crate::practice::Operation::InspectShortcuts),
        }],
    },
    crate::practice::Lesson {
        subject: "learn-areas-of-influence",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "learn-areas-of-influence",
            subject: "learn-areas-of-influence",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "area-forces",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "area-forces",
            subject: "area-forces",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "area-effects",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-area-effects-scale",
                subject: "area-effects",
                operation: Some(crate::practice::Operation::ScaleArea),
            },
            Step {
                slug: "step-area-effects-disable",
                subject: "area-effects",
                operation: Some(crate::practice::Operation::DisableAreaEffect),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "area-inspection",
        prerequisites: &[],
        steps: &[Step {
            slug: "step-area-inspection-inspect",
            subject: "area-inspection",
            operation: Some(crate::practice::Operation::InspectArea),
        }],
    },
    crate::practice::Lesson {
        subject: "learn-record",
        prerequisites: &[],
        steps: &[Step {
            slug: "step-learn-record-sample",
            subject: "learn-record",
            operation: Some(crate::practice::Operation::ReadRecord),
        }],
    },
    crate::practice::Lesson {
        subject: "learn-assertion",
        prerequisites: &[],
        steps: &[Step {
            slug: "step-learn-assertion-sample",
            subject: "learn-assertion",
            operation: Some(crate::practice::Operation::ApplyAssertion),
        }],
    },
    crate::practice::Lesson {
        subject: "learn-ontology",
        prerequisites: &[],
        steps: &[Step {
            slug: "step-learn-ontology-inspect",
            subject: "learn-ontology",
            operation: Some(crate::practice::Operation::ReadVocabulary),
        }],
    },
    crate::practice::Lesson {
        subject: "record-castles",
        prerequisites: &[],
        steps: &[Step {
            slug: "step-record-castles-open",
            subject: "record-castles",
            operation: Some(crate::practice::Operation::OpenRecordViews),
        }],
    },
    crate::practice::Lesson {
        subject: "facts",
        prerequisites: &[],
        steps: &[Step {
            slug: "step-facts-inspect",
            subject: "facts",
            operation: Some(crate::practice::Operation::ReadFacts),
        }],
    },
    crate::practice::Lesson {
        subject: "protein",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "protein",
            subject: "protein",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "protein-presentation",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "protein-presentation",
            subject: "protein-presentation",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "area-record-actions",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "area-record-actions",
            subject: "area-record-actions",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "protein-arrangement",
        prerequisites: &[],
        steps: &[Step {
            slug: "step-protein-arrangement-arrange",
            subject: "protein-arrangement",
            operation: Some(crate::practice::Operation::ArrangeProtein),
        }],
    },
    crate::practice::Lesson {
        subject: "todo",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-todo-complete",
                subject: "todo",
                operation: Some(crate::practice::Operation::CompleteTask),
            },
            Step {
                slug: "step-todo-undo",
                subject: "todo",
                operation: Some(crate::practice::Operation::UndoTask),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "kanban",
        prerequisites: &[],
        steps: &[Step {
            slug: "step-kanban-move",
            subject: "kanban",
            operation: Some(crate::practice::Operation::MoveTask),
        }],
    },
    crate::practice::Lesson {
        subject: "time",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-time-start",
                subject: "time",
                operation: Some(crate::practice::Operation::StartTimer),
            },
            Step {
                slug: "step-time-stop",
                subject: "time",
                operation: Some(crate::practice::Operation::StopTimer),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "calendar",
        prerequisites: &[],
        steps: &[Step {
            slug: "step-calendar-open",
            subject: "calendar",
            operation: Some(crate::practice::Operation::OpenDatedRecord),
        }],
    },
    crate::practice::Lesson {
        subject: "operation",
        prerequisites: &[],
        steps: &[Step {
            slug: "step-operation-activate",
            subject: "operation",
            operation: Some(crate::practice::Operation::SetOperation),
        }],
    },
    crate::practice::Lesson {
        subject: "notifications",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-notifications-open",
                subject: "notifications",
                operation: Some(crate::practice::Operation::ShowNotice),
            },
            Step {
                slug: "step-notifications-dismiss",
                subject: "notifications",
                operation: Some(crate::practice::Operation::DismissNotice),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "frequency",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "step-frequency-create",
            subject: "frequency",
            operation: Some(crate::practice::Operation::CreateFrequency),
        }],
    },
    crate::practice::Lesson {
        subject: "learn-karma",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-karma-preview",
                subject: "learn-karma",
                operation: Some(crate::practice::Operation::PreviewRule),
            },
            Step {
                slug: "step-karma-run",
                subject: "learn-karma",
                operation: Some(crate::practice::Operation::RunRule),
            },
            Step {
                slug: "step-karma-pause",
                subject: "learn-karma",
                operation: Some(crate::practice::Operation::PauseRule),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "habits",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-habits-preview",
                subject: "habits",
                operation: Some(crate::practice::Operation::PreviewHabit),
            },
            Step {
                slug: "step-habits-import",
                subject: "habits",
                operation: Some(crate::practice::Operation::ImportHabit),
            },
            Step {
                slug: "step-habits-complete",
                subject: "habits",
                operation: Some(crate::practice::Operation::CompleteHabit),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "commands",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-commands-save",
                subject: "commands",
                operation: Some(crate::practice::Operation::SaveCommand),
            },
            Step {
                slug: "step-commands-run",
                subject: "commands",
                operation: Some(crate::practice::Operation::RunCommand),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "simulation",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-simulation-event",
                subject: "simulation",
                operation: Some(crate::practice::Operation::StepSimulation),
            },
            Step {
                slug: "step-simulation-stop",
                subject: "simulation",
                operation: Some(crate::practice::Operation::StopSimulation),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "learn-fiote",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "step-fiote-inspect",
            subject: "learn-fiote",
            operation: Some(crate::practice::Operation::InspectFiote),
        }],
    },
    crate::practice::Lesson {
        subject: "learn-organ",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-organ-identity",
                subject: "learn-organ",
                operation: Some(crate::practice::Operation::InspectOrgan),
            },
            Step {
                slug: "step-organ-context",
                subject: "learn-organ",
                operation: Some(crate::practice::Operation::SwitchOrgan),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "contacts",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "step-contacts-pair",
            subject: "contacts",
            operation: Some(crate::practice::Operation::PairContact),
        }],
    },
    crate::practice::Lesson {
        subject: "access-control",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "step-access-control-inspect",
            subject: "access-control",
            operation: Some(crate::practice::Operation::InspectAccess),
        }],
    },
    crate::practice::Lesson {
        subject: "organ-sync",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "step-organ-sync-share",
            subject: "organ-sync",
            operation: Some(crate::practice::Operation::ShareRecords),
        }],
    },
    crate::practice::Lesson {
        subject: "devices",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "step-devices-inspect",
            subject: "devices",
            operation: Some(crate::practice::Operation::InspectDevices),
        }],
    },
    crate::practice::Lesson {
        subject: "mail",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "step-mail-inspect",
            subject: "mail",
            operation: Some(crate::practice::Operation::InspectMail),
        }],
    },
    crate::practice::Lesson {
        subject: "discovery",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-discovery-search",
                subject: "discovery",
                operation: Some(crate::practice::Operation::SearchDiscovery),
            },
            Step {
                slug: "step-discovery-draft",
                subject: "discovery",
                operation: Some(crate::practice::Operation::PrepareAnnouncement),
            },
            Step {
                slug: "step-discovery-request",
                subject: "discovery",
                operation: Some(crate::practice::Operation::OpenDiscoveryRequest),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "conversations",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "step-conversations-send",
            subject: "conversations",
            operation: Some(crate::practice::Operation::SendPracticeMessage),
        }],
    },
    crate::practice::Lesson {
        subject: "calls",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "step-calls-inspect",
            subject: "calls",
            operation: Some(crate::practice::Operation::InspectCalls),
        }],
    },
    crate::practice::Lesson {
        subject: "learn-transfer",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-transfer-check",
                subject: "learn-transfer",
                operation: Some(crate::practice::Operation::CheckTransfer),
            },
            Step {
                slug: "step-transfer-agree",
                subject: "learn-transfer",
                operation: Some(crate::practice::Operation::AgreeTransfer),
            },
            Step {
                slug: "step-transfer-activate",
                subject: "learn-transfer",
                operation: Some(crate::practice::Operation::ActivateTransfer),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "transfer-automation",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "step-transfer-automation-pause",
            subject: "transfer-automation",
            operation: Some(crate::practice::Operation::PauseTransferRule),
        }],
    },
    crate::practice::Lesson {
        subject: "instinct-import",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-import-conflict",
                subject: "instinct-import",
                operation: Some(crate::practice::Operation::PreviewImportConflict),
            },
            Step {
                slug: "step-import-cancel",
                subject: "instinct-import",
                operation: Some(crate::practice::Operation::CancelImportPreview),
            },
            Step {
                slug: "step-import-clean",
                subject: "instinct-import",
                operation: Some(crate::practice::Operation::PreviewCleanImport),
            },
            Step {
                slug: "step-import-commit",
                subject: "instinct-import",
                operation: Some(crate::practice::Operation::ImportPreparedRecords),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "learn-sync",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-sync-export",
                subject: "learn-sync",
                operation: Some(crate::practice::Operation::ExportPracticeFiles),
            },
            Step {
                slug: "step-sync-incoming",
                subject: "learn-sync",
                operation: Some(crate::practice::Operation::EditPracticeFile),
            },
            Step {
                slug: "step-sync-stop",
                subject: "learn-sync",
                operation: Some(crate::practice::Operation::StopPracticeSync),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "blob-sync",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-blob-send",
                subject: "blob-sync",
                operation: Some(crate::practice::Operation::SendPracticeCopy),
            },
            Step {
                slug: "step-blob-accept",
                subject: "blob-sync",
                operation: Some(crate::practice::Operation::AcceptPracticeCopy),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "backup",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "step-backup-inspect",
            subject: "backup",
            operation: Some(crate::practice::Operation::InspectBackup),
        }],
    },
    crate::practice::Lesson {
        subject: "ide",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-ide-open",
                subject: "ide",
                operation: Some(crate::practice::Operation::OpenPracticeFile),
            },
            Step {
                slug: "step-ide-save",
                subject: "ide",
                operation: Some(crate::practice::Operation::EditSavePracticeFile),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "language-tools",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "step-language-tools-inspect",
            subject: "language-tools",
            operation: Some(crate::practice::Operation::InspectLanguageTools),
        }],
    },
    crate::practice::Lesson {
        subject: "documents",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "step-document-next",
            subject: "documents",
            operation: Some(crate::practice::Operation::NextDocumentPage),
        }],
    },
    crate::practice::Lesson {
        subject: "terminal",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "step-terminal-inspect",
            subject: "terminal",
            operation: Some(crate::practice::Operation::InspectTerminal),
        }],
    },
    crate::practice::Lesson {
        subject: "external-files",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-file-choices",
                subject: "external-files",
                operation: Some(crate::practice::Operation::InspectFileChoices),
            },
            Step {
                slug: "step-file-cancel",
                subject: "external-files",
                operation: Some(crate::practice::Operation::CancelFileChoices),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "recorder",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-recorder-preview",
                subject: "recorder",
                operation: Some(crate::practice::Operation::PreviewRecording),
            },
            Step {
                slug: "step-recorder-stop",
                subject: "recorder",
                operation: Some(crate::practice::Operation::StopRecordingPlayback),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "area-sound",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-area-sound-enable",
                subject: "area-sound",
                operation: Some(crate::practice::Operation::EnableAreaSound),
            },
            Step {
                slug: "step-area-sound-assign",
                subject: "area-sound",
                operation: Some(crate::practice::Operation::AssignAreaSound),
            },
            Step {
                slug: "step-area-sound-preview",
                subject: "area-sound",
                operation: Some(crate::practice::Operation::PreviewAreaSound),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "shaders",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "step-shader-example",
            subject: "shaders",
            operation: Some(crate::practice::Operation::AddShaderExample),
        }],
    },
    crate::practice::Lesson {
        subject: "topology",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-topology-spatial",
                subject: "topology",
                operation: Some(crate::practice::Operation::SwitchSpatialView),
            },
            Step {
                slug: "step-topology-flat",
                subject: "topology",
                operation: Some(crate::practice::Operation::ReturnFlatView),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "custom-castles",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-custom-save",
                subject: "custom-castles",
                operation: Some(crate::practice::Operation::SaveCustomCastle),
            },
            Step {
                slug: "step-custom-add",
                subject: "custom-castles",
                operation: Some(crate::practice::Operation::AddCustomCastle),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "freedoom",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "freedoom",
            subject: "freedoom",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "inspection",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "step-inspection-control",
            subject: "inspection",
            operation: Some(crate::practice::Operation::InspectControl),
        }],
    },
    crate::practice::Lesson {
        subject: "information",
        prerequisites: &[],
        steps: &[
            Step {
                slug: "step-information-inspect",
                subject: "information",
                operation: Some(crate::practice::Operation::InspectInformation),
            },
            Step {
                slug: "step-information-credits",
                subject: "information",
                operation: Some(crate::practice::Operation::InspectSandCredits),
            },
        ],
    },
    crate::practice::Lesson {
        subject: "laboratory",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "step-laboratory-resources",
            subject: "laboratory",
            operation: Some(crate::practice::Operation::InspectLaboratory),
        }],
    },
];
