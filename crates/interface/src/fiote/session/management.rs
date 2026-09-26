use super::*;

#[derive(Clone)]
pub(super) struct Open;
impl Action for Open {
    fn apply(&self, world: &mut World, owner: Entity) {
        if super::open_setup(world, owner) {
            return;
        }
        super::show(world, owner, Step::Manage);
        let record = world.get::<Panel>(owner).unwrap().binding.uid.clone();
        request(world, owner, FioteRequest::Prepare { record });
    }
}

pub(super) fn show(world: &mut World, owner: Entity, content: Entity, saved: Option<&FioteStatus>) {
    let Some(saved) = saved else { return };
    crate::edit_mode::label(world, content, "Fiotes", 22.0);
    let list = section(world, content);
    for choice in &saved.fiotes {
        crate::description::button(
            world,
            list,
            owner,
            &choice.title,
            Select(choice.record.clone()),
        );
    }
    crate::description::button(world, content, owner, "Create Fiote", Create);
    crate::edit_mode::label(world, content, "Selected Fiote", 18.0);
    let binding = world.get::<Panel>(owner).unwrap().binding.clone();
    if let Some(source) = saved.instructions.last() {
        for (property, label, text) in [
            ("head", "Name", &source.title),
            ("body", "System prompt", &source.body),
        ] {
            crate::edit_mode::label(world, content, label, 14.0);
            let editor = world
                .spawn((
                    crate::sand::text_editor(text, world.resource::<crate::theme::Typography>(), 0),
                    ChildOf(content),
                ))
                .id();
            if property == "head" {
                world
                    .get_mut::<EditableText>(editor)
                    .unwrap()
                    .allow_newlines = false;
            }
            crate::record_binding::attach(world, editor, binding.clone(), property, None);
        }
    }
    crate::description::button(
        world,
        content,
        owner,
        "Provider, model and settings",
        Connection,
    );
    crate::description::button(
        world,
        content,
        owner,
        "Open Record and conversation",
        OpenRecord,
    );
    crate::description::button(world, content, owner, "Delete Fiote…", ConfirmDelete);
    crate::description::button(
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
    let parent = saved
        .behavior
        .prompt_parent
        .as_ref()
        .and_then(|uid| saved.fiotes.iter().find(|choice| &choice.record == uid));
    crate::edit_mode::label(
        world,
        content,
        &format!(
            "Prompt parent: {}",
            parent.map_or("None", |choice| choice.title.as_str())
        ),
        14.0,
    );
    let parents = collapsed(world, content, "Change prompt parent");
    crate::description::button(world, parents, owner, "No prompt parent", Parent(None));
    for choice in saved
        .fiotes
        .iter()
        .filter(|choice| choice.record != saved.record)
    {
        crate::description::button(
            world,
            parents,
            owner,
            &choice.title,
            Parent(Some(choice.record.clone())),
        );
    }
    crate::edit_mode::label(
        world,
        content,
        "Instructions for new sessions · ancestor first",
        16.0,
    );
    crate::edit_mode::label(
        world,
        content,
        "Sessions pin these descriptions. After editing a prompt, apply it to an idle thread or start a new thread. Compaction reloads the pinned instructions.",
        14.0,
    );
    let instructions = collapsed(world, content, "View effective instructions");
    if let Some(error) = &saved.instruction_error {
        crate::edit_mode::label(world, instructions, error, 14.0);
    }
    for source in &saved.instructions {
        crate::edit_mode::label(
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
    crate::edit_mode::label(world, content, "Assigned work", 16.0);
    let tasks = collapsed(world, content, "Show assigned work");
    let view_uid = world.get::<Panel>(owner).unwrap().view_uid.clone();
    for task in saved
        .tasks
        .iter()
        .filter(|task| view_uid == saved.record || task.task == view_uid)
    {
        crate::edit_mode::label(
            world,
            tasks,
            &format!("{} · {}\n{}", task.title, task.state, task.detail),
            14.0,
        );
        crate::description::button(
            world,
            tasks,
            owner,
            "Open task thread",
            OpenTask(task.thread.clone()),
        );
        if matches!(task.state.as_str(), "failed" | "interrupted" | "finished") {
            crate::description::button(
                world,
                tasks,
                owner,
                "Retry task after inspecting changes",
                Retry(task.thread.clone()),
            );
        }
    }
}

fn section(world: &mut World, parent: Entity) -> Entity {
    let entity = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                max_height: px(240),
                flex_shrink: 0.0,
                row_gap: px(4),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    crate::scroll_sand::attach(world, entity);
    entity
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
struct Connection;
impl Action for Connection {
    fn apply(&self, world: &mut World, owner: Entity) {
        super::show(world, owner, Step::Agent);
    }
}

#[derive(Clone)]
struct OpenTask(String);
impl Action for OpenTask {
    fn apply(&self, world: &mut World, owner: Entity) {
        let panel = world.get::<Panel>(owner).unwrap();
        let binding = RecordBinding {
            uid: panel.view_uid.clone(),
            ..panel.binding.clone()
        };
        super::show(world, owner, Step::Closed);
        crate::thread_castle::select_thread(world, &binding, &self.0);
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
    for entity in controls {
        world.get_mut::<Text>(entity).unwrap().0 = text.clone();
    }
}

#[derive(Clone)]
struct Select(String);
impl Action for Select {
    fn apply(&self, world: &mut World, owner: Entity) {
        let area = world.get::<Panel>(owner).unwrap().binding.area;
        if let Some(mut area) = world.get_mut::<crate::area::InfluenceArea>(area) {
            if let Some(config) = &mut area.protein {
                if config.fiote {
                    config.draft.query["where"] = serde_json::json!([{"uid_eq": self.0}]);
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
        crate::full_record::Open(binding).apply(world, owner);
    }
}
#[derive(Clone)]
struct ConfirmDelete;
impl Action for ConfirmDelete {
    fn apply(&self, world: &mut World, owner: Entity) {
        let content = world.get::<Panel>(owner).unwrap().content;
        crate::edit_mode::label(
            world,
            content,
            "Delete this Fiote Record? Its conversations are retained.",
            14.0,
        );
        crate::description::button(world, content, owner, "Confirm deletion", Delete);
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

fn collapsed(world: &mut World, parent: Entity, label: &str) -> Entity {
    let content = section(world, parent);
    world.get_mut::<Node>(content).unwrap().display = Display::None;
    crate::description::button(world, parent, parent, label, ToggleSection(content));
    content
}
#[derive(Clone)]
struct ToggleSection(Entity);
impl Action for ToggleSection {
    fn apply(&self, world: &mut World, _: Entity) {
        if let Some(mut node) = world.get_mut::<Node>(self.0) {
            node.display = if node.display == Display::None {
                Display::Flex
            } else {
                Display::None
            };
        }
    }
}
