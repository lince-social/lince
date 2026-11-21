// non-lean-yet
// Scope: mod tests, struct DirectoryEditor, struct StopChild, fn directory_editing.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 25, 26, 79, 110,
//   120, 142, 143, 152, 153, 199, 223, 322, 356, 373, 381, 391, 400, 417, 446, 447, 464, 465, 481,
//   513, 514, 536, 540, 615, 619, 633, 639, 656, 657, 725, 747, 769, 792.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   348, 707.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (79, 199, 220, 247, 252, 356, 391, 400, 417, 443, 464, 465, 481, 486, 491, 510,
//   582, 633, 662, 666, 678, 679, 725, 734, 769, 777, 791, 799).

use super::*;

#[cfg(test)]
mod tests;

#[derive(Component)]
struct DirectoryEditor {
    field: Entity,
    summary: Entity,
    original: String,
}

#[derive(Clone)]
struct StopChild {
    record: String,
    thread: String,
}

impl Action for StopChild {
    fn apply(&self, world: &mut World, owner: Entity) {
        request(
            world,
            owner,
            FioteRequest::StopChild {
                record: self.record.clone(),
                thread: self.thread.clone(),
            },
        );
    }
}

pub(super) fn directory_editing(world: &World, owner: Entity) -> bool {
    world.get::<DirectoryEditor>(owner).is_some_and(|editor| {
        value(world, editor.field).is_ok_and(|value| value != editor.original)
    })
}

pub(super) fn directory_summary(saved: &FioteStatus) -> String {
    if let Some(error) = &saved.working_directory.error {
        return format!("Working directory: {error}");
    }
    let Some(path) = &saved.working_directory.effective else {
        return "Working directory: none set in this branch.".into();
    };
    if saved.working_directory.source.as_deref() == Some(&saved.record) {
        return format!("Working directory: {} · set on this Fiote", path.display());
    }
    let source = saved
        .fiotes
        .iter()
        .find(|choice| Some(&choice.record) == saved.working_directory.source.as_ref());
    format!(
        "Working directory: {} · inherited from {}",
        path.display(),
        source
            .map(|choice| choice.title.as_str())
            .unwrap_or("parent Fiote")
    )
}

pub(super) fn update_directory(world: &mut World, owner: Entity, saved: &FioteStatus) {
    if let Some(summary) = world
        .get::<DirectoryEditor>(owner)
        .map(|editor| editor.summary)
        && let Some(mut text) = world.get_mut::<Text>(summary)
    {
        text.set_if_neq(Text::new(directory_summary(saved)));
    }
}

#[derive(Clone)]
pub(super) struct Open;
impl Action for Open {
    fn apply(&self, world: &mut World, owner: Entity) {
        if super::open_setup(world, owner) {
            return;
        }
        if world.get::<super::dashboard::Dashboard>(owner).is_some()
            && world
                .get::<Panel>(owner)
                .is_some_and(|panel| panel.step != Step::Closed)
        {
            super::Close.apply(world, owner);
            return;
        }
        super::show(world, owner, Step::Manage);
        let record = world.get::<Panel>(owner).unwrap().binding.uid.clone();
        request(world, owner, FioteRequest::Prepare { record });
    }
}

pub(super) fn show(world: &mut World, owner: Entity, content: Entity, saved: Option<&FioteStatus>) {
    let Some(saved) = saved else { return };

    let profile = accordion(world, content, owner, "Profile and connection", true);
    profile_settings(world, owner, profile, saved);
    let work = accordion(world, content, owner, "Runs and assigned work", false);
    run_settings(world, owner, work, saved);
    let inheritance = accordion(world, content, owner, "Parent and instructions", false);
    instruction_settings(world, owner, inheritance, saved);
    let roster = accordion(world, content, owner, "Manage roster", false);
    roster_settings(world, owner, roster, saved);
}

