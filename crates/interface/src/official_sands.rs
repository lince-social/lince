use crate::{
    composition::{
        DefinitionCatalog, DefinitionLineage, DefinitionOrigin, DefinitionRevisionRecord,
    },
    configuration::configuration_sand_package,
    sand::{
        AccessibilityRole, AccessibilitySpec, BehaviorBinding, ConfigurationField,
        DeclarativeBehavior, DefinitionChild, DefinitionRef, ExportedPort, InputPort, Isolation,
        OutputPort, PortDirection, ProjectionKind, ProjectionManifest, RendererCapability,
        SandCapability, SandDefinition, SandElement, SandPackage, SandValue, Transform2d,
        ValueType,
    },
    style::StyleLayer,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub const OFFICIAL_SAND_COUNT: usize = 25;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OfficialMigrationState {
    Landed,
    NativeBehavior,
    RustStructure,
}

impl OfficialMigrationState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Landed => "landed",
            Self::NativeBehavior => {
                "native retained runtime and Behavior landed; domain coverage is Sand-specific"
            }
            Self::RustStructure => "Rust structure cataloged; runtime Behavior pending",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct OfficialSandSpec {
    pub uid: &'static str,
    pub display_name: &'static str,
    pub legacy_source: &'static str,
    pub purpose: &'static str,
    pub state: OfficialMigrationState,
}

pub const OFFICIAL_SANDS: [OfficialSandSpec; OFFICIAL_SAND_COUNT] = [
    OfficialSandSpec {
        uid: "shell-edit",
        display_name: "Edit controls",
        legacy_source: "sand/shell.rs::edit_source",
        purpose: "Select, compose, lock, save and inspect Sands",
        state: OfficialMigrationState::NativeBehavior,
    },
    OfficialSandSpec {
        uid: "shell-zoom",
        display_name: "Zoom controls",
        legacy_source: "sand/shell.rs::zoom_source",
        purpose: "Navigate Box scale and recenter the workspace",
        state: OfficialMigrationState::NativeBehavior,
    },
    OfficialSandSpec {
        uid: "shell-ai",
        display_name: "AI builder",
        legacy_source: "sand/shell.rs::ai_source",
        purpose: "Describe and inspect proposed Sand composition",
        state: OfficialMigrationState::RustStructure,
    },
    OfficialSandSpec {
        uid: "instinct",
        display_name: "Instinct",
        legacy_source: "sand/instinct",
        purpose: "Read tutorial material and materialize selected Records",
        state: OfficialMigrationState::RustStructure,
    },
    OfficialSandSpec {
        uid: "document-viewer",
        display_name: "Document Viewer",
        legacy_source: "sand/document_viewer",
        purpose: "Read supported documents without turning pixels into Box truth",
        state: OfficialMigrationState::RustStructure,
    },
    OfficialSandSpec {
        uid: "freedoom-portal",
        display_name: "Freedoom Portal",
        legacy_source: "sand/freedoom",
        purpose: "Host the game renderer behind ordinary Sand controls",
        state: OfficialMigrationState::RustStructure,
    },
    OfficialSandSpec {
        uid: "lince-logo-led",
        display_name: "Lince Logo LED",
        legacy_source: "sand/lince_logo_led",
        purpose: "Drive the specialized LED visualization",
        state: OfficialMigrationState::RustStructure,
    },
    OfficialSandSpec {
        uid: "lince-website",
        display_name: "Lince Website",
        legacy_source: "sand/lince_website",
        purpose: "Display the Website with explicit zero Lince authority",
        state: OfficialMigrationState::RustStructure,
    },
    OfficialSandSpec {
        uid: "terminal",
        display_name: "Terminal",
        legacy_source: "sand/terminal",
        purpose: "Run a terminal surface with visible session controls",
        state: OfficialMigrationState::RustStructure,
    },
    OfficialSandSpec {
        uid: "kanban",
        display_name: "Kanban",
        legacy_source: "sand/kanban",
        purpose: "Compose Protein-backed columns from reusable Record summaries",
        state: OfficialMigrationState::NativeBehavior,
    },
    OfficialSandSpec {
        uid: "relations",
        display_name: "Relations",
        legacy_source: "sand/relations",
        purpose: "Explore assertion graphs with separate controls and inspector",
        state: OfficialMigrationState::RustStructure,
    },
    OfficialSandSpec {
        uid: "ontology",
        display_name: "Ontology",
        legacy_source: "sand/ontology",
        purpose: "Explore Concepts, assertions and their Record ownership",
        state: OfficialMigrationState::RustStructure,
    },
    OfficialSandSpec {
        uid: "record",
        display_name: "Record",
        legacy_source: "sand/record",
        purpose: "Inspect one Record through independently selectable properties",
        state: OfficialMigrationState::NativeBehavior,
    },
    OfficialSandSpec {
        uid: "organ",
        display_name: "Organ",
        legacy_source: "sand/organ",
        purpose: "Inspect an Organ, its members and reachable actions",
        state: OfficialMigrationState::RustStructure,
    },
    OfficialSandSpec {
        uid: "conversation",
        display_name: "Conversation",
        legacy_source: "sand/conversation",
        purpose: "Read live Messages and manage private drafts and send timing",
        state: OfficialMigrationState::NativeBehavior,
    },
    OfficialSandSpec {
        uid: "record-editor",
        display_name: "Record Editor",
        legacy_source: "sand/record_editor",
        purpose: "Edit typed Record properties through reusable fields",
        state: OfficialMigrationState::RustStructure,
    },
    OfficialSandSpec {
        uid: "table",
        display_name: "Table",
        legacy_source: "sand/table",
        purpose: "Project Protein rows through reusable row and property Sands",
        state: OfficialMigrationState::NativeBehavior,
    },
    OfficialSandSpec {
        uid: "permissions",
        display_name: "Roles and Permissions",
        legacy_source: "sand/permissions",
        purpose: "Review and request typed authority changes",
        state: OfficialMigrationState::RustStructure,
    },
    OfficialSandSpec {
        uid: "todo",
        display_name: "Todo",
        legacy_source: "sand/todo",
        purpose: "Compose a focused task list from Record summaries",
        state: OfficialMigrationState::NativeBehavior,
    },
    OfficialSandSpec {
        uid: "transfer",
        display_name: "Transfer",
        legacy_source: "sand/transfer",
        purpose: "Inspect and act on Transfer participants and quantities",
        state: OfficialMigrationState::RustStructure,
    },
    OfficialSandSpec {
        uid: "karma",
        display_name: "Karma",
        legacy_source: "sand/karma",
        purpose: "Build and inspect rules around a specialized causal view",
        state: OfficialMigrationState::RustStructure,
    },
    OfficialSandSpec {
        uid: "archive",
        display_name: "Archive",
        legacy_source: "sand/archive",
        purpose: "Inspect and export selected workspace material",
        state: OfficialMigrationState::RustStructure,
    },
    OfficialSandSpec {
        uid: "communication",
        display_name: "Communication",
        legacy_source: "sand/communication",
        purpose: "Inspect contacts, threads and live communication state",
        state: OfficialMigrationState::RustStructure,
    },
    OfficialSandSpec {
        uid: "configuration",
        display_name: "Configuration",
        legacy_source: "interface/configuration.rs",
        purpose: "Edit inherited visual, Behavior and capability configuration",
        state: OfficialMigrationState::Landed,
    },
    OfficialSandSpec {
        uid: "sand-publisher",
        display_name: "Sand Publisher",
        legacy_source: "sand/sand_publisher",
        purpose: "Inspect and publish a validated Sand package",
        state: OfficialMigrationState::RustStructure,
    },
];

pub fn official_sand_package() -> SandPackage {
    let mut package = configuration_sand_package();
    package.uid = "lince-official-sands".into();
    for definition in shared_definitions() {
        package
            .graph
            .definitions
            .insert(definition.uid.clone(), definition);
    }
    for spec in OFFICIAL_SANDS {
        if spec.uid != "configuration" {
            let definition = official_definition(spec);
            package
                .graph
                .definitions
                .insert(definition.uid.clone(), definition);
        }
    }
    package
}

pub fn official_sand_catalog() -> Result<DefinitionCatalog, String> {
    let package = official_sand_package();
    package.validate().map_err(|error| error.to_string())?;
    DefinitionCatalog::from_records(package.graph.definitions.into_values().map(|definition| {
        let uid = definition.uid.clone();
        DefinitionRevisionRecord {
            definition,
            origin: DefinitionOrigin::Rust,
            lineage: DefinitionLineage::CodeOwned {
                constructor: format!("interface::official_sands::{uid}"),
            },
        }
    }))
    .map_err(|error| error.to_string())
}

#[derive(Clone, Debug)]
pub struct OfficialSandGalleryState {
    selected: usize,
    package: SandPackage,
}

impl OfficialSandGalleryState {
    pub fn new() -> Result<Self, String> {
        let package = official_sand_package();
        package.validate().map_err(|error| error.to_string())?;
        Ok(Self {
            selected: 0,
            package,
        })
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected(&self) -> OfficialSandSpec {
        OFFICIAL_SANDS[self.selected]
    }

    pub fn move_focus(&mut self, backwards: bool) {
        self.selected = if backwards {
            self.selected
                .checked_sub(1)
                .unwrap_or(OFFICIAL_SANDS.len() - 1)
        } else {
            (self.selected + 1) % OFFICIAL_SANDS.len()
        };
    }

    pub fn focus_at(&mut self, index: usize) {
        self.selected = index.min(OFFICIAL_SANDS.len() - 1);
    }

    pub fn package(&self) -> &SandPackage {
        &self.package
    }

    pub fn tree_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        self.append_tree(self.selected().uid, "", &mut lines);
        lines
    }

    pub fn boundary_lines(&self) -> Vec<String> {
        let definition = &self.package.graph.definitions[self.selected().uid];
        let inputs = definition
            .inputs
            .iter()
            .map(|port| format!("IN  {}: {:?}", port.name, port.value_type))
            .collect::<Vec<_>>();
        let outputs = definition
            .outputs
            .iter()
            .map(|port| format!("OUT {}: {:?}", port.name, port.value_type))
            .collect::<Vec<_>>();
        inputs.into_iter().chain(outputs).collect()
    }

    fn append_tree(&self, uid: &str, indent: &str, lines: &mut Vec<String>) {
        let definition = &self.package.graph.definitions[uid];
        lines.push(format!(
            "{indent}{} · {:?} · r{}",
            definition.display_name, definition.element, definition.revision
        ));
        let next = format!("{indent}  ");
        for child in &definition.children {
            lines.push(format!("{next}{}", child.local_uid));
            self.append_tree(&child.definition.uid, &format!("{next}  "), lines);
        }
    }
}

fn shared_definitions() -> Vec<SandDefinition> {
    let mut definitions = vec![
        trigger_definition(),
        action_definition(),
        record_action_definition(),
        record_identity_definition(),
        property_definition(),
        labeled_field_definition("official-labeled-field", "Labeled field", "field"),
        labeled_field_definition("official-labeled-textarea", "Labeled textarea", "textarea"),
        status_definition(),
        toolbar_definition(),
        empty_state_definition(),
        record_summary_definition(),
        message_definition(),
        draft_entry_definition(),
        conversation_thread_definition(),
        draft_queue_definition(),
        kanban_column_definition(),
        table_row_definition(),
        inspector_definition(),
    ];
    definitions.extend([
        specialized_definition(
            "official-document-surface",
            "Document surface",
            ProjectionKind::NativeRetained,
        ),
        specialized_definition(
            "official-game-surface",
            "Game surface",
            ProjectionKind::NativeWorld,
        ),
        specialized_definition(
            "official-led-surface",
            "LED surface",
            ProjectionKind::NativeWorld,
        ),
        specialized_definition(
            "official-website-surface",
            "Website surface",
            ProjectionKind::Website,
        ),
        specialized_definition(
            "official-terminal-surface",
            "Terminal surface",
            ProjectionKind::NativeRetained,
        ),
        specialized_definition(
            "official-graph-surface",
            "Graph surface",
            ProjectionKind::NativeWorld,
        ),
        specialized_definition(
            "official-ontology-surface",
            "Ontology surface",
            ProjectionKind::NativeWorld,
        ),
        specialized_definition(
            "official-karma-surface",
            "Karma surface",
            ProjectionKind::NativeWorld,
        ),
    ]);
    definitions
}

fn record_identity_definition() -> SandDefinition {
    SandDefinition {
        uid: "official-record-identity".into(),
        revision: 1,
        display_name: "Record identity".into(),
        element: SandElement::Badge,
        inputs: vec![record_input("record", "unbound-record")],
        outputs: vec![record_output("selected")],
        children: Vec::new(),
        connections: Vec::new(),
        exports: Vec::new(),
        behaviors: Vec::new(),
        configuration: Vec::new(),
        style: StyleLayer::default(),
        accessibility: AccessibilitySpec {
            role: AccessibilityRole::Button,
            label: "Record identity".into(),
            description: Some("A typed Record reference that can be selected".into()),
            live: false,
        },
        capabilities: BTreeSet::new(),
        projections: vec![ProjectionManifest {
            key: "native-retained".into(),
            kind: ProjectionKind::NativeRetained,
            isolation: Isolation::Trusted,
            required: BTreeSet::from([
                RendererCapability::RetainedControls,
                RendererCapability::Accessibility,
            ]),
            capabilities: BTreeSet::new(),
            assets: Vec::new(),
            projected_nodes: BTreeSet::from(["root".into()]),
        }],
    }
}

fn action_definition() -> SandDefinition {
    let children = vec![child(
        "control",
        "official-trigger",
        0.0,
        0.0,
        160.0,
        28.0,
        0,
    )];
    compound(
        "official-action",
        "Action",
        children,
        vec![
            text_input("label", "Action"),
            text_input("description", "Run this Action"),
        ],
        vec![boolean_output("pressed")],
        vec![
            input_export("label", "control", "label"),
            input_export("description", "control", "description"),
            output_export("pressed", "control", "pressed"),
        ],
        Vec::new(),
        "One independently placeable action control",
    )
}

fn trigger_definition() -> SandDefinition {
    SandDefinition {
        uid: "official-trigger".into(),
        revision: 1,
        display_name: "Button".into(),
        element: SandElement::Button,
        inputs: vec![
            text_input("label", "Action"),
            text_input("description", "Run this Action"),
        ],
        outputs: vec![boolean_output("pressed")],
        children: Vec::new(),
        connections: Vec::new(),
        exports: Vec::new(),
        behaviors: Vec::new(),
        configuration: Vec::new(),
        style: StyleLayer::default(),
        accessibility: AccessibilitySpec {
            role: AccessibilityRole::Button,
            label: "Button".into(),
            description: Some("A generic trigger with no implicit domain event".into()),
            live: false,
        },
        capabilities: BTreeSet::new(),
        projections: vec![ProjectionManifest {
            key: "native-retained".into(),
            kind: ProjectionKind::NativeRetained,
            isolation: Isolation::Trusted,
            required: BTreeSet::from([
                RendererCapability::RetainedControls,
                RendererCapability::Accessibility,
            ]),
            capabilities: BTreeSet::new(),
            assets: Vec::new(),
            projected_nodes: BTreeSet::from(["root".into()]),
        }],
    }
}

fn record_action_definition() -> SandDefinition {
    compound(
        "official-record-action",
        "Record action",
        vec![child("control", "button", 0.0, 0.0, 160.0, 28.0, 0)],
        vec![
            text_input("label", "Open Record"),
            text_input("description", "Open this Record"),
            record_input("record", "unbound-record"),
        ],
        vec![record_output("pressed")],
        vec![
            input_export("label", "control", "label"),
            input_export("description", "control", "description"),
            input_export("record", "control", "record"),
            output_export("pressed", "control", "pressed"),
        ],
        Vec::new(),
        "A Record-typed action that keeps record-clicked out of generic buttons",
    )
}

fn property_definition() -> SandDefinition {
    compound(
        "official-property",
        "Record property",
        vec![
            child("label", "text", 0.0, 0.0, 110.0, 24.0, 0),
            child("value", "text", 114.0, 0.0, 226.0, 24.0, 1),
        ],
        vec![
            text_input("label", "Property"),
            text_input("value", "Unset"),
        ],
        Vec::new(),
        vec![
            input_export("label", "label", "value"),
            input_export("value", "value", "value"),
        ],
        Vec::new(),
        "One visible property that can be added to or removed from a composition",
    )
}

fn labeled_field_definition(uid: &str, name: &str, control: &str) -> SandDefinition {
    compound(
        uid,
        name,
        vec![
            child("label", "text", 0.0, 0.0, 320.0, 20.0, 0),
            child("control", control, 0.0, 24.0, 320.0, 32.0, 1),
            child("error", "validation-message", 0.0, 60.0, 320.0, 20.0, 2),
        ],
        vec![
            text_input("label", "Field"),
            text_input("value", ""),
            text_input("error", ""),
        ],
        vec![text_output("changed")],
        vec![
            input_export("label", "label", "value"),
            input_export("value", "control", "value"),
            input_export("error", "error", "value"),
            output_export("changed", "control", "changed"),
        ],
        Vec::new(),
        "A label, editable value and associated validation state",
    )
}

fn status_definition() -> SandDefinition {
    compound(
        "official-status",
        "Status line",
        vec![
            child("state", "badge", 0.0, 0.0, 90.0, 24.0, 0),
            child("detail", "text", 94.0, 0.0, 246.0, 24.0, 1),
        ],
        vec![
            text_input("state", "Ready"),
            text_input("detail", "No pending work"),
        ],
        Vec::new(),
        vec![
            input_export("state", "state", "value"),
            input_export("detail", "detail", "value"),
        ],
        Vec::new(),
        "A non-color-only state and its explanation",
    )
}

fn toolbar_definition() -> SandDefinition {
    compound(
        "official-toolbar",
        "Toolbar",
        vec![
            child("primary", "official-action", 0.0, 0.0, 104.0, 28.0, 0),
            child("secondary", "official-action", 108.0, 0.0, 104.0, 28.0, 1),
            child("tertiary", "official-action", 216.0, 0.0, 104.0, 28.0, 2),
        ],
        vec![
            text_input("primary-label", "Primary"),
            text_input("secondary-label", "Secondary"),
            text_input("tertiary-label", "Tertiary"),
        ],
        vec![
            boolean_output("primary"),
            boolean_output("secondary"),
            boolean_output("tertiary"),
        ],
        vec![
            input_export("primary-label", "primary", "label"),
            input_export("secondary-label", "secondary", "label"),
            input_export("tertiary-label", "tertiary", "label"),
            output_export("primary", "primary", "pressed"),
            output_export("secondary", "secondary", "pressed"),
            output_export("tertiary", "tertiary", "pressed"),
        ],
        Vec::new(),
        "Three ordinary Action Sands arranged as one reusable toolbar",
    )
}

fn empty_state_definition() -> SandDefinition {
    compound(
        "official-empty-state",
        "Empty state",
        vec![
            child("surface", "panel", 0.0, 0.0, 340.0, 130.0, 0),
            child("title", "title", 16.0, 14.0, 308.0, 24.0, 1),
            child("detail", "text", 16.0, 44.0, 308.0, 36.0, 2),
            child("action", "official-action", 16.0, 88.0, 160.0, 28.0, 3),
        ],
        vec![
            text_input("title", "Nothing here"),
            text_input("detail", "No results yet"),
        ],
        vec![boolean_output("action")],
        vec![
            input_export("title", "title", "value"),
            input_export("detail", "detail", "value"),
            output_export("action", "action", "pressed"),
        ],
        Vec::new(),
        "An honest empty result and its reachable next action",
    )
}

fn record_summary_definition() -> SandDefinition {
    compound(
        "official-record-summary",
        "Record summary",
        vec![
            child("surface", "card", 0.0, 0.0, 340.0, 156.0, 0),
            child("title", "title", 14.0, 12.0, 250.0, 24.0, 1),
            child("description", "text", 14.0, 42.0, 312.0, 46.0, 2),
            child("quantity", "quantity", 14.0, 96.0, 90.0, 24.0, 3),
            child("state", "badge", 112.0, 96.0, 92.0, 24.0, 4),
            child(
                "open",
                "official-record-action",
                212.0,
                96.0,
                114.0,
                28.0,
                5,
            ),
        ],
        vec![
            text_input("title", "Untitled Record"),
            text_input("description", "No description"),
            number_input("quantity", 0.0),
            text_input("state", "neutral"),
            record_input("record", "unbound-record"),
            text_input("open-label", "Open Record"),
            text_input("open-description", "Open this Record"),
        ],
        vec![record_output("record-clicked")],
        vec![
            input_export("title", "title", "value"),
            input_export("description", "description", "value"),
            input_export("quantity", "quantity", "value"),
            input_export("state", "state", "value"),
            input_export("record", "open", "record"),
            input_export("open-label", "open", "label"),
            input_export("open-description", "open", "description"),
            output_export("record-clicked", "open", "pressed"),
        ],
        Vec::new(),
        "A configurable title, description, quantity, state and Record action",
    )
}

fn message_definition() -> SandDefinition {
    compound(
        "official-message",
        "Message",
        vec![
            child("surface", "card", 0.0, 0.0, 340.0, 112.0, 0),
            child(
                "author",
                "official-record-identity",
                12.0,
                10.0,
                150.0,
                20.0,
                1,
            ),
            child(
                "operator",
                "official-record-identity",
                170.0,
                10.0,
                104.0,
                20.0,
                2,
            ),
            child("state", "badge", 282.0, 10.0, 46.0, 20.0, 3),
            child("content", "text", 12.0, 38.0, 316.0, 58.0, 4),
        ],
        vec![
            record_input("author", "unknown-author"),
            record_input("operator", "unknown-operator"),
            text_input("content", "Empty message"),
            text_input("state", "finished"),
        ],
        Vec::new(),
        vec![
            input_export("author", "author", "record"),
            input_export("operator", "operator", "record"),
            input_export("content", "content", "value"),
            input_export("state", "state", "value"),
        ],
        Vec::new(),
        "One authored Message with delegated operator and explicit live state",
    )
}

fn draft_entry_definition() -> SandDefinition {
    compound(
        "official-draft-entry",
        "Private draft",
        vec![
            child("pinned", "checkbox", 0.0, 0.0, 24.0, 24.0, 0),
            child("content", "textarea", 30.0, 0.0, 310.0, 48.0, 1),
            child("age", "badge", 30.0, 54.0, 92.0, 24.0, 2),
            child("timing", "badge", 128.0, 54.0, 112.0, 24.0, 3),
            child("send", "official-action", 0.0, 86.0, 108.0, 28.0, 4),
            child(
                "timing-action",
                "official-action",
                116.0,
                86.0,
                108.0,
                28.0,
                5,
            ),
            child("move-up", "official-action", 232.0, 86.0, 108.0, 28.0, 6),
            child("delete", "official-action", 232.0, 122.0, 108.0, 28.0, 7),
        ],
        vec![
            boolean_input("pinned", false),
            text_input("content", "Draft"),
            text_input("age", "now"),
            text_input("send-timing", "manual"),
            record_input("draft", "unbound-draft"),
            text_input("send-label", "Send now"),
            text_input("timing-label", "Change timing"),
            text_input("move-label", "Move up"),
            text_input("delete-label", "Delete"),
        ],
        vec![
            boolean_output("send"),
            boolean_output("change-timing"),
            boolean_output("move-up"),
            boolean_output("delete"),
        ],
        vec![
            input_export("pinned", "pinned", "value"),
            input_export("content", "content", "value"),
            input_export("age", "age", "value"),
            input_export("send-timing", "timing", "value"),
            input_export("send-label", "send", "label"),
            input_export("timing-label", "timing-action", "label"),
            input_export("move-label", "move-up", "label"),
            input_export("delete-label", "delete", "label"),
            output_export("send", "send", "pressed"),
            output_export("change-timing", "timing-action", "pressed"),
            output_export("move-up", "move-up", "pressed"),
            output_export("delete", "delete", "pressed"),
        ],
        Vec::new(),
        "An author-private draft with pin, age, order and declared send timing",
    )
}

fn conversation_thread_definition() -> SandDefinition {
    compound(
        "official-conversation-thread",
        "Conversation thread",
        vec![
            child("title", "title", 0.0, 0.0, 340.0, 26.0, 0),
            child("message", "official-message", 0.0, 32.0, 340.0, 112.0, 1),
            child("composer", "textarea", 0.0, 152.0, 340.0, 74.0, 2),
            child("send", "official-action", 220.0, 234.0, 120.0, 28.0, 3),
        ],
        vec![
            record_input("conversation", "unbound-conversation"),
            text_input("title", "Conversation"),
            record_input("author", "unknown-author"),
            record_input("operator", "unknown-operator"),
            text_input("message-content", "Nothing said yet"),
            text_input("message-state", "finished"),
            text_input("draft", ""),
            text_input("send-label", "Send"),
            text_input("send-description", "Send this Message"),
        ],
        vec![boolean_output("send")],
        vec![
            input_export("title", "title", "value"),
            input_export("author", "message", "author"),
            input_export("operator", "message", "operator"),
            input_export("message-content", "message", "content"),
            input_export("message-state", "message", "state"),
            input_export("draft", "composer", "value"),
            input_export("send-label", "send", "label"),
            input_export("send-description", "send", "description"),
            output_export("send", "send", "pressed"),
        ],
        Vec::new(),
        "Live Message reading and an ordinary composer action",
    )
}

fn draft_queue_definition() -> SandDefinition {
    compound(
        "official-draft-queue",
        "Draft and preset queue",
        vec![
            child("title", "title", 0.0, 0.0, 340.0, 26.0, 0),
            child("draft", "official-draft-entry", 0.0, 34.0, 340.0, 150.0, 1),
            child("add", "official-action", 0.0, 192.0, 140.0, 28.0, 2),
        ],
        vec![
            boolean_input("pinned", false),
            text_input("content", "Draft"),
            text_input("age", "now"),
            text_input("send-timing", "send now"),
            record_input("draft", "unbound-draft"),
            text_input("send-label", "Send now"),
            text_input("add-label", "Add draft"),
        ],
        vec![boolean_output("add")],
        vec![
            input_export("pinned", "draft", "pinned"),
            input_export("content", "draft", "content"),
            input_export("age", "draft", "age"),
            input_export("send-timing", "draft", "send-timing"),
            input_export("draft", "draft", "draft"),
            input_export("send-label", "draft", "send-label"),
            input_export("add-label", "add", "label"),
            output_export("add", "add", "pressed"),
        ],
        vec![ConfigurationField {
            name: "author-private".into(),
            value_type: ValueType::Boolean,
            required: true,
            default: Some(SandValue::Boolean(true)),
        }],
        "Pinned presets and private drafts remain one ordered author-owned list",
    )
}

fn kanban_column_definition() -> SandDefinition {
    compound(
        "official-kanban-column",
        "Kanban column",
        vec![
            child("title", "title", 0.0, 0.0, 300.0, 26.0, 0),
            child(
                "record",
                "official-record-summary",
                0.0,
                34.0,
                300.0,
                156.0,
                1,
            ),
            child("add", "official-action", 0.0, 198.0, 140.0, 28.0, 2),
        ],
        vec![
            text_input("title", "Column"),
            record_input("record", "unbound-record"),
            text_input("record-title", "Untitled Record"),
            text_input("record-description", "No description"),
            number_input("record-quantity", 0.0),
            text_input("record-state", "neutral"),
            text_input("add-label", "Add Record"),
        ],
        vec![record_output("record-clicked"), boolean_output("add")],
        vec![
            input_export("title", "title", "value"),
            input_export("record", "record", "record"),
            input_export("record-title", "record", "title"),
            input_export("record-description", "record", "description"),
            input_export("record-quantity", "record", "quantity"),
            input_export("record-state", "record", "state"),
            input_export("add-label", "add", "label"),
            output_export("record-clicked", "record", "record-clicked"),
            output_export("add", "add", "pressed"),
        ],
        Vec::new(),
        "One configurable column made from a Record summary and Action",
    )
}

fn table_row_definition() -> SandDefinition {
    compound(
        "official-table-row",
        "Table row",
        vec![
            child("title", "text", 0.0, 0.0, 160.0, 26.0, 0),
            child("quantity", "quantity", 166.0, 0.0, 70.0, 26.0, 1),
            child("state", "badge", 242.0, 0.0, 68.0, 26.0, 2),
            child("open", "official-record-action", 316.0, 0.0, 90.0, 26.0, 3),
        ],
        vec![
            text_input("title", "Untitled"),
            number_input("quantity", 0.0),
            text_input("state", "neutral"),
            record_input("record", "unbound-record"),
        ],
        vec![record_output("record-clicked")],
        vec![
            input_export("title", "title", "value"),
            input_export("quantity", "quantity", "value"),
            input_export("state", "state", "value"),
            input_export("record", "open", "record"),
            output_export("record-clicked", "open", "pressed"),
        ],
        Vec::new(),
        "A reusable row projection rather than copied table markup",
    )
}

fn inspector_definition() -> SandDefinition {
    compound(
        "official-inspector",
        "Inspector",
        vec![
            child("surface", "panel", 0.0, 0.0, 340.0, 150.0, 0),
            child("title", "title", 14.0, 12.0, 312.0, 24.0, 1),
            child("first", "official-property", 14.0, 44.0, 312.0, 24.0, 2),
            child("second", "official-property", 14.0, 74.0, 312.0, 24.0, 3),
            child("action", "official-action", 14.0, 108.0, 140.0, 28.0, 4),
        ],
        Vec::new(),
        vec![boolean_output("action")],
        vec![output_export("action", "action", "pressed")],
        Vec::new(),
        "A property composition that Box can add to, remove from or fork",
    )
}

fn specialized_definition(uid: &str, name: &str, kind: ProjectionKind) -> SandDefinition {
    let (isolation, required) = match kind {
        ProjectionKind::Website => (
            Isolation::Website,
            BTreeSet::from([RendererCapability::ExternalHtml]),
        ),
        ProjectionKind::NativeWorld => (
            Isolation::Trusted,
            BTreeSet::from([
                RendererCapability::InstancedNodes,
                RendererCapability::Accessibility,
            ]),
        ),
        _ => (
            Isolation::Trusted,
            BTreeSet::from([
                RendererCapability::RetainedControls,
                RendererCapability::Accessibility,
            ]),
        ),
    };
    SandDefinition {
        uid: uid.into(),
        revision: 1,
        display_name: name.into(),
        element: SandElement::Specialized,
        inputs: vec![InputPort {
            name: "source".into(),
            value_type: ValueType::Json,
            required: false,
            default: Some(SandValue::Json(serde_json::json!({}))),
        }],
        outputs: vec![record_output("selected")],
        children: Vec::new(),
        connections: Vec::new(),
        exports: Vec::new(),
        behaviors: Vec::new(),
        configuration: Vec::new(),
        style: StyleLayer::default(),
        accessibility: AccessibilitySpec {
            role: AccessibilityRole::Group,
            label: name.into(),
            description: Some("Bounded specialized renderer leaf".into()),
            live: false,
        },
        capabilities: BTreeSet::new(),
        projections: vec![ProjectionManifest {
            key: projection_key(kind).into(),
            kind,
            isolation,
            required,
            capabilities: BTreeSet::new(),
            assets: Vec::new(),
            projected_nodes: BTreeSet::from(["root".into()]),
        }],
    }
}

fn official_definition(spec: OfficialSandSpec) -> SandDefinition {
    let children = official_children(spec.uid);
    let mut definition = compound(
        spec.uid,
        spec.display_name,
        children,
        vec![record_input("source", "unbound-record")],
        Vec::new(),
        Vec::new(),
        vec![ConfigurationField {
            name: "compact".into(),
            value_type: ValueType::Boolean,
            required: true,
            default: Some(SandValue::Boolean(false)),
        }],
        spec.purpose,
    );
    definition.capabilities.insert(SandCapability::ProteinRead);
    match spec.uid {
        "shell-edit" => {
            definition.inputs.extend([
                text_input("primary-label", "Toggle edit"),
                text_input("secondary-label", "Lock group"),
                text_input("tertiary-label", "Save Castle"),
            ]);
            definition.outputs.extend([
                boolean_output("toggle-edit"),
                boolean_output("toggle-lock"),
                boolean_output("save-castle"),
            ]);
            definition.exports.extend([
                input_export("primary-label", "tools", "primary-label"),
                input_export("secondary-label", "tools", "secondary-label"),
                input_export("tertiary-label", "tools", "tertiary-label"),
                output_export("toggle-edit", "tools", "primary"),
                output_export("toggle-lock", "tools", "secondary"),
                output_export("save-castle", "tools", "tertiary"),
            ]);
            add_action_behavior(&mut definition, "toggle-edit", "interface.edit.toggle");
            add_action_behavior(
                &mut definition,
                "toggle-lock",
                "interface.group.lock.toggle",
            );
            add_action_behavior(&mut definition, "save-castle", "interface.castle.save");
        }
        "shell-zoom" => {
            definition.inputs.extend([
                number_input("level", 100.0),
                text_input("zoom-out-label", "Zoom out"),
                text_input("zoom-in-label", "Zoom in"),
                text_input("recenter-label", "Recenter"),
            ]);
            definition.outputs.extend([
                boolean_output("zoom-out"),
                boolean_output("zoom-in"),
                boolean_output("recenter"),
            ]);
            definition.exports.extend([
                input_export("zoom-out-label", "zoom-out", "label"),
                input_export("zoom-in-label", "zoom-in", "label"),
                input_export("recenter-label", "recenter", "label"),
                input_export("level", "level", "value"),
                output_export("zoom-out", "zoom-out", "pressed"),
                output_export("zoom-in", "zoom-in", "pressed"),
                output_export("recenter", "recenter", "pressed"),
            ]);
            add_action_behavior(&mut definition, "zoom-out", "interface.camera.zoom-out");
            add_action_behavior(&mut definition, "zoom-in", "interface.camera.zoom-in");
            add_action_behavior(&mut definition, "recenter", "interface.camera.recenter");
        }
        "record" => {
            definition.inputs.extend([
                text_input("title", "Untitled Record"),
                text_input("description", "No description"),
                number_input("quantity", 0.0),
                text_input("state", "neutral"),
                text_input("open-label", "Open Record"),
                text_input("open-description", "Open this Record"),
                record_input("conversation", "unbound-conversation"),
                text_input("draft", ""),
                text_input("primary-label", "Next property"),
                text_input("secondary-label", "Show or hide"),
                text_input("tertiary-label", "Save view"),
            ]);
            definition.outputs.extend([
                record_output("record-clicked"),
                boolean_output("property-presentation"),
                boolean_output("save-view"),
                boolean_output("send-message"),
            ]);
            definition.exports.extend([
                input_export("source", "summary", "record"),
                input_export("title", "summary", "title"),
                input_export("description", "summary", "description"),
                input_export("quantity", "summary", "quantity"),
                input_export("state", "summary", "state"),
                input_export("open-label", "summary", "open-label"),
                input_export("open-description", "summary", "open-description"),
                input_export("conversation", "thread", "conversation"),
                input_export("draft", "thread", "draft"),
                input_export("primary-label", "actions", "primary-label"),
                input_export("secondary-label", "actions", "secondary-label"),
                input_export("tertiary-label", "actions", "tertiary-label"),
                output_export("record-clicked", "summary", "record-clicked"),
                output_export("property-presentation", "properties", "action"),
                output_export("save-view", "actions", "tertiary"),
                output_export("send-message", "thread", "send"),
            ]);
            definition.capabilities.insert(SandCapability::EmitEvent {
                event: "record-clicked".into(),
            });
            definition.behaviors.push(BehaviorBinding::Declarative {
                behavior: DeclarativeBehavior::EmitEvent {
                    source_output: "record-clicked".into(),
                    event: "record-clicked".into(),
                },
            });
            definition.behaviors.push(BehaviorBinding::Declarative {
                behavior: DeclarativeBehavior::SetLocalState {
                    source_output: "property-presentation".into(),
                    key: "record-property-presentation".into(),
                    value: SandValue::Text("next".into()),
                },
            });
            add_action_behavior(&mut definition, "save-view", "record.view.save");
            add_action_behavior(&mut definition, "send-message", "conversation.message.send");
        }
        "conversation" => {
            definition.inputs.extend([
                record_input("author", "unknown-author"),
                record_input("operator", "unknown-operator"),
                text_input("title", "Conversation"),
                text_input("message-content", "Nothing said yet"),
                text_input("message-state", "finished"),
                text_input("draft", ""),
                text_input("queued-draft", ""),
                boolean_input("draft-pinned", false),
                text_input("draft-age", "now"),
                text_input("send-timing", "send now"),
                record_input("draft-record", "unbound-draft"),
                text_input("send-label", "Send now"),
                text_input("queued-send-label", "Send now"),
                text_input("status", "Conversation ready"),
            ]);
            definition
                .outputs
                .extend([boolean_output("send-message"), boolean_output("add-draft")]);
            definition.exports.extend([
                input_export("source", "thread", "conversation"),
                input_export("title", "thread", "title"),
                input_export("author", "thread", "author"),
                input_export("operator", "thread", "operator"),
                input_export("message-content", "thread", "message-content"),
                input_export("message-state", "thread", "message-state"),
                input_export("draft", "thread", "draft"),
                input_export("send-label", "thread", "send-label"),
                input_export("draft-pinned", "drafts", "pinned"),
                input_export("queued-draft", "drafts", "content"),
                input_export("draft-age", "drafts", "age"),
                input_export("send-timing", "drafts", "send-timing"),
                input_export("draft-record", "drafts", "draft"),
                input_export("queued-send-label", "drafts", "send-label"),
                input_export("status", "status", "detail"),
                output_export("send-message", "thread", "send"),
                output_export("add-draft", "drafts", "add"),
            ]);
            add_action_behavior(&mut definition, "send-message", "conversation.message.send");
            add_action_behavior(&mut definition, "add-draft", "conversation.draft.add");
            definition.configuration.push(ConfigurationField {
                name: "drafts-private".into(),
                value_type: ValueType::Boolean,
                required: true,
                default: Some(SandValue::Boolean(true)),
            });
        }
        "table" => {
            definition.configuration.push(ConfigurationField {
                name: "show-description".into(),
                value_type: ValueType::Boolean,
                required: true,
                default: Some(SandValue::Boolean(true)),
            });
            definition.inputs.extend([
                text_input("primary-label", "Previous page"),
                text_input("secondary-label", "Next page"),
                text_input("tertiary-label", "Add Record"),
            ]);
            definition.outputs.extend([
                boolean_output("previous-page"),
                boolean_output("next-page"),
                boolean_output("create-record"),
                record_output("record-clicked"),
            ]);
            definition.exports.extend([
                input_export("primary-label", "tools", "primary-label"),
                input_export("secondary-label", "tools", "secondary-label"),
                input_export("tertiary-label", "tools", "tertiary-label"),
                output_export("previous-page", "tools", "primary"),
                output_export("next-page", "tools", "secondary"),
                output_export("create-record", "tools", "tertiary"),
                output_export("record-clicked", "row", "record-clicked"),
            ]);
            add_action_behavior(&mut definition, "create-record", "record.create");
            add_record_event_behavior(&mut definition, "record-clicked");
        }
        "todo" => {
            definition.configuration.push(ConfigurationField {
                name: "show-description".into(),
                value_type: ValueType::Boolean,
                required: true,
                default: Some(SandValue::Boolean(true)),
            });
            definition.inputs.extend([
                text_input("primary-label", "Previous page"),
                text_input("secondary-label", "Next page"),
                text_input("tertiary-label", "Open selected"),
            ]);
            definition.outputs.extend([
                boolean_output("previous-page"),
                boolean_output("next-page"),
                boolean_output("complete"),
                record_output("record-clicked"),
            ]);
            definition.exports.extend([
                input_export("primary-label", "tools", "primary-label"),
                input_export("secondary-label", "tools", "secondary-label"),
                input_export("tertiary-label", "tools", "tertiary-label"),
                output_export("previous-page", "tools", "primary"),
                output_export("next-page", "tools", "secondary"),
                output_export("record-clicked", "task", "record-clicked"),
                output_export("complete", "complete", "pressed"),
            ]);
            add_action_behavior(&mut definition, "complete", "record.quantity.complete");
            add_record_event_behavior(&mut definition, "record-clicked");
        }
        "kanban" => {
            definition.configuration.push(ConfigurationField {
                name: "show-description".into(),
                value_type: ValueType::Boolean,
                required: true,
                default: Some(SandValue::Boolean(true)),
            });
            definition.inputs.extend([
                text_input("primary-label", "Move left"),
                text_input("secondary-label", "Move right"),
                text_input("tertiary-label", "Add Record"),
            ]);
            definition.outputs.extend([
                boolean_output("move-left"),
                boolean_output("move-right"),
                boolean_output("create-record"),
                record_output("backlog-record-clicked"),
                record_output("active-record-clicked"),
                record_output("done-record-clicked"),
            ]);
            definition.exports.extend([
                input_export("primary-label", "tools", "primary-label"),
                input_export("secondary-label", "tools", "secondary-label"),
                input_export("tertiary-label", "tools", "tertiary-label"),
                output_export("move-left", "tools", "primary"),
                output_export("move-right", "tools", "secondary"),
                output_export("create-record", "tools", "tertiary"),
                output_export(
                    "backlog-record-clicked",
                    "backlog",
                    "record-clicked",
                ),
                output_export("active-record-clicked", "active", "record-clicked"),
                output_export("done-record-clicked", "done", "record-clicked"),
            ]);
            add_action_behavior(&mut definition, "move-left", "record.quantity.move-left");
            add_action_behavior(&mut definition, "move-right", "record.quantity.move-right");
            add_action_behavior(&mut definition, "create-record", "record.create");
            for output in [
                "backlog-record-clicked",
                "active-record-clicked",
                "done-record-clicked",
            ] {
                add_record_event_behavior(&mut definition, output);
            }
        }
        _ => {}
    }
    definition
}

fn add_action_behavior(definition: &mut SandDefinition, output: &str, action: &str) {
    definition
        .capabilities
        .insert(SandCapability::RequestAction {
            action: action.into(),
        });
    definition.behaviors.push(BehaviorBinding::Declarative {
        behavior: DeclarativeBehavior::RequestAction {
            source_output: output.into(),
            action: action.into(),
        },
    });
}

fn add_record_event_behavior(definition: &mut SandDefinition, output: &str) {
    definition.capabilities.insert(SandCapability::EmitEvent {
        event: "record-clicked".into(),
    });
    definition.behaviors.push(BehaviorBinding::Declarative {
        behavior: DeclarativeBehavior::EmitEvent {
            source_output: output.into(),
            event: "record-clicked".into(),
        },
    });
}

fn official_children(uid: &str) -> Vec<DefinitionChild> {
    if uid == "shell-zoom" {
        return vec![
            child("zoom-out", "official-action", 0.0, 0.0, 104.0, 28.0, 0),
            child("level", "quantity", 112.0, 0.0, 80.0, 28.0, 1),
            child("zoom-in", "official-action", 200.0, 0.0, 104.0, 28.0, 2),
            child("recenter", "official-action", 312.0, 0.0, 108.0, 28.0, 3),
        ];
    }
    let definitions: &[(&str, &str)] = match uid {
        "shell-edit" => &[
            ("surface", "panel"),
            ("tools", "official-toolbar"),
            ("selection", "official-inspector"),
            ("status", "official-status"),
        ],
        "shell-ai" => &[
            ("surface", "panel"),
            ("title", "title"),
            ("prompt", "official-labeled-textarea"),
            ("actions", "official-toolbar"),
            ("status", "official-status"),
        ],
        "instinct" => &[
            ("surface", "official-document-surface"),
            ("tools", "official-toolbar"),
            ("progress", "official-status"),
        ],
        "document-viewer" => &[
            ("surface", "official-document-surface"),
            ("tools", "official-toolbar"),
            ("status", "official-status"),
        ],
        "freedoom-portal" => &[
            ("surface", "official-game-surface"),
            ("tools", "official-toolbar"),
            ("status", "official-status"),
        ],
        "lince-logo-led" => &[
            ("surface", "official-led-surface"),
            ("controls", "official-toolbar"),
            ("status", "official-status"),
        ],
        "lince-website" => &[
            ("security", "official-status"),
            ("surface", "official-website-surface"),
            ("controls", "official-toolbar"),
        ],
        "terminal" => &[
            ("surface", "official-terminal-surface"),
            ("controls", "official-toolbar"),
            ("status", "official-status"),
        ],
        "kanban" => &[
            ("tools", "official-toolbar"),
            ("backlog", "official-kanban-column"),
            ("active", "official-kanban-column"),
            ("done", "official-kanban-column"),
            ("empty", "official-empty-state"),
        ],
        "relations" => &[
            ("tools", "official-toolbar"),
            ("graph", "official-graph-surface"),
            ("inspector", "official-inspector"),
            ("status", "official-status"),
        ],
        "ontology" => &[
            ("tools", "official-toolbar"),
            ("graph", "official-ontology-surface"),
            ("inspector", "official-inspector"),
        ],
        "record" => &[
            ("summary", "official-record-summary"),
            ("properties", "official-inspector"),
            ("thread", "official-conversation-thread"),
            ("actions", "official-toolbar"),
        ],
        "organ" => &[
            ("summary", "official-record-summary"),
            ("members", "official-table-row"),
            ("actions", "official-toolbar"),
            ("status", "official-status"),
        ],
        "conversation" => &[
            ("thread", "official-conversation-thread"),
            ("drafts", "official-draft-queue"),
            ("status", "official-status"),
        ],
        "record-editor" => &[
            ("title", "official-labeled-field"),
            ("description", "official-labeled-textarea"),
            ("quantity", "official-labeled-field"),
            ("actions", "official-toolbar"),
            ("status", "official-status"),
        ],
        "table" => &[
            ("tools", "official-toolbar"),
            ("row", "official-table-row"),
            ("empty", "official-empty-state"),
            ("status", "official-status"),
        ],
        "permissions" => &[
            ("tools", "official-toolbar"),
            ("principal", "official-record-summary"),
            ("grant", "official-labeled-field"),
            ("status", "official-status"),
        ],
        "todo" => &[
            ("tools", "official-toolbar"),
            ("task", "official-record-summary"),
            ("complete", "official-action"),
            ("empty", "official-empty-state"),
            ("status", "official-status"),
        ],
        "transfer" => &[
            ("need", "official-record-summary"),
            ("contribution", "official-record-summary"),
            ("quantity", "official-property"),
            ("actions", "official-toolbar"),
            ("status", "official-status"),
        ],
        "karma" => &[
            ("tools", "official-toolbar"),
            ("surface", "official-karma-surface"),
            ("rule", "official-inspector"),
            ("status", "official-status"),
        ],
        "archive" => &[
            ("tools", "official-toolbar"),
            ("selection", "official-table-row"),
            ("destination", "official-labeled-field"),
            ("status", "official-status"),
        ],
        "communication" => &[
            ("tools", "official-toolbar"),
            ("contact", "official-record-summary"),
            ("thread", "official-conversation-thread"),
            ("status", "official-status"),
        ],
        "sand-publisher" => &[
            ("package", "official-labeled-field"),
            ("title", "official-labeled-field"),
            ("description", "official-labeled-textarea"),
            ("actions", "official-toolbar"),
            ("status", "official-status"),
        ],
        _ => &[("surface", "panel"), ("status", "official-status")],
    };
    let mut y = 0.0;
    definitions
        .iter()
        .enumerate()
        .map(|(index, (local_uid, definition_uid))| {
            let [width, height] = preferred_child_size(definition_uid);
            let definition = child(
                local_uid,
                definition_uid,
                0.0,
                y,
                width,
                height,
                index as u32,
            );
            y += height + 8.0;
            definition
        })
        .collect()
}

fn preferred_child_size(uid: &str) -> [f64; 2] {
    match uid {
        "official-toolbar" => [320.0, 28.0],
        "official-inspector" => [340.0, 150.0],
        "official-status" => [340.0, 24.0],
        "official-record-summary" => [340.0, 156.0],
        "official-conversation-thread" => [340.0, 262.0],
        "official-draft-queue" => [340.0, 228.0],
        "official-kanban-column" => [300.0, 226.0],
        "official-table-row" => [406.0, 26.0],
        "official-labeled-field" | "official-labeled-textarea" => [320.0, 80.0],
        "official-empty-state" => [340.0, 130.0],
        "official-property" => [340.0, 24.0],
        "official-action" => [160.0, 28.0],
        "title" => [340.0, 26.0],
        "quantity" => [100.0, 28.0],
        value if value.ends_with("-surface") => [420.0, 260.0],
        _ => [420.0, 88.0],
    }
}

fn compound(
    uid: &str,
    name: &str,
    children: Vec<DefinitionChild>,
    inputs: Vec<InputPort>,
    outputs: Vec<OutputPort>,
    exports: Vec<ExportedPort>,
    configuration: Vec<ConfigurationField>,
    description: &str,
) -> SandDefinition {
    let nodes = std::iter::once("root".to_string())
        .chain(children.iter().map(|child| child.local_uid.clone()))
        .collect();
    SandDefinition {
        uid: uid.into(),
        revision: 1,
        display_name: name.into(),
        element: SandElement::Compound,
        inputs,
        outputs,
        children,
        connections: Vec::new(),
        exports,
        behaviors: Vec::new(),
        configuration,
        style: StyleLayer::default(),
        accessibility: AccessibilitySpec {
            role: AccessibilityRole::Group,
            label: name.into(),
            description: Some(description.into()),
            live: false,
        },
        capabilities: BTreeSet::new(),
        projections: vec![ProjectionManifest {
            key: "native-retained".into(),
            kind: ProjectionKind::NativeRetained,
            isolation: Isolation::Trusted,
            required: BTreeSet::from([
                RendererCapability::RetainedControls,
                RendererCapability::Accessibility,
            ]),
            capabilities: BTreeSet::new(),
            assets: Vec::new(),
            projected_nodes: nodes,
        }],
    }
}

fn child(
    local_uid: &str,
    definition_uid: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    sibling_order: u32,
) -> DefinitionChild {
    DefinitionChild {
        local_uid: local_uid.into(),
        definition: DefinitionRef {
            uid: definition_uid.into(),
            revision: 1,
        },
        transform: Transform2d {
            x,
            y,
            width,
            height,
            rotation_radians: 0.0,
        },
        sibling_order,
        configuration: BTreeMap::new(),
        style: StyleLayer::default(),
    }
}

fn text_input(name: &str, default: &str) -> InputPort {
    InputPort {
        name: name.into(),
        value_type: ValueType::Text,
        required: false,
        default: Some(SandValue::Text(default.into())),
    }
}

fn number_input(name: &str, default: f64) -> InputPort {
    InputPort {
        name: name.into(),
        value_type: ValueType::Number,
        required: false,
        default: Some(SandValue::Number(default)),
    }
}

fn boolean_input(name: &str, default: bool) -> InputPort {
    InputPort {
        name: name.into(),
        value_type: ValueType::Boolean,
        required: false,
        default: Some(SandValue::Boolean(default)),
    }
}

fn record_input(name: &str, default: &str) -> InputPort {
    InputPort {
        name: name.into(),
        value_type: ValueType::Record,
        required: false,
        default: Some(SandValue::Record(default.into())),
    }
}

fn text_output(name: &str) -> OutputPort {
    OutputPort {
        name: name.into(),
        value_type: ValueType::Text,
    }
}

fn boolean_output(name: &str) -> OutputPort {
    OutputPort {
        name: name.into(),
        value_type: ValueType::Boolean,
    }
}

fn record_output(name: &str) -> OutputPort {
    OutputPort {
        name: name.into(),
        value_type: ValueType::Record,
    }
}

fn input_export(name: &str, child_uid: &str, child_port: &str) -> ExportedPort {
    ExportedPort {
        name: name.into(),
        child_uid: child_uid.into(),
        child_port: child_port.into(),
        direction: PortDirection::Input,
    }
}

fn output_export(name: &str, child_uid: &str, child_port: &str) -> ExportedPort {
    ExportedPort {
        name: name.into(),
        child_uid: child_uid.into(),
        child_port: child_port.into(),
        direction: PortDirection::Output,
    }
}

fn projection_key(kind: ProjectionKind) -> &'static str {
    match kind {
        ProjectionKind::NativeRetained => "native-retained",
        ProjectionKind::NativeWorld => "native-world",
        ProjectionKind::InstalledHtml => "installed-html",
        ProjectionKind::Website => "website",
        ProjectionKind::BrowserDom => "browser-dom",
        ProjectionKind::Wasm => "wasm",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_catalog_is_valid_rust_owned_and_recursively_decomposed() {
        let package = official_sand_package();
        package.validate().unwrap();
        let catalog = official_sand_catalog().unwrap();
        assert!(package.graph.definitions.len() >= 60);
        for spec in OFFICIAL_SANDS {
            let definition = &package.graph.definitions[spec.uid];
            assert_eq!(definition.element, SandElement::Compound);
            assert!(definition.children.len() >= 2);
            assert!(
                definition
                    .projections
                    .iter()
                    .any(|projection| projection.kind == ProjectionKind::NativeRetained)
            );
            let record = catalog.record(&DefinitionRef {
                uid: spec.uid.into(),
                revision: 1,
            });
            assert!(matches!(record.unwrap().origin, DefinitionOrigin::Rust));
        }
        assert!(
            package
                .graph
                .definitions
                .values()
                .filter(|definition| definition.element == SandElement::Specialized)
                .all(|definition| !OFFICIAL_SANDS.iter().any(|spec| spec.uid == definition.uid))
        );
    }

    #[test]
    fn conversation_exposes_general_message_and_private_draft_structure() {
        let gallery = OfficialSandGalleryState::new().unwrap();
        let graph = &gallery.package().graph;
        let conversation = &graph.definitions["conversation"];
        assert!(
            conversation
                .children
                .iter()
                .any(|child| child.definition.uid == "official-conversation-thread")
        );
        assert!(
            conversation
                .children
                .iter()
                .any(|child| child.definition.uid == "official-draft-queue")
        );
        let message = &graph.definitions["official-message"];
        assert!(message.inputs.iter().any(|input| input.name == "author"));
        assert!(message.inputs.iter().any(|input| input.name == "operator"));
        assert!(message.inputs.iter().any(|input| input.name == "state"));
        let draft = &graph.definitions["official-draft-entry"];
        for name in ["pinned", "age", "send-timing"] {
            assert!(draft.inputs.iter().any(|input| input.name == name));
        }
    }

    #[test]
    fn official_gallery_discloses_incomplete_migration() {
        let mut gallery = OfficialSandGalleryState::new().unwrap();
        assert_eq!(
            gallery.selected().state,
            OfficialMigrationState::NativeBehavior
        );
        gallery.focus_at(23);
        assert_eq!(gallery.selected().uid, "configuration");
        assert_eq!(gallery.selected().state, OfficialMigrationState::Landed);
        assert!(!gallery.tree_lines().is_empty());
    }

    #[test]
    fn operational_shared_roots_declare_their_behavior_and_data_boundary() {
        let package = official_sand_package();
        for uid in [
            "shell-edit",
            "shell-zoom",
            "record",
            "conversation",
            "table",
            "todo",
            "kanban",
        ] {
            let definition = &package.graph.definitions[uid];
            assert!(!definition.behaviors.is_empty());
            assert_eq!(
                OFFICIAL_SANDS
                    .iter()
                    .find(|spec| spec.uid == uid)
                    .unwrap()
                    .state,
                OfficialMigrationState::NativeBehavior
            );
        }
        let record = &package.graph.definitions["record"];
        for input in [
            "source",
            "title",
            "description",
            "quantity",
            "state",
            "conversation",
            "draft",
        ] {
            assert!(record.inputs.iter().any(|port| port.name == input));
        }
        for output in [
            "record-clicked",
            "property-presentation",
            "save-view",
            "send-message",
        ] {
            assert!(record.outputs.iter().any(|port| port.name == output));
        }
        let table = &package.graph.definitions["table"];
        assert!(table.outputs.iter().any(|port| port.name == "record-clicked"));
        assert!(table.outputs.iter().any(|port| port.name == "create-record"));
        let todo = &package.graph.definitions["todo"];
        assert!(todo.outputs.iter().any(|port| port.name == "complete"));
        let kanban = &package.graph.definitions["kanban"];
        for output in ["move-left", "move-right", "create-record"] {
            assert!(kanban.outputs.iter().any(|port| port.name == output));
        }
    }
}
