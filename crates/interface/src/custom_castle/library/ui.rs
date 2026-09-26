use super::*;
use bevy::text::EditableText;

#[derive(Component)]
struct View {
    root: Entity,
    organs: Entity,
    entries: Entity,
    status: Entity,
    source: Entity,
    name: Entity,
    observed_name: String,
    login: Entity,
    password: Entity,
}

#[derive(Component)]
struct PasswordMask(Entity);

pub(in crate::custom_castle) fn show(world: &mut World, root: Entity, parent: Entity) {
    if world.get::<Library>(root).is_none() {
        world.entity_mut(root).insert(Library::default());
    }
    let body = panel::column(world, parent);
    crate::edit_mode::label(world, body, "Organ components", 18.0);
    let source = crate::edit_mode::label(world, body, "", 14.0);
    let organs = panel::row(world, body);
    panel::button(world, body, root, "Refresh components", Command::Refresh);
    let status = crate::edit_mode::label(world, body, "", 14.0);
    let login = panel::column(world, body);
    let username = panel::field(world, login, "Username", "");
    let password = panel::field(world, login, "Password", "");
    world
        .entity_mut(password)
        .remove::<crate::token_style::TextToken>()
        .insert(TextColor(Color::NONE));
    let font = world.resource::<crate::theme::Typography>().text(18.0);
    world.spawn((
        Text::new(""),
        font,
        crate::token_style::text(crate::tokens::Token::Ink),
        Node {
            position_type: PositionType::Absolute,
            left: px(6),
            top: px(6),
            ..default()
        },
        Pickable::IGNORE,
        PasswordMask(password),
        ChildOf(password),
    ));
    panel::button(
        world,
        login,
        root,
        "Log in",
        Command::Login(username, password),
    );
    crate::edit_mode::label(
        world,
        body,
        "Select Sands or a group on your canvas, then save a component to this Organ.",
        14.0,
    );
    let initial = world.get::<Library>(root).unwrap().name.clone();
    let name = panel::field(world, body, "Component name", &initial);
    world.get_mut::<EditableText>(name).unwrap().max_characters = Some(80);
    panel::button(
        world,
        body,
        root,
        "Save selection as new component",
        Command::Save(name),
    );
    let entries = panel::column(world, body);
    let controls = panel::row(world, body);
    for (caption, command) in [
        ("Add to canvas", Command::Add),
        ("Rename", Command::Rename(name)),
        ("Replace with selection", Command::Replace(name)),
        ("Delete component", Command::Delete),
    ] {
        panel::button(world, controls, root, caption, command);
    }
    world.entity_mut(body).insert(View {
        root,
        organs,
        entries,
        status,
        source,
        name,
        observed_name: initial,
        login,
        password,
    });
    world.get_mut::<Library>(root).unwrap().dirty = true;
    refresh(world, root);
}

pub(super) fn refresh(world: &mut World, root: Entity) {
    if !world
        .get::<Library>(root)
        .is_some_and(|library| library.dirty)
    {
        return;
    }
    let views: Vec<_> = world
        .query::<(Entity, &View)>()
        .iter(world)
        .filter(|(_, view)| view.root == root)
        .map(|(entity, _)| entity)
        .collect();
    if views.is_empty() {
        return;
    }
    let library = world.get::<Library>(root).unwrap();
    let (organ, contacts, entries, selected, name, status, login) = (
        library.organ.clone(),
        library.contacts.clone(),
        library.entries.clone(),
        library.selected.clone(),
        library.name.clone(),
        library.status.clone(),
        library.login,
    );
    for entity in views {
        let view = world.get::<View>(entity).unwrap();
        let (organs, list, output, source, field, login_panel, password) = (
            view.organs,
            view.entries,
            view.status,
            view.source,
            view.name,
            view.login,
            view.password,
        );
        if view.observed_name != name {
            world
                .get_mut::<EditableText>(field)
                .unwrap()
                .editor
                .set_text(&name);
            world.get_mut::<View>(entity).unwrap().observed_name = name.clone();
        }
        panel::status(world, output, &status);
        let title = organ
            .as_ref()
            .map(|uid| {
                contacts
                    .iter()
                    .find(|row| row["uid"] == *uid)
                    .and_then(|row| row["head"].as_str())
                    .unwrap_or(uid)
            })
            .unwrap_or("Local Organ");
        panel::status(world, source, format!("Library: {title}"));
        world.get_mut::<Node>(login_panel).unwrap().display =
            if login { Display::Flex } else { Display::None };
        if !login {
            world
                .get_mut::<EditableText>(password)
                .unwrap()
                .editor
                .set_text("");
        }
        panel::clear(world, organs);
        panel::button(world, organs, root, "Local Organ", Command::Organ(None));
        for row in &contacts {
            if row["slug"] == "local-organ" {
                continue;
            }
            let Some(uid) = row["uid"].as_str() else {
                continue;
            };
            let title = row["head"].as_str().unwrap_or(uid);
            panel::button(world, organs, root, title, Command::Organ(Some(uid.into())));
        }
        panel::clear(world, list);
        if entries.is_empty() {
            crate::edit_mode::label(world, list, "No components in this Organ.", 14.0);
        }
        for row in &entries {
            let Some(uid) = row["uid"].as_str() else {
                continue;
            };
            let title = row["head"].as_str().unwrap_or("Component");
            let caption = if selected.as_deref() == Some(uid) {
                format!("Selected: {title}")
            } else {
                title.into()
            };
            panel::button(world, list, root, &caption, Command::Select(uid.into()));
        }
    }
    world.get_mut::<Library>(root).unwrap().dirty = false;
}

pub(super) fn password_masks(world: &mut World) {
    let changes: Vec<_> = world
        .query::<(Entity, &PasswordMask, &Text)>()
        .iter(world)
        .filter_map(|(entity, mask, text)| {
            let length = world.get::<EditableText>(mask.0)?.value().chars().count();
            let value = "•".repeat(length.min(256));
            (text.0 != value).then_some((entity, value))
        })
        .collect();
    for (entity, value) in changes {
        world.get_mut::<Text>(entity).unwrap().0 = value;
    }
}