fn profile_settings(world: &mut World, owner: Entity, content: Entity, saved: &FioteStatus) {
    let binding = world.get::<Panel>(owner).unwrap().binding.clone();
    if let Some(source) = saved.instructions.last() {
        for (property, label, text) in [
            ("head", "Name", &source.title),
            ("body", "System prompt", &source.body),
        ] {
            crate::native::edit_mode::label(world, content, label, 14.0);
            let editor = world
                .spawn((
                    crate::native::sand::text_editor(
                        text,
                        world.resource::<crate::native::theme::Typography>(),
                        0,
                    ),
                    ChildOf(content),
                ))
                .id();
            crate::native::accessibility::input(world, editor, label, property != "head");
            if let Some(mut input) = world.get_mut::<EditableText>(editor) {
                input.allow_newlines = property == "body";
                input.visible_lines = Some(if property == "body" { 5.0 } else { 1.0 });
            }
            crate::native::record_binding::attach(world, editor, binding.clone(), property, None);
        }
    }
    super::dashboard::button(
        world,
        content,
        owner,
        "AI connections",
        super::connections::Open,
    );
    let summary = crate::native::edit_mode::label(world, content, &directory_summary(saved), 14.0);
    let original = saved
        .working_directory
        .configured
        .to_string_lossy()
        .into_owned();
    let directory = field(
        world,
        content,
        "Working directory override · blank inherits",
        false,
    );
    world
        .get_mut::<EditableText>(directory)
        .unwrap()
        .editor
        .set_text(&original);
    world
        .get_mut::<EditableText>(directory)
        .unwrap()
        .allow_newlines = false;
    world.entity_mut(owner).insert(DirectoryEditor {
        field: directory,
        summary,
        original,
    });
    let directory_actions = super::dashboard::controls(world, content);
    super::dashboard::button(
        world,
        directory_actions,
        owner,
        "Save working directory",
        SaveDirectory,
    );
    super::dashboard::button(
        world,
        directory_actions,
        owner,
        if saved.behavior.prompt_parent.is_some() {
            "Use parent directory"
        } else {
            "Clear working directory"
        },
        InheritDirectory,
    );
    super::dashboard::button(
        world,
        directory_actions,
        owner,
        "Open Record and conversation",
        OpenRecord,
    );
}

fn run_settings(world: &mut World, owner: Entity, content: Entity, saved: &FioteStatus) {
    let handoffs = collapsed(world, content, owner, "Show delegated work");
    for handoff in &saved.handoffs {
        crate::native::edit_mode::label(
            world,
            handoffs,
            &format!(
                "{} · to {} · {}\n{}",
                handoff.request_id, handoff.target, handoff.state, handoff.detail,
            ),
            13.0,
        );
        if let Some(thread) = &handoff.thread {
            let actions = super::dashboard::controls(world, handoffs);
            super::dashboard::button(
                world,
                actions,
                owner,
                "Open delegated conversation",
                OpenConversation {
                    target: handoff.target.clone(),
                    thread: thread.clone(),
                },
            );
            super::dashboard::button(
                world,
                actions,
                owner,
                "Stop delegated Fiote",
                StopChild {
                    record: saved.record.clone(),
                    thread: thread.clone(),
                },
            );
        }
    }
    let actions = super::dashboard::controls(world, content);
    super::dashboard::button(world, actions, owner, "Run now", RunNow);
    super::dashboard::button(
        world,
        actions,
        owner,
        "Refresh activations",
        RefreshActivations,
    );
    super::dashboard::button(
        world,
        actions,
        owner,
        "Stop current and pending runs",
        CancelActivations,
    );
    let activations = collapsed(world, content, owner, "Show activations");
    for activation in &saved.activations {
        crate::native::edit_mode::label(
            world,
            activations,
            &format!(
                "{} · value {} · {}\n{}\n{}",
                activation["request_id"].as_str().unwrap_or_default(),
                activation["value"].as_str().unwrap_or_default(),
                activation["state"].as_str().unwrap_or_default(),
                activation["detail"].as_str().unwrap_or_default(),
                activation["cause"]
            ),
            13.0,
        );
        if let Some(thread) = activation["thread"].as_str() {
            super::dashboard::button(
                world,
                activations,
                owner,
                "Open activation thread",
                OpenTask(thread.into()),
            );
        }
    }
    super::dashboard::button(
        world,
        content,
        owner,
        if saved.behavior.run_assigned {
            "Assigned work: starts automatically"
        } else {
            "Assigned work: automatic start is off"
        },
        ToggleAssignments,
    );
    let tasks = collapsed(world, content, owner, "Show assigned work");
    let view_uid = world.get::<Panel>(owner).unwrap().view_uid.clone();
    for task in saved
        .tasks
        .iter()
        .filter(|task| view_uid == saved.record || task.task == view_uid)
    {
        crate::native::edit_mode::label(
            world,
            tasks,
            &format!("{} · {}\n{}", task.title, task.state, task.detail),
            14.0,
        );
        let actions = super::dashboard::controls(world, tasks);
        super::dashboard::button(
            world,
            actions,
            owner,
            "Open task thread",
            OpenTask(task.thread.clone()),
        );
        if matches!(task.state.as_str(), "failed" | "interrupted" | "finished") {
            super::dashboard::button(
                world,
                actions,
                owner,
                "Retry task after inspecting changes",
                Retry(task.thread.clone()),
            );
        }
    }
}

