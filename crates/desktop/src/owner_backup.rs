use crate::{actions::Action, sand_panel as panel};
use bevy::{prelude::*, text::EditableText};
use cell::owner_backup::{BackupRequest, Passphrase};
use std::sync::{Arc, Mutex};

#[derive(Resource, Clone, Default)]
pub struct BackupHandoff(Arc<Mutex<Option<BackupRequest>>>);

impl BackupHandoff {
    pub fn take(&self) -> std::io::Result<Option<BackupRequest>> {
        self.0
            .lock()
            .map(|mut request| request.take())
            .map_err(|_| std::io::Error::other("Owner backup handoff is unavailable"))
    }

    fn submit(&self, request: BackupRequest) -> Result<(), &'static str> {
        let mut held = self
            .0
            .lock()
            .map_err(|_| "Owner backup handoff is unavailable")?;
        if held.is_some() {
            return Err("An owner backup is already waiting for shutdown");
        }
        *held = Some(request);
        Ok(())
    }
}

#[derive(Component)]
struct Form {
    destination: Entity,
    passphrase: Entity,
    confirmation: Entity,
    status: Entity,
}

#[derive(Component)]
struct Mask(Entity);

pub(crate) struct BackupPlugin;

impl Plugin for BackupPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AppExit>()
            .add_systems(PostUpdate, protect.before(bevy::text::EditableTextSystems))
            .add_systems(
                PostUpdate,
                masks
                    .after(bevy::text::EditableTextSystems)
                    .before(bevy::a11y::AccessibilitySystems::Update),
            )
            .add_systems(Last, cancelled.after(crate::workspace::persist));
    }
}

pub(crate) fn populate(world: &mut World, parent: Entity) {
    let form = panel::column(world, parent);
    crate::edit_mode::label(world, form, "Encrypted owner backup", 16.0);
    crate::edit_mode::label(
        world,
        form,
        "Use the owner device. Lince closes, captures retained Records and owner keys, then reopens. Choose a new file outside the data folder. External attachments, interface layouts and live messaging sessions are excluded. Database limit: 1 GiB. Copying the complete data directory requires Lince to be stopped.",
        13.0,
    );
    let destination = panel::field(world, form, "New backup file", "");
    let passphrase = secret_field(world, form, "Backup passphrase");
    let confirmation = secret_field(world, form, "Repeat backup passphrase");
    let status = crate::edit_mode::label(
        world,
        form,
        "Use a long passphrase and keep it with your backup; it is required to open the file.",
        13.0,
    );
    crate::accessibility::status(world, status);
    panel::button(world, form, form, "Back up and reopen Lince", Submit);
    world.entity_mut(form).insert(Form {
        destination,
        passphrase,
        confirmation,
        status,
    });
}

fn secret_field(world: &mut World, parent: Entity, caption: &str) -> Entity {
    crate::edit_mode::label(world, parent, caption, 13.0);
    let bundle = crate::sand::text_editor("", world.resource::<crate::theme::Typography>(), 0);
    let field = world.spawn((bundle, ChildOf(parent))).id();
    world
        .entity_mut(field)
        .remove::<crate::token_style::TextToken>()
        .insert(TextColor(Color::NONE));
    {
        let mut text = world.get_mut::<EditableText>(field).unwrap();
        text.max_characters = Some(4096);
        text.allow_newlines = false;
        text.visible_lines = Some(1.0);
    }
    crate::accessibility::input(world, field, caption, false);
    crate::accessibility::secret(world, field);
    let font = world.resource::<crate::theme::Typography>().text(16.0);
    world.spawn((
        Text::new(""),
        font,
        crate::token_style::text(crate::tokens::Token::Ink),
        Mask(field),
        Pickable::IGNORE,
        Node {
            position_type: PositionType::Absolute,
            left: px(4),
            top: px(4),
            ..default()
        },
        ChildOf(field),
    ));
    field
}

fn clear(world: &mut World, field: Entity) {
    if let Some(mut text) = world.get_mut::<EditableText>(field) {
        *text = crate::sand::editable("");
        text.max_characters = Some(4096);
        text.allow_newlines = false;
        text.visible_lines = Some(1.0);
    }
}

