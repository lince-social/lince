use crate::practice::Step;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Page {
    pub slug: &'static str,
    pub chapter: &'static str,
    pub reference: Option<&'static str>,
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
            slug: "frequency",
            subject: "frequency",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "learn-karma",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "learn-karma",
            subject: "learn-karma",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "habits",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "habits",
            subject: "habits",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "commands",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "commands",
            subject: "commands",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "simulation",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "simulation",
            subject: "simulation",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "learn-fiote",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "learn-fiote",
            subject: "learn-fiote",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "learn-organ",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "learn-organ",
            subject: "learn-organ",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "contacts",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "contacts",
            subject: "contacts",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "access-control",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "access-control",
            subject: "access-control",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "organ-sync",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "organ-sync",
            subject: "organ-sync",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "devices",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "devices",
            subject: "devices",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "mail",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "mail",
            subject: "mail",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "discovery",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "discovery",
            subject: "discovery",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "conversations",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "conversations",
            subject: "conversations",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "calls",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "calls",
            subject: "calls",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "learn-transfer",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "learn-transfer",
            subject: "learn-transfer",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "transfer-automation",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "transfer-automation",
            subject: "transfer-automation",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "instinct-import",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "instinct-import",
            subject: "instinct-import",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "learn-sync",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "learn-sync",
            subject: "learn-sync",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "blob-sync",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "blob-sync",
            subject: "blob-sync",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "backup",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "backup",
            subject: "backup",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "ide",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "ide",
            subject: "ide",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "language-tools",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "language-tools",
            subject: "language-tools",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "documents",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "documents",
            subject: "documents",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "terminal",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "terminal",
            subject: "terminal",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "external-files",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "external-files",
            subject: "external-files",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "recorder",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "recorder",
            subject: "recorder",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "area-sound",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "area-sound",
            subject: "area-sound",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "shaders",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "shaders",
            subject: "shaders",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "topology",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "topology",
            subject: "topology",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "custom-castles",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "custom-castles",
            subject: "custom-castles",
            operation: None,
        }],
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
            slug: "inspection",
            subject: "inspection",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "information",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "information",
            subject: "information",
            operation: None,
        }],
    },
    crate::practice::Lesson {
        subject: "laboratory",
        prerequisites: &[],
        steps: &[crate::practice::Step {
            slug: "laboratory",
            subject: "laboratory",
            operation: None,
        }],
    },
];
