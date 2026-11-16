use super::*;
use bevy::ecs::query::QueryState;

#[derive(Resource)]
pub(super) struct Owners {
    query: QueryState<Entity, With<Editor>>,
    pub(super) pending: Vec<Entity>,
}

impl FromWorld for Owners {
    fn from_world(world: &mut World) -> Self {
        Self {
            query: world.query_filtered(),
            pending: Vec::new(),
        }
    }
}

impl Owners {
    pub(super) fn collect(&mut self, world: &World) {
        self.pending.clear();
        self.pending.extend(self.query.iter(world));
    }
}

#[derive(Default)]
pub(in crate::native::layout::desktop::opening::ui) struct Presentation {
    content: Content,
    feedback: Feedback,
}

#[derive(Default)]
struct Content {
    root: Option<Entity>,
    controls: Vec<Entity>,
    stack: Vec<Entity>,
    disabled: Option<bool>,
}

impl Presentation {
    pub(super) fn invalidate_content(&mut self) {
        self.content.root = None;
    }

    pub(super) fn enable_content(&mut self, world: &mut World, root: Entity, disabled: bool) {
        let changed = self.content.root != Some(root);
        if !changed && self.content.disabled == Some(disabled) {
            return;
        }
        let _content = engine::performance::Span::new("opening-editor-content");
        if changed {
            self.content.controls.clear();
            self.content.stack.clear();
            self.content.stack.push(root);
            while let Some(entity) = self.content.stack.pop() {
                if let Some(children) = world.get::<Children>(entity) {
                    self.content.stack.extend(children.iter());
                }
                if world.get::<EditableText>(entity).is_some()
                    || world.get::<ActionButton>(entity).is_some()
                {
                    self.content.controls.push(entity);
                }
            }
            self.content.root = Some(root);
            engine::performance::add("opening-editor-content-walks", 1);
            engine::performance::add(
                "opening-editor-controls-derived",
                self.content.controls.len() as u64,
            );
        }
        for &entity in &self.content.controls {
            // Controls come from this private editor's construction paths. Recheck
            // ownership before applying a later state transition to a retained ID.
            if belongs(world, entity, root)
                && (world.get::<EditableText>(entity).is_some()
                    || world.get::<ActionButton>(entity).is_some())
            {
                disable(world, entity, disabled);
            }
        }
        self.content.disabled = Some(disabled);
    }

    pub(super) fn feedback(&mut self, error: Option<&str>, message: &str) -> &str {
        if self.feedback.error.as_deref() != error || self.feedback.message != message {
            self.feedback.error = error.map(str::to_owned);
            message.clone_into(&mut self.feedback.message);
            self.feedback.text.clear();
            if let Some(error) = error {
                self.feedback.text.push_str(error);
                if !message.is_empty() {
                    self.feedback.text.push('\n');
                }
            }
            self.feedback.text.push_str(message);
            engine::performance::add("settings-feedback-prepared", 1);
        }
        &self.feedback.text
    }
}

fn belongs(world: &World, mut entity: Entity, root: Entity) -> bool {
    for _ in 0..crate::native::layout::MAX_DEPTH {
        if entity == root {
            return true;
        }
        let Some(parent) = world.get::<ChildOf>(entity) else {
            return false;
        };
        entity = parent.parent();
    }
    false
}

#[derive(Default)]
struct Feedback {
    error: Option<String>,
    message: String,
    text: String,
}
