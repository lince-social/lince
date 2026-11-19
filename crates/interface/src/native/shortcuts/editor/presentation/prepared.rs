use super::*;
use crate::host::input::{KeyLabel, KeyboardSnapshot};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Default)]
pub(in super::super) struct Prepared(Option<Recipe>);

pub(super) struct Recipe {
    draft: Settings,
    origin: Settings,
    labels: Option<Arc<BTreeMap<String, KeyLabel>>>,
    pub rows: BTreeMap<Shortcut, RowText>,
    pub review: BTreeSet<Shortcut>,
    pub changed: usize,
    pub count: usize,
    pub conflicting: bool,
    pub validation: Option<String>,
}

pub(super) struct RowText {
    pub display: String,
    pub search: String,
    pub accessibility: String,
    pub message: String,
    pub conflicting: bool,
}

impl Prepared {
    pub(super) fn observe(
        &mut self,
        draft: &Settings,
        origin: &Settings,
        keyboard: Option<&KeyboardSnapshot>,
    ) -> &Recipe {
        let labels = keyboard.map(|keyboard| &keyboard.labels);
        let current = self.0.as_ref().is_some_and(|recipe| {
            recipe.draft == *draft
                && recipe.origin == *origin
                && match (recipe.labels.as_ref(), labels) {
                    (Some(previous), Some(current)) => Arc::ptr_eq(previous, current),
                    (None, None) => true,
                    _ => false,
                }
        });
        if !current {
            let _preparation = engine::performance::Span::new("shortcut-editor-preparation");
            let conflicts = draft.conflict_groups();
            let mut sharing = BTreeMap::new();
            let mut review = BTreeSet::new();
            for (_, commands) in conflicts {
                for command in commands {
                    sharing.entry(*command).or_insert(commands);
                    review.insert(*command);
                }
            }
            let mut changed = 0;
            let mut rows = BTreeMap::new();
            for shortcut in Shortcut::ALL {
                let raw_changed = draft.text(shortcut) != origin.text(shortcut);
                if raw_changed || draft.chord(shortcut) != origin.chord(shortcut) {
                    changed += 1;
                    review.insert(shortcut);
                }
                let display = draft.display_text(shortcut, keyboard);
                let search = format!(
                    "{} {} {} {}",
                    shortcut.label(),
                    shortcut.id(),
                    shortcut.scope().label(),
                    display
                )
                .to_lowercase();
                let accessibility = format!("Change {}: {display}", shortcut.label());
                let commands = sharing.get(&shortcut);
                let message = if let Some(commands) = commands {
                    let mut message = String::from("Conflicts with: ");
                    let mut first = true;
                    for command in commands.iter().filter(|command| **command != shortcut) {
                        if !first {
                            message.push_str(", ");
                        }
                        first = false;
                        message.push_str(command.label());
                    }
                    message
                } else if raw_changed || origin.trigger(shortcut) != draft.trigger(shortcut) {
                    format!(
                        "Was {} · changed in draft",
                        origin
                            .trigger(shortcut)
                            .map_or_else(|| "Unbound".into(), Trigger::text)
                    )
                } else {
                    String::new()
                };
                rows.insert(
                    shortcut,
                    RowText {
                        display,
                        search,
                        accessibility,
                        message,
                        conflicting: commands.is_some(),
                    },
                );
            }
            engine::performance::add("shortcut-editor-rows-prepared", rows.len() as u64);
            self.0 = Some(Recipe {
                draft: draft.clone(),
                origin: origin.clone(),
                labels: labels.cloned(),
                rows,
                review,
                changed,
                count: conflicts.iter().map(|(_, commands)| commands.len()).sum(),
                conflicting: !conflicts.is_empty(),
                validation: draft.validate().err(),
            });
        }
        self.0.as_ref().unwrap()
    }
}
