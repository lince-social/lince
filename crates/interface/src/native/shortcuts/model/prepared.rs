// non-lean-yet
// Scope: struct Prepared.
// BeautifulRust (baseline lines):
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   69.

use super::{Settings, Shortcut, Trigger};
use crate::native::shortcuts::{DesktopBinding, PointerBinding};
use std::{
    collections::{BTreeMap, HashMap},
    sync::OnceLock,
};

pub(super) struct Prepared {
    pub(super) triggers: BTreeMap<Shortcut, Result<Option<Trigger>, String>>,
    pub(super) desktop: Vec<DesktopBinding>,
    pub(super) pointers: Vec<PointerBinding>,
    conflicts: OnceLock<Vec<(Trigger, Vec<Shortcut>)>>,
}

impl Prepared {
    pub(super) fn new(settings: &Settings) -> Self {
        let _preparation = engine::performance::Span::new("shortcut-settings-preparation");
        let mut result = Self {
            triggers: BTreeMap::new(),
            desktop: Vec::new(),
            pointers: Vec::new(),
            conflicts: OnceLock::new(),
        };
        for shortcut in Shortcut::ALL {
            let text = settings.text(shortcut);
            let parsed = if text.eq_ignore_ascii_case("unbound") {
                Ok(None)
            } else {
                settings.parse(shortcut, text).map(Some)
            };
            if let Ok(Some(trigger)) = &parsed {
                match *trigger {
                    Trigger::Key(chord) => {
                        if let Some(action) = shortcut.desktop_action() {
                            result.desktop.push(DesktopBinding {
                                key: chord.key,
                                modifiers: chord.modifiers,
                                action,
                                repeat: shortcut.repeats(),
                            });
                        }
                    }
                    Trigger::Pointer(chord) => {
                        if let Some(operation) = shortcut.pointer_action() {
                            result.pointers.push(PointerBinding { operation, chord });
                        }
                    }
                }
            }
            result.triggers.insert(shortcut, parsed);
        }
        engine::performance::add("shortcut-settings-prepared", 1);
        result
    }

    pub(super) fn conflicts(&self) -> &[(Trigger, Vec<Shortcut>)] {
        self.conflicts.get_or_init(|| {
            let _preparation = engine::performance::Span::new("shortcut-conflict-preparation");
            let mut indices = HashMap::new();
            let mut groups: Vec<(Trigger, Vec<Shortcut>)> = Vec::new();
            for shortcut in Shortcut::ALL {
                let Some(Ok(Some(trigger))) = self.triggers.get(&shortcut) else {
                    continue;
                };
                let index = *indices.entry(*trigger).or_insert_with(|| {
                    groups.push((*trigger, Vec::new()));
                    groups.len() - 1
                });
                groups[index].1.push(shortcut);
            }
            groups.retain(|(_, commands)| commands.len() > 1);
            engine::performance::add("shortcut-conflicts-prepared", 1);
            groups
        })
    }
}
