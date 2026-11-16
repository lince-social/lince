use super::*;
use crate::{
    host::{external::screenshot::Options, launch::Capability},
    native::layout::desktop::input_settings::prepared::Prepared,
};

// Selected field values replace program/arguments/directory. Only the other
// commands and selected metadata belong in the non-field preparation key.
struct Source {
    selected: ExternalAction,
    others: Settings,
    mode: Mode,
}

enum Mode {
    Screenshot(bool),
    Custom {
        allow_when_locked: bool,
        capabilities: Vec<Capability>,
    },
}

impl Source {
    fn matches(&self, page: &PageState) -> bool {
        self.selected == page.selected
            && self.others.0.iter().eq(page
                .draft
                .0
                .iter()
                .filter(|(action, _)| **action != page.selected))
            && match (&self.mode, &page.capture_options) {
                (Mode::Screenshot(pointer), Some(options)) => *pointer == options.show_pointer,
                (
                    Mode::Custom {
                        allow_when_locked,
                        capabilities,
                    },
                    None,
                ) => {
                    if let Some(command) = page.draft.0.get(&page.selected) {
                        *allow_when_locked == command.allow_when_locked
                            && *capabilities == command.command.capabilities
                    } else {
                        !*allow_when_locked
                            && capabilities.iter().copied().eq(page.selected.capability())
                    }
                }
                _ => false,
            }
    }

    fn capture(page: &PageState) -> Self {
        let mode = if let Some(options) = &page.capture_options {
            Mode::Screenshot(options.show_pointer)
        } else {
            let command = page.draft.0.get(&page.selected);
            Mode::Custom {
                allow_when_locked: command.is_some_and(|command| command.allow_when_locked),
                capabilities: command.map_or_else(
                    || page.selected.capability().into_iter().collect(),
                    |command| command.command.capabilities.clone(),
                ),
            }
        };
        Self {
            selected: page.selected,
            others: Settings(
                page.draft
                    .0
                    .iter()
                    .filter(|(action, _)| **action != page.selected)
                    .map(|(action, command)| (*action, command.clone()))
                    .collect(),
            ),
            mode,
        }
    }
}

pub(super) struct Capture {
    command: Option<Command>,
    options: Option<Options>,
    pub(super) settings: Settings,
    validation: Result<(), String>,
}

impl Capture {
    fn read(world: &World, page: &PageState) -> Result<Self, String> {
        let (command, options) = if let Some(mut options) = page.capture_options.clone() {
            options.directory = value(world, page.capture_directory)?;
            options.validate()?;
            let command = if options == Default::default() {
                None
            } else {
                Some(Command {
                    command: options
                        .command(page.selected)
                        .ok_or("Choose a screenshot action")?,
                    allow_when_locked: false,
                })
            };
            (command, Some(options))
        } else {
            let mut command = empty(page.selected);
            if let Some(previous) = page.draft.0.get(&page.selected) {
                command.allow_when_locked = previous.allow_when_locked;
                command
                    .command
                    .capabilities
                    .clone_from(&previous.command.capabilities);
            }
            command.command.program = value(world, page.program)?;
            let directory = value(world, page.directory)?;
            command.command.directory = (!directory.is_empty()).then(|| directory.into());
            command.command.arguments = page
                .arguments
                .iter()
                .map(|entity| value(world, *entity))
                .collect::<Result<_, _>>()?;
            (Some(command), None)
        };
        let mut settings = Settings(
            page.draft
                .0
                .iter()
                .filter(|(action, command)| {
                    **action != page.selected && !command.command.program.is_empty()
                })
                .map(|(action, command)| (*action, command.clone()))
                .collect(),
        );
        if let Some(command) = &command
            && !command.command.program.is_empty()
        {
            settings.0.insert(page.selected, command.clone());
        }
        let validation = settings.validate();
        Ok(Self {
            command,
            options,
            settings,
            validation,
        })
    }

    fn apply(&self, page: &mut PageState) {
        if page.draft.0.get(&page.selected) != self.command.as_ref() {
            if let Some(command) = &self.command {
                page.draft.0.insert(page.selected, command.clone());
            } else {
                page.draft.0.remove(&page.selected);
            }
        }
        if page.capture_options != self.options {
            page.capture_options.clone_from(&self.options);
        }
    }

    pub(super) fn validated(&self) -> Result<&Settings, &String> {
        self.validation.as_ref().map(|()| &self.settings)
    }
}

#[derive(Default)]
pub(super) struct Draft {
    fields: Prepared<Source, Capture>,
}

fn fields(page: &PageState) -> impl Iterator<Item = Entity> + Clone + '_ {
    let screenshot = page.capture_options.is_some();
    std::iter::once(page.capture_directory)
        .filter(move |_| screenshot)
        .chain(
            [page.program, page.directory]
                .into_iter()
                .filter(move |_| !screenshot),
        )
        .chain(page.arguments.iter().copied().filter(move |_| !screenshot))
}

impl Draft {
    pub(super) fn prepare(
        &mut self,
        world: &World,
        page: &mut PageState,
    ) -> &Result<Capture, String> {
        let _draft = engine::performance::Span::new("external-settings-draft");
        let result = self.fields.prepare_matching(
            world,
            fields(page),
            |previous| previous.matches(page),
            || Source::capture(page),
            || Capture::read(world, page),
        );
        // Draft capture precedes full validation, including validation errors.
        // Explicit actions still receive the unvalidated configured settings.
        if let Ok(capture) = result {
            capture.apply(page);
        }
        result
    }
}
