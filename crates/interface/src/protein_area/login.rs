use super::*;
use crate::{
    actions::{Action, ActionButton},
    edit_mode::label,
    tokens::Token,
};
use bevy::text::EditableText;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Connecting,
    Locked,
    SigningIn,
    Loading,
    Live,
    Disconnected,
    Stopped,
}

#[derive(Component)]
pub(super) struct Form {
    area: Entity,
    organ: String,
    canvas: bool,
    title: Entity,
    status: Entity,
    fields: Entity,
    username: Entity,
    password: Entity,
    retry: Entity,
    stage: Option<Stage>,
}

#[derive(Component)]
struct PasswordMask(Entity);

#[derive(Clone)]
enum Command {
    Login,
    Reconnect,
}

fn field(world: &mut World, parent: Entity, title: &str, secret: bool) -> Entity {
    label(world, parent, title, 14.0);
    let bundle = crate::sand::text_editor("", world.resource::<crate::theme::Typography>(), 0);
    let entity = world.spawn((bundle, ChildOf(parent))).id();
    let mut text = world.get_mut::<EditableText>(entity).unwrap();
    text.allow_newlines = false;
    text.visible_lines = Some(1.0);
    text.max_characters = Some(if secret { 1024 } else { 256 });
    world.get_mut::<Node>(entity).unwrap().flex_shrink = 0.0;
    let mut accessible = accesskit::Node::new(if secret {
        accesskit::Role::PasswordInput
    } else {
        accesskit::Role::TextInput
    });
    accessible.set_label(title);
    world
        .entity_mut(entity)
        .insert(bevy::a11y::AccessibilityNode::from(accessible));
    if secret {
        world
            .entity_mut(entity)
            .remove::<crate::token_style::TextToken>()
            .insert(TextColor(Color::NONE));
        let font = world.resource::<crate::theme::Typography>().text(22.0);
        world.spawn((
            Text::new(""),
            font,
            crate::token_style::text(Token::Ink),
            PasswordMask(entity),
            Pickable::IGNORE,
            Node {
                position_type: PositionType::Absolute,
                left: px(6),
                top: px(6),
                ..default()
            },
            ChildOf(entity),
        ));
    }
    entity
}

fn button(
    world: &mut World,
    parent: Entity,
    form: Entity,
    title: &str,
    command: Command,
) -> Entity {
    let entity = world
        .spawn((
            crate::sand::button(0),
            crate::sand::Square,
            ActionButton::new(form, crate::actions![command]),
            crate::token_style::background(Token::Surface),
            crate::token_style::border(Token::Accent),
            Node {
                padding: UiRect::all(px(8)),
                border: UiRect::all(px(1)),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    label(world, entity, title, 16.0);
    entity
}

pub(super) fn form(world: &mut World, parent: Entity, area: Entity, organ: &str, canvas: bool) {
    let entity = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                padding: UiRect::all(px(12)),
                width: if canvas { Val::Auto } else { percent(100) },
                max_width: px(400),
                max_height: percent(100),
                position_type: if canvas {
                    PositionType::Absolute
                } else {
                    PositionType::Relative
                },
                left: if canvas { px(12) } else { Val::Auto },
                right: if canvas { px(12) } else { Val::Auto },
                top: if canvas { px(12) } else { Val::Auto },
                border: UiRect::all(px(1)),
                ..default()
            },
            crate::token_style::background(Token::Surface),
            crate::token_style::border(Token::Accent),
            ChildOf(parent),
        ))
        .id();
    crate::scroll_sand::attach(world, entity);
    let title = label(world, entity, "Connecting to Organ", 20.0);
    let name = world
        .query::<&crate::area::RecordProperties>()
        .iter(world)
        .find(|record| record.0["kind"] == "organ" && record.0["uid"] == organ)
        .and_then(|record| record.0["head"].as_str())
        .unwrap_or(organ)
        .to_owned();
    label(world, entity, &format!("Organ: {name}"), 14.0);
    let status = label(world, entity, "", 14.0);
    let fields = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(entity),
        ))
        .id();
    let username = field(world, fields, "Username", false);
    let password = field(world, fields, "Password", true);
    button(world, fields, entity, "Log in", Command::Login);
    label(
        world,
        fields,
        "One login unlocks all Areas using this Organ. Passwords are not saved.",
        14.0,
    );
    let retry = button(world, entity, entity, "Connect again", Command::Reconnect);
    world.entity_mut(entity).insert(Form {
        area,
        organ: organ.into(),
        canvas,
        title,
        status,
        fields,
        username,
        password,
        retry,
        stage: None,
    });
    refresh(world, entity);
}