fn instruction_settings(world: &mut World, owner: Entity, content: Entity, saved: &FioteStatus) {
    let parent = saved
        .behavior
        .prompt_parent
        .as_ref()
        .and_then(|uid| saved.fiotes.iter().find(|choice| &choice.record == uid));
    crate::native::edit_mode::label(
        world,
        content,
        &format!(
            "Parent Fiote: {}",
            parent.map_or("None", |choice| choice.title.as_str())
        ),
        14.0,
    );
    let parents = collapsed(world, content, owner, "Change parent Fiote");
    super::dashboard::button(world, parents, owner, "No parent Fiote", Parent(None));
    let parent_choices = super::dashboard::controls(world, parents);
    for choice in saved
        .fiotes
        .iter()
        .filter(|choice| choice.record != saved.record)
    {
        super::dashboard::button(
            world,
            parent_choices,
            owner,
            &choice.title,
            Parent(Some(choice.record.clone())),
        );
    }
    crate::native::edit_mode::label(
        world,
        content,
        "Instructions for new sessions · ancestor first",
        16.0,
    );
    crate::native::edit_mode::label(
        world,
        content,
        "Sessions pin these descriptions. After editing a prompt, apply it to an idle thread or start a new thread. Compaction reloads the pinned instructions.",
        14.0,
    );
    let instructions = collapsed(world, content, owner, "View effective instructions");
    if let Some(error) = &saved.instruction_error {
        crate::native::edit_mode::label(world, instructions, error, 14.0);
    }
    for source in &saved.instructions {
        crate::native::edit_mode::label(
            world,
            instructions,
            &format!(
                "{} · {}\n{}",
                source.title,
                &source.revision[..source.revision.len().min(12)],
                source.body
            ),
            14.0,
        );
    }
}

fn roster_settings(world: &mut World, owner: Entity, content: Entity, saved: &FioteStatus) {
    super::dashboard::button(world, content, owner, "Ask Fiote", super::assistant::Open);
    if let Some(intake) = &saved.intake {
        crate::native::edit_mode::label(
            world,
            content,
            &format!(
                "Automatic assistant: {} · {} direct children · created {}{}",
                intake.title,
                intake.children,
                intake.created_at,
                if intake.enabled {
                    ""
                } else {
                    " · connection needs setup"
                },
            ),
            13.0,
        );
        super::dashboard::button(
            world,
            content,
            owner,
            "Open automatic assistant",
            Select(intake.record.clone()),
        );
    }
    let list = super::dashboard::controls(world, content);
    for choice in &saved.fiotes {
        super::dashboard::button(
            world,
            list,
            owner,
            &choice.title,
            Select(choice.record.clone()),
        );
    }
    super::dashboard::button(world, content, owner, "Create Fiote", Create);
    super::dashboard::button(world, content, owner, "Delete Fiote…", ConfirmDelete);
}

#[derive(Clone)]
struct RefreshActivations;
impl Action for RefreshActivations {
    fn apply(&self, world: &mut World, owner: Entity) {
        let record = world.get::<Panel>(owner).unwrap().binding.uid.clone();
        request(world, owner, FioteRequest::Inspect { record });
    }
}

#[derive(Clone)]
struct RunNow;
impl Action for RunNow {
    fn apply(&self, world: &mut World, owner: Entity) {
        let record = world.get::<Panel>(owner).unwrap().binding.uid.clone();
        request(
            world,
            owner,
            FioteRequest::Activate {
                record,
                value: "1".into(),
                request_id: nucleus::new_uid("fiote-run"),
            },
        );
    }
}