fn value(world: &World, field: Entity) -> Result<String, String> {
    let input = world
        .get::<EditableText>(field)
        .ok_or("Field is unavailable")?;
    if input.is_composing()
        || input.pending_paste.is_some()
        || input.pending_edits.iter().any(|edit| {
            matches!(
                edit,
                bevy::text::TextEdit::Insert(_)
                    | bevy::text::TextEdit::Paste
                    | bevy::text::TextEdit::Cut
                    | bevy::text::TextEdit::Backspace
                    | bevy::text::TextEdit::BackspaceWord
                    | bevy::text::TextEdit::Delete
                    | bevy::text::TextEdit::DeleteWord
                    | bevy::text::TextEdit::ImeSetCompose { .. }
                    | bevy::text::TextEdit::ImeCommit { .. }
            )
        })
    {
        return Err("Finish typing or pasting before creating the backup".into());
    }
    Ok(input.value().to_string())
}

#[derive(Clone)]
struct Submit;

impl Action for Submit {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(form) = world.get::<Form>(owner) else {
            return;
        };
        let (destination, passphrase, confirmation, status) = (
            form.destination,
            form.passphrase,
            form.confirmation,
            form.status,
        );
        let result = (|| {
            if crate::practice_cells::source(world, owner).is_some() {
                return Err("This is a practice form. Create owner backups from your ordinary Configuration.".to_string());
            }
            if crate::laboratory::active(world) {
                return Err("Backups are unavailable in the Laboratory".to_string());
            }
            let handoff = world
                .get_resource::<BackupHandoff>()
                .cloned()
                .ok_or("Open the native application to create an owner backup")?;
            let path = value(world, destination)?;
            if path.is_empty() || path.len() > 4096 {
                return Err("Choose a backup filename of at most 4096 bytes".into());
            }
            let secret =
                Passphrase::new(value(world, passphrase)?).map_err(|error| error.to_string())?;
            let repeated =
                Passphrase::new(value(world, confirmation)?).map_err(|error| error.to_string())?;
            if secret.expose() != repeated.expose() {
                return Err("The two backup passphrases must match".into());
            }
            handoff
                .submit(BackupRequest {
                    destination: path.into(),
                    passphrase: secret,
                })
                .map_err(str::to_owned)?;
            Ok(())
        })();
        clear(world, passphrase);
        clear(world, confirmation);
        match result {
            Ok(()) => {
                panel::status(
                    world,
                    status,
                    "Closing Lince to create the encrypted owner backup. The result appears after reopening.",
                );
                world.write_message(AppExit::Success);
            }
            Err(error) => panel::status(world, status, &error),
        }
    }
}

fn protect(forms: Query<&Form>, mut fields: Query<&mut EditableText>) {
    for form in &forms {
        for field in [form.passphrase, form.confirmation] {
            if let Ok(mut text) = fields.get_mut(field) {
                text.pending_edits.retain(|edit| {
                    !matches!(edit, bevy::text::TextEdit::Copy | bevy::text::TextEdit::Cut)
                });
            }
        }
    }
}

fn masks(world: &mut World) {
    let values: Vec<_> = world
        .query::<(Entity, &Mask)>()
        .iter(world)
        .map(|(entity, mask)| {
            (
                entity,
                world
                    .get::<EditableText>(mask.0)
                    .map_or(0, |text| text.value().chars().count()),
            )
        })
        .collect();
    for (entity, count) in values {
        if let Some(mut text) = world.get_mut::<Text>(entity) {
            text.set_if_neq(Text::new("•".repeat(count.min(32))));
        }
    }
}

fn cancelled(world: &mut World) {
    if !world.resource::<Messages<AppExit>>().is_empty() {
        return;
    }
    let Some(handoff) = world.get_resource::<BackupHandoff>() else {
        return;
    };
    if handoff.take().is_ok_and(|request| request.is_some()) {
        let statuses: Vec<_> = world
            .query::<&Form>()
            .iter(world)
            .map(|form| form.status)
            .collect();
        for status in statuses {
            panel::status(
                world,
                status,
                "Backup cancelled because Lince could not close. Resolve unsaved work or storage errors, then try again.",
            );
        }
    }
}

#[cfg(test)]
mod tests;