fn stage(state: &State) -> Stage {
    if state.applied.as_ref().is_none_or(|config| !config.enabled) {
        Stage::Stopped
    } else if state.login_pending {
        Stage::SigningIn
    } else if state.login {
        Stage::Locked
    } else if state.ready {
        if state.revision == 0 {
            Stage::Loading
        } else {
            Stage::Live
        }
    } else if state.remote.is_none() || state.status != "Connecting" {
        Stage::Disconnected
    } else {
        Stage::Connecting
    }
}

fn refresh(world: &mut World, entity: Entity) {
    let Some(form) = world.get::<Form>(entity) else {
        return;
    };
    let (area, title, status, fields, retry, canvas, previous) = (
        form.area,
        form.title,
        form.status,
        form.fields,
        form.retry,
        form.canvas,
        form.stage,
    );
    let state = world
        .get_resource::<Runtime>()
        .and_then(|runtime| runtime.areas.get(&area));
    let current = state.map_or(Stage::Stopped, stage);
    let message = state
        .map_or("Start this Protein to connect.", |state| {
            state.status.as_str()
        })
        .to_owned();
    if previous != Some(current) && !matches!(current, Stage::Locked | Stage::SigningIn) {
        clear_passwords(world, area);
    }
    let heading = match current {
        Stage::Connecting => "Connecting to Organ",
        Stage::Locked => "Protein locked · log in",
        Stage::SigningIn => "Logging in…",
        Stage::Loading => "Loading remote Records…",
        Stage::Live => "Connected to Organ",
        Stage::Disconnected => "Protein locked · disconnected",
        Stage::Stopped => "Protein stopped",
    };
    world
        .get_mut::<Text>(title)
        .unwrap()
        .set_if_neq(Text::new(heading));
    world
        .get_mut::<Text>(status)
        .unwrap()
        .set_if_neq(Text::new(message));
    display(world, fields, current == Stage::Locked);
    display(
        world,
        retry,
        matches!(current, Stage::Disconnected | Stage::Stopped),
    );
    display(world, entity, !canvas || current != Stage::Live);
    if previous != Some(current) {
        world.get_mut::<Form>(entity).unwrap().stage = Some(current);
    }
}

fn display(world: &mut World, entity: Entity, visible: bool) {
    let display = if visible {
        Display::Flex
    } else {
        Display::None
    };
    if world
        .get::<Node>(entity)
        .is_some_and(|node| node.display != display)
    {
        world.get_mut::<Node>(entity).unwrap().display = display;
    }
}

pub(super) fn sync(world: &mut World) {
    let existing: Vec<_> = world
        .query::<(Entity, &Form)>()
        .iter(world)
        .map(|(entity, form)| (entity, form.area, form.organ.clone(), form.canvas))
        .collect();
    for (entity, area, organ, canvas) in existing {
        let config = configuration(world, area);
        if config.as_ref().is_none_or(|config| {
            config.source != Source::Organ(organ) || (canvas && !config.enabled)
        }) {
            world.despawn(entity);
        } else {
            refresh(world, entity);
        }
    }
    let owners: Vec<_> = world
        .resource::<Runtime>()
        .areas
        .keys()
        .copied()
        .filter(|owner| world.get::<InfluenceArea>(*owner).is_some())
        .filter_map(|owner| {
            let config = configuration(world, owner)?;
            if !config.enabled {
                return None;
            }
            let Source::Organ(organ) = config.source else {
                return None;
            };
            Some((owner, organ))
        })
        .collect();
    let present: std::collections::HashSet<_> = world
        .query::<&Form>()
        .iter(world)
        .filter(|form| form.canvas)
        .map(|form| form.area)
        .collect();
    for (owner, organ) in owners {
        if !present.contains(&owner) {
            form(world, owner, owner, &organ, true);
        }
    }
}