#[derive(Clone)]
struct CancelActivations;
impl Action for CancelActivations {
    fn apply(&self, world: &mut World, owner: Entity) {
        let record = world.get::<Panel>(owner).unwrap().binding.uid.clone();
        request(world, owner, FioteRequest::CancelActivations { record });
    }
}

#[derive(Clone)]
struct Parent(Option<String>);
impl Action for Parent {
    fn apply(&self, world: &mut World, owner: Entity) {
        let panel = world.get::<Panel>(owner).unwrap();
        let Some(saved) = &panel.saved else { return };
        let message = FioteRequest::Behavior {
            record: saved.record.clone(),
            prompt_parent: self.0.clone(),
            run_assigned: saved.behavior.run_assigned,
        };
        request(world, owner, message);
    }
}

#[derive(Clone)]
struct SaveDirectory;
impl Action for SaveDirectory {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(editor) = world.get::<DirectoryEditor>(owner) else {
            return;
        };
        let Ok(directory) = value(world, editor.field) else {
            return;
        };
        let record = world.get::<Panel>(owner).unwrap().binding.uid.clone();
        world.get_mut::<DirectoryEditor>(owner).unwrap().original = directory.clone();
        request(
            world,
            owner,
            FioteRequest::WorkingDirectory {
                record,
                directory: directory.into(),
            },
        );
    }
}

#[derive(Clone)]
struct InheritDirectory;
impl Action for InheritDirectory {
    fn apply(&self, world: &mut World, owner: Entity) {
        let record = world.get::<Panel>(owner).unwrap().binding.uid.clone();
        if let Some(editor) = world.get::<DirectoryEditor>(owner) {
            let field = editor.field;
            world
                .get_mut::<EditableText>(field)
                .unwrap()
                .editor
                .set_text("");
            world
                .get_mut::<DirectoryEditor>(owner)
                .unwrap()
                .original
                .clear();
        }
        request(
            world,
            owner,
            FioteRequest::WorkingDirectory {
                record,
                directory: Default::default(),
            },
        );
    }
}

#[derive(Clone)]
struct ToggleAssignments;
impl Action for ToggleAssignments {
    fn apply(&self, world: &mut World, owner: Entity) {
        let panel = world.get::<Panel>(owner).unwrap();
        let Some(saved) = &panel.saved else { return };
        let message = FioteRequest::Behavior {
            record: saved.record.clone(),
            prompt_parent: saved.behavior.prompt_parent.clone(),
            run_assigned: !saved.behavior.run_assigned,
        };
        request(world, owner, message);
    }
}

#[derive(Clone)]
pub(super) struct Connection;
impl Action for Connection {
    fn apply(&self, world: &mut World, owner: Entity) {
        super::show(world, owner, Step::Agent);
    }
}

#[derive(Clone)]
pub(super) struct OpenTask(pub(super) String);
impl Action for OpenTask {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(panel) = world.get::<Panel>(owner) else {
            return;
        };
        let target = panel.binding.uid.clone();
        super::show(world, owner, Step::Closed);
        OpenConversation {
            target,
            thread: self.0.clone(),
        }
        .apply(world, owner);
    }
}

#[derive(Clone)]
pub(super) struct OpenConversation {
    pub(super) target: String,
    pub(super) thread: String,
}

impl Action for OpenConversation {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(panel) = world.get::<Panel>(owner) else {
            return;
        };
        let label = panel.status;
        if !nucleus::valid_uid(&self.target, "r") || !nucleus::valid_uid(&self.thread, "r") {
            if let Some(mut text) = world.get_mut::<Text>(label) {
                text.0 = "Inspect the request before opening its conversation.".into();
            }
            return;
        }
        let mut ancestor = Some(panel.binding.area);
        let root = loop {
            let Some(entity) = ancestor else {
                if let Some(mut text) = world.get_mut::<Text>(label) {
                    text.0 = "Open this conversation from an active workspace.".into();
                }
                return;
            };
            if world
                .get::<crate::native::workspace::Workspaces>(entity)
                .is_some()
            {
                break entity;
            }
            ancestor = world.get::<ChildOf>(entity).map(ChildOf::parent);
        };
        let active = world
            .get::<crate::native::workspace::Workspaces>(root)
            .unwrap()
            .active;
        let existing = world
            .query::<(
                Entity,
                &crate::native::area::InfluenceArea,
                &crate::native::workspace::WorkspaceMember,
                &ChildOf,
            )>()
            .iter(world)
            .filter(|(_, area, workspace, parent)| {
                parent.parent() == root
                    && workspace.0 == active
                    && area.protein.as_ref().is_some_and(|config| {
                        config.fiote
                            && config.source == Source::Local
                            && config.draft.query["where"][0]["all"][0]["uid_eq"].as_str()
                                == Some(self.target.as_str())
                    })
            })
            .map(|(entity, _, _, _)| entity)
            .min_by_key(|entity| entity.to_bits());
        let area =
            existing.or_else(|| crate::native::full_record::open_fiote(world, root, &self.target));
        let Some(area) = area else {
            if let Some(mut text) = world.get_mut::<Text>(label) {
                text.0 = "The requested conversation could not be opened.".into();
            }
            return;
        };
        world
            .entity_mut(area)
            .insert(crate::native::thread_castle::RequestedThread(
                self.thread.clone(),
            ));
        let binding = RecordBinding {
            area,
            uid: self.target.clone(),
            source: Source::Local,
        };
        crate::native::thread_castle::select_thread(world, &binding, &self.thread);
        if let Some(mut text) = world.get_mut::<Text>(label) {
            text.0 = "Opening the requested Fiote conversation in this workspace…".into();
        }
    }
}

#[derive(Clone)]
struct Retry(String);
impl Action for Retry {
    fn apply(&self, world: &mut World, owner: Entity) {
        let record = world.get::<Panel>(owner).unwrap().binding.uid.clone();
        request(
            world,
            owner,
            FioteRequest::RetryAssignment {
                record,
                thread: self.0.clone(),
            },
        );
    }
}

#[derive(Clone)]
pub(super) struct Refresh;
impl Action for Refresh {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(control) = world.get::<ThreadControl>(owner) else {
            return;
        };
        if control.pending.is_some() && !control.inspecting {
            return;
        }
        let message = FioteRequest::RefreshInstructions {
            record: control.record.clone(),
            thread: control.thread.clone(),
        };
        let label = control.status;
        match send(world, owner, message) {
            Ok(id) => {
                let mut control = world.get_mut::<ThreadControl>(owner).unwrap();
                control.pending = Some(id);
                control.inspecting = false;
            }
            Err(error) => world.get_mut::<Text>(label).unwrap().0 = error,
        }
    }
}

#[derive(Clone)]
pub(super) struct ViewInstructions;
impl Action for ViewInstructions {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(control) = world.get::<ThreadControl>(owner) else {
            return;
        };
        let entity = world.get::<ChildOf>(control.instructions).unwrap().parent();
        let mut node = world.get_mut::<Node>(entity).unwrap();
        node.display = if node.display == Display::None {
            Display::Flex
        } else {
            Display::None
        };
    }
}

pub(super) fn instructions(world: &mut World, saved: &FioteStatus) {
    let Some(session) = &saved.session else {
        return;
    };
    let controls: Vec<_> = world
        .query::<&ThreadControl>()
        .iter(world)
        .filter(|control| control.thread == session.thread)
        .map(|control| control.instructions)
        .collect();
    let mut text = if session.changed {
        "Prompt descriptions changed. This session still uses the versions below. Apply current instructions between turns to update.\n\n".into()
    } else {
        String::new()
    };
    for source in &session.sources {
        text.push_str(&format!(
            "{} · revision {}\n{}\n\n",
            source.title,
            &source.revision[..source.revision.len().min(12)],
            source.body
        ));
        if let Some(current) = saved
            .instructions
            .iter()
            .find(|current| current.record == source.record && current.revision != source.revision)
        {
            text.push_str(&format!(
                "Current description (not yet applied):\n{}\n\n",
                current.body
            ));
        }
    }
    if let Some(context) = &session.context {
        text.push_str(&format!("\nContext: {}\nTools: {}\nTask: {}\nActivation: {}\nIncluded messages and attachment references:\n{}\nChild sessions (reported unless marked Lince runtime):\n{}\n", context["scope"], context["tools"], context["task"], context["activation"], serde_json::to_string_pretty(&context["messages"]).unwrap_or_default(), serde_json::to_string_pretty(&context["children"]).unwrap_or_default()));
    }
    for entity in controls {
        world.get_mut::<Text>(entity).unwrap().0 = text.clone();
    }
}