fn value(world: &World, entity: Entity) -> Result<String, String> {
    let text = world
        .get::<EditableText>(entity)
        .ok_or("Open the login form again.")?;
    if text.is_composing() || text.pending_paste.is_some() {
        return Err("Finish typing or pasting before logging in.".into());
    }
    Ok(text.value().to_string())
}

fn submit(world: &mut World, entity: Entity) -> Result<(), String> {
    let form = world
        .get::<Form>(entity)
        .ok_or("Open the login form again.")?;
    let state = world
        .resource::<Runtime>()
        .areas
        .get(&form.area)
        .ok_or("Connect to the Organ first.")?;
    if !state.login || state.ready || state.login_pending {
        return Err("Wait for the Organ to request a login.".into());
    }
    if state
        .applied
        .as_ref()
        .is_none_or(|config| !config.enabled || config.source != Source::Organ(form.organ.clone()))
    {
        return Err("The selected Organ changed. Open its login form again.".into());
    }
    let username = value(world, form.username)?.trim().to_owned();
    let password = value(world, form.password)?;
    let organ = form.organ.clone();
    if username.is_empty() || username.len() > 256 || password.is_empty() || password.len() > 1024 {
        return Err("Enter a username and password within the field limits.".into());
    }
    sessions::login(world, &organ, username, password)
}

impl Action for Command {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(form) = world.get::<Form>(entity) else {
            return;
        };
        let (area, organ) = (form.area, form.organ.clone());
        let target = world
            .get::<filter::Subscription>(area)
            .map_or(area, |filter| filter.0);
        let Some(root) = world.get::<ChildOf>(target).map(ChildOf::parent) else {
            return;
        };
        let Some(mut config) = configuration(world, area) else {
            return;
        };
        if !crate::area_panel::owns(world, root, target)
            || crate::laboratory::active(world)
            || crate::laboratory::suspended(world, target)
            || config.source != Source::Organ(organ.clone())
            || (matches!(self, Self::Login) && !config.enabled)
        {
            return;
        }
        if world
            .resource::<Runtime>()
            .areas
            .get(&area)
            .is_some_and(|state| state.login_pending)
        {
            return;
        }
        match self {
            Self::Reconnect => {
                config.enabled = true;
                set_configuration(world, area, Some(config.clone()));
                start(world, area, config);
                sessions::reconnect(world, &organ);
            }
            Self::Login => match submit(world, entity) {
                Ok(()) => {}
                Err(error) => {
                    if let Some(state) = world.resource_mut::<Runtime>().areas.get_mut(&area) {
                        state.status = error;
                    }
                }
            },
        }
        sync(world);
        if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
            wake.ring();
        }
    }
}

pub(super) fn clear_passwords(world: &mut World, area: Entity) {
    let fields: Vec<_> = world
        .query::<&Form>()
        .iter(world)
        .filter(|form| form.area == area)
        .map(|form| form.password)
        .collect();
    for entity in fields {
        if let Some(mut focus) = world.get_resource_mut::<bevy::input_focus::InputFocus>() {
            if focus.get() == Some(entity) {
                focus.clear();
            }
        }
        if let Some(mut text) = world.get_mut::<EditableText>(entity) {
            *text = crate::sand::editable("");
            text.allow_newlines = false;
            text.visible_lines = Some(1.0);
            text.max_characters = Some(1024);
        }
    }
}

pub(super) fn protect_passwords(forms: Query<&Form>, mut fields: Query<&mut EditableText>) {
    for form in &forms {
        if let Ok(mut field) = fields.get_mut(form.password) {
            field.pending_edits.retain(|edit| {
                !matches!(edit, bevy::text::TextEdit::Copy | bevy::text::TextEdit::Cut)
            });
        }
    }
}

pub(super) fn masks(world: &mut World) {
    let updates: Vec<_> = world
        .query::<(Entity, &PasswordMask)>()
        .iter(world)
        .map(|(entity, mask)| {
            (
                entity,
                world
                    .get::<EditableText>(mask.0)
                    .map_or(0, |field| field.value().chars().count()),
            )
        })
        .collect();
    for (entity, count) in updates {
        world
            .get_mut::<Text>(entity)
            .unwrap()
            .set_if_neq(Text::new("•".repeat(count.min(32))));
    }
}

#[cfg(test)]
mod tests;