#[derive(Clone)]
pub(super) struct Select(pub(super) String);
impl Action for Select {
    fn apply(&self, world: &mut World, owner: Entity) {
        if world.get::<super::dashboard::Dashboard>(owner).is_some()
            && world.get::<Panel>(owner).is_some_and(|panel| panel.binding.uid == self.0)
        {
            super::show(world, owner, Step::Closed);
            super::dashboard::open_current(world, owner);
            return;
        }
        world.entity_mut(owner).remove::<DirectoryEditor>();
        let area = world.get::<Panel>(owner).unwrap().binding.area;
        if let Some(mut area) = world.get_mut::<crate::native::area::InfluenceArea>(area) {
            if let Some(config) = &mut area.protein {
                if config.fiote {
                    config.draft.query["where"] = serde_json::json!([{"all":[{"uid_eq": self.0}]}]);
                    return;
                }
            }
        }
        request(
            world,
            owner,
            FioteRequest::Prepare {
                record: self.0.clone(),
            },
        );
    }
}
#[derive(Clone)]
struct Create;
impl Action for Create {
    fn apply(&self, world: &mut World, owner: Entity) {
        request(
            world,
            owner,
            FioteRequest::Create {
                head: "New Fiote".into(),
            },
        );
    }
}
#[derive(Clone)]
struct OpenRecord;
impl Action for OpenRecord {
    fn apply(&self, world: &mut World, owner: Entity) {
        let binding = world.get::<Panel>(owner).unwrap().binding.clone();
        crate::native::full_record::Open(binding).apply(world, owner);
    }
}
#[derive(Clone)]
struct ConfirmDelete;
impl Action for ConfirmDelete {
    fn apply(&self, world: &mut World, owner: Entity) {
        let content = world.get::<Panel>(owner).unwrap().content;
        crate::native::edit_mode::label(
            world,
            content,
            "Delete this Fiote Record? Its conversations are retained.",
            14.0,
        );
        super::dashboard::button(world, content, owner, "Confirm deletion", Delete);
    }
}
#[derive(Clone)]
struct Delete;
impl Action for Delete {
    fn apply(&self, world: &mut World, owner: Entity) {
        let panel = world.get::<Panel>(owner).unwrap();
        let record = panel.binding.uid.clone();
        request(world, owner, FioteRequest::Delete { record });
    }
}

fn collapsed(world: &mut World, parent: Entity, owner: Entity, label: &str) -> Entity {
    accordion(world, parent, owner, label, false)
}

#[derive(Component, Default)]
struct ExpandedSections(std::collections::HashMap<String, bool>);

pub(super) fn accordion(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    label: &str,
    default_open: bool,
) -> Entity {
    let open = world
        .get::<ExpandedSections>(owner)
        .and_then(|sections| sections.0.get(label))
        .copied()
        .unwrap_or(default_open);
    let group = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                min_width: px(0),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    let content = world
        .spawn(Node {
            display: if open { Display::Flex } else { Display::None },
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            min_width: px(0),
            padding: UiRect::left(px(10)),
            ..default()
        })
        .id();
    let toggle = super::dashboard::button(
        world,
        group,
        group,
        label,
        ToggleSection {
            content,
            owner,
            label: label.into(),
        },
    );
    if let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(toggle) {
        accessibility.set_expanded(open);
    }
    world.entity_mut(content).insert(ChildOf(group));
    content
}
#[derive(Clone)]
struct ToggleSection {
    content: Entity,
    owner: Entity,
    label: String,
}
impl Action for ToggleSection {
    fn apply(&self, world: &mut World, group: Entity) {
        let Some(mut node) = world.get_mut::<Node>(self.content) else {
            return;
        };
        let open = node.display == Display::None;
        node.display = if open { Display::Flex } else { Display::None };
        if let Ok(mut owner) = world.get_entity_mut(self.owner) {
            owner
                .entry::<ExpandedSections>()
                .or_default()
                .into_mut()
                .0
                .insert(self.label.clone(), open);
        }
        if let Some(toggle) = world
            .get::<Children>(group)
            .and_then(|children| children.first())
            .copied()
            && let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(toggle)
        {
            accessibility.set_expanded(open);
        }
    }
}
