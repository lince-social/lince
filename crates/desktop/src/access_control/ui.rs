use super::*;
use crate::{
    actions::{Action, ActionButton},
    edit_mode::label,
    tokens::Token,
};
use bevy::text::EditableText;

#[derive(Component)]
pub(super) struct UserForm {
    pub uid: Option<String>,
    username: Entity,
    name: Entity,
    password: Entity,
    role: String,
    roles: Vec<String>,
    expected_revision: i64,
    role_label: Entity,
    chooser: Entity,
    preview_record: Entity,
}

#[derive(Component)]
struct StandingEditor {
    parent: Entity,
    state: Value,
    note: Entity,
    confirm: Entity,
}

#[derive(Component)]
struct RoleForm(Entity);

#[derive(Component)]
struct RoleEdit {
    role: i64,
    revision: i64,
    original_name: String,
    confirm: Entity,
    permissions: Entity,
}

#[derive(Component)]
struct PasswordMask(Entity);

#[derive(Component)]
struct PolicyForm {
    role: String,
    revision: i64,
    raw: Entity,
    controls: Entity,
    signature: String,
}

#[derive(Clone)]
struct AcceptSelector {
    target: Entity,
    grant: Option<usize>,
}

impl Action for AcceptSelector {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(castle) = world.get::<crate::protein_castle::ProteinCastle>(owner) else {
            return;
        };
        let Ok(query) = castle.draft.compile() else {
            return;
        };
        if query.source != protein::Source::Record {
            return;
        }
        let Some(form) = world.get::<PolicyForm>(self.target) else {
            return;
        };
        let field = form.raw;
        let Ok(text) = value(world, field) else {
            return;
        };
        let Ok(mut policy) = serde_json::from_str::<protein::authority::RolePolicy>(&text) else {
            return;
        };
        let selector = protein::Predicate::All(query.filter);
        if let Some(index) = self.grant {
            let Some(grant) = policy.grants.get_mut(index) else {
                return;
            };
            grant.selector = selector;
        } else {
            policy.read = selector;
        }
        if let Ok(raw) = serde_json::to_string_pretty(&policy) {
            let mut text = crate::sand::editable(&raw);
            text.visible_lines = Some(8.0);
            text.max_characters = Some(65536);
            *world.get_mut::<EditableText>(field).unwrap() = text;
            status(
                world,
                self.target,
                "Selector updated. Save the policy to apply it.",
            );
        }
    }
}

#[derive(Clone)]
enum Command {
    Tab(Tab),
    Select(String),
    New,
    Refresh,
    Page(bool),
    ChooseRole(String),
    SaveUser,
    Preview,
    Assign,
    Delete,
    ConfirmDelete,
    CancelDelete,
    Restore,
    CreateRole,
    RenameRole,
    DeleteRole,
    ConfirmDeleteRole,
    CancelDeleteRole,
    Permission(String, bool),
    SavePolicy,
    Selector(Option<usize>),
    AddGrant(protein::authority::Operation),
    RemoveGrant(usize),
    Property(usize, protein::authority::Property),
    Assertion(usize, bool, Entity, Entity),
    RemoveAssertion(usize, bool, usize),
    AssertionProperty(usize, bool, usize, protein::authority::AssertionProperty),
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(view) = world.get::<AccessControlSand>(owner) else {
            return;
        };
        if view.pending.is_some() {
            status(world, owner, "Wait for the current change to finish.");
            return;
        }
        match self {
            Self::AddGrant(_)
            | Self::RemoveGrant(_)
            | Self::Property(_, _)
            | Self::Assertion(..)
            | Self::RemoveAssertion(..)
            | Self::AssertionProperty(..) => {
                edit_grant(world, owner, self);
            }
            Self::Selector(grant) => open_selector(world, owner, *grant),
            Self::Refresh => {
                world.resource_mut::<Catalog>().refresh = true;
                world.resource_mut::<Catalog>().ready = false;
                world
                    .get_mut::<AccessControlSand>(owner)
                    .unwrap()
                    .reload_editor = true;
                status(world, owner, "Refreshing…");
            }
            Self::Tab(tab) => {
                let mut view = world.get_mut::<AccessControlSand>(owner).unwrap();
                view.tab = *tab;
                view.page = 0;
                view.selected = None;
                view.dirty = true;
                editor(world, owner);
                status(world, owner, "Select an entry or create one.");
            }
            Self::Select(id) => {
                world.get_mut::<AccessControlSand>(owner).unwrap().selected = Some(id.clone());
                editor(world, owner);
                let message = if world.get::<AccessControlSand>(owner).unwrap().tab == Tab::Roles {
                    "Select a permission to turn it on or off."
                } else {
                    "Changes are saved only when submitted."
                };
                status(world, owner, message);
            }
            Self::New => {
                world.get_mut::<AccessControlSand>(owner).unwrap().selected = None;
                editor(world, owner);
            }
            Self::Page(next) => {
                let mut view = world.get_mut::<AccessControlSand>(owner).unwrap();
                view.page = if *next {
                    view.page.saturating_add(1)
                } else {
                    view.page.saturating_sub(1)
                };
                view.dirty = true;
            }
            Self::ChooseRole(role) => {
                let Some(mut form) = world.get_mut::<UserForm>(owner) else {
                    return;
                };
                form.role = role.clone();
                if let Some(index) = form.roles.iter().position(|selected| selected == role) {
                    form.roles.remove(index);
                } else {
                    form.roles.push(role.clone());
                }
                if form.uid.is_none() {
                    form.roles.retain(|selected| selected == role);
                }
                let caption = form.role_label;
                let names = form.roles.join(", ");
                world.get_mut::<Text>(caption).unwrap().0 = if names.is_empty() {
                    "No Roles".into()
                } else {
                    names
                };
            }
            Self::Delete | Self::CancelDelete => {
                let Some(form) = world.get::<StandingEditor>(owner) else {
                    return;
                };
                let confirm = form.confirm;
                world.get_mut::<Node>(confirm).unwrap().display = if matches!(self, Self::Delete) {
                    Display::Flex
                } else {
                    Display::None
                };
            }
            Self::DeleteRole | Self::CancelDeleteRole => {
                let Some(form) = world.get::<RoleEdit>(owner) else {
                    return;
                };
                let confirm = form.confirm;
                world.get_mut::<Node>(confirm).unwrap().display =
                    if matches!(self, Self::DeleteRole) {
                        Display::Flex
                    } else {
                        Display::None
                    };
            }
            _ => {
                if !world.resource::<Catalog>().ready {
                    status(
                        world,
                        owner,
                        "Refresh the users and Roles before making changes.",
                    );
                    return;
                }
                let result = mutation(world, owner, self);
                match result {
                    Ok((action, kind)) => request(world, owner, action, kind),
                    Err(message) => status(world, owner, &message),
                }
            }
        }
    }
}

fn value(world: &World, entity: Entity) -> Result<String, String> {
    let text = world
        .get::<EditableText>(entity)
        .ok_or("Open the editor again.")?;
    if text.is_composing() || text.pending_paste.is_some() {
        return Err("Finish typing or pasting before saving.".into());
    }
    Ok(text.value().to_string())
}

fn mutation(
    world: &World,
    owner: Entity,
    command: &Command,
) -> Result<(engine::actions::Action, Mutation), String> {
    use engine::actions::Action as Backend;
    match command {
        Command::Preview => {
            let form = world
                .get::<UserForm>(owner)
                .ok_or("Select an Actor first.")?;
            let person = form.uid.clone().ok_or("Create the Actor first.")?;
            let record_uid = value(world, form.preview_record)?.trim().to_owned();
            if !nucleus::valid_uid(&record_uid, "r") {
                return Err("Choose an existing Record UID to preview its authority.".into());
            }
            Ok((
                Backend::InspectRecordAuthority { person, record_uid },
                Mutation::Preview,
            ))
        }
        Command::SavePolicy => {
            let form = world
                .get::<PolicyForm>(owner)
                .ok_or("Select a Role first.")?;
            let policy: protein::authority::RolePolicy =
                serde_json::from_str(&value(world, form.raw)?)
                    .map_err(|error| error.to_string())?;
            Ok((
                Backend::SetRolePolicy {
                    role: form.role.clone(),
                    policy: serde_json::to_value(policy).map_err(|error| error.to_string())?,
                    expected_revision: form.revision,
                },
                Mutation::Policy,
            ))
        }
        Command::ConfirmDelete | Command::Restore => {
            let person = world
                .get::<UserForm>(owner)
                .and_then(|form| form.uid.clone())
                .ok_or("Select an existing user.")?;
            let form = world
                .get::<StandingEditor>(owner)
                .ok_or("Open the user editor again.")?;
            let entry = world
                .resource::<Catalog>()
                .rows
                .iter()
                .find(|entry| entry["kind"] == "user" && entry["id"] == person)
                .ok_or("User no longer exists. Refresh the list.")?;
            let active = matches!(command, Command::Restore);
            if active == (entry["active"] != false) {
                return Err("This user's access already changed. Refresh the list.".into());
            }
            let note = if active {
                None
            } else {
                if world
                    .get::<Node>(form.confirm)
                    .is_none_or(|node| node.display == Display::None)
                {
                    return Err("Confirm soft-deleting this user first.".into());
                }
                let note = value(world, form.note)?.trim().to_owned();
                if note.len() > 1024 {
                    return Err("Keep the note within 1024 bytes.".into());
                }
                (!note.is_empty()).then_some(note)
            };
            Ok((
                Backend::SetPersonStanding {
                    person,
                    active,
                    note,
                },
                Mutation::Standing,
            ))
        }
        Command::SaveUser | Command::Assign => {
            let form = world.get::<UserForm>(owner).ok_or("Select a user first.")?;
            match command {
                Command::Assign => {
                    let user = form.uid.clone().ok_or("Create the user first.")?;
                    Ok((
                        Backend::AssignRoles {
                            person: user,
                            roles: form.roles.clone(),
                            expected_revision: form.expected_revision,
                        },
                        Mutation::Assign,
                    ))
                }
                _ => {
                    let username = value(world, form.username)?.trim().to_owned();
                    let name = value(world, form.name)?.trim().to_owned();
                    let password = value(world, form.password)?;
                    if username.is_empty()
                        || name.is_empty()
                        || username.len() > 256
                        || name.len() > 500
                        || password.len() > 1024
                    {
                        return Err("Enter a username and name within the field limits.".into());
                    }
                    if let Some(user) = &form.uid {
                        Ok((
                            Backend::UpdateUser {
                                user: user.clone(),
                                username,
                                name,
                                password,
                            },
                            Mutation::SaveUser,
                        ))
                    } else {
                        if password.is_empty() || form.role.is_empty() {
                            return Err("A new user needs a password and one Role.".into());
                        }
                        Ok((
                            Backend::CreateUser {
                                username,
                                name,
                                password,
                                role: form.role.clone(),
                            },
                            Mutation::CreateUser,
                        ))
                    }
                }
            }
        }
        Command::CreateRole => {
            let form = world
                .get::<RoleForm>(owner)
                .ok_or("Open the new Role form.")?;
            let name = value(world, form.0)?.trim().to_owned();
            if name.is_empty() || name.len() > 100 || name.chars().any(char::is_control) {
                return Err("Enter a Role name of up to 100 bytes.".into());
            }
            if world
                .resource::<Catalog>()
                .rows
                .iter()
                .any(|row| row["kind"] == "role" && row["name"] == name)
            {
                return Err("That Role already exists. Select it to edit permissions.".into());
            }
            Ok((Backend::CreateRole { name }, Mutation::Role))
        }
        Command::RenameRole | Command::ConfirmDeleteRole => {
            let edit = world
                .get::<RoleEdit>(owner)
                .ok_or("Select an editable Role first.")?;
            if matches!(command, Command::RenameRole) {
                let field = world
                    .get::<RoleForm>(owner)
                    .ok_or("Open the Role editor again.")?
                    .0;
                let name = value(world, field)?.trim().to_owned();
                if name.is_empty() || name.len() > 100 || name.chars().any(char::is_control) {
                    return Err("Enter a Role name of up to 100 bytes.".into());
                }
                Ok((
                    Backend::RenameRole {
                        role: edit.role,
                        expected_revision: edit.revision,
                        name,
                    },
                    Mutation::RenameRole,
                ))
            } else {
                if world
                    .get::<Node>(edit.confirm)
                    .is_none_or(|node| node.display == Display::None)
                {
                    return Err("Confirm deleting this Role first.".into());
                }
                Ok((
                    Backend::DeleteRole {
                        role: edit.role,
                        expected_revision: edit.revision,
                    },
                    Mutation::DeleteRole,
                ))
            }
        }
        Command::Permission(permission, grant) => {
            let catalog = world.resource::<Catalog>();
            let known = catalog
                .rows
                .iter()
                .filter(|row| row["kind"] == "permission_catalog")
                .filter_map(|row| row["keys"].as_array())
                .flatten()
                .any(|key| key.as_str() == Some(permission));
            if !known {
                return Err("This permission is not in the Organ's catalog.".into());
            }
            let selected = world
                .get::<AccessControlSand>(owner)
                .and_then(|view| view.selected.as_ref())
                .ok_or("Select a Role first.")?;
            let role = catalog
                .rows
                .iter()
                .find(|row| row["kind"] == "role" && row["id"].as_str() == Some(selected))
                .ok_or("Role no longer exists. Refresh the list.")?;
            let name = role["name"].as_str().ok_or("Role has no name.")?.to_owned();
            Ok((
                if !grant {
                    Backend::RevokePermission {
                        role: name,
                        permission: permission.clone(),
                    }
                } else {
                    Backend::GrantPermission {
                        role: name,
                        permission: permission.clone(),
                    }
                },
                Mutation::Permission,
            ))
        }
        _ => Err("Choose an action.".into()),
    }
}

fn column(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

fn button(world: &mut World, parent: Entity, owner: Entity, title: &str, command: Command) {
    let entity = world
        .spawn((
            crate::sand::button(0),
            crate::sand::Square,
            ActionButton::new(owner, crate::actions![command]),
            crate::token_style::background(Token::Surface),
            crate::token_style::border(Token::Accent),
            Node {
                padding: UiRect::axes(px(8), px(5)),
                border: UiRect::all(px(1)),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    world
        .get_mut::<bevy::a11y::AccessibilityNode>(entity)
        .unwrap()
        .set_label(title);
    label(world, entity, title, 14.0);
}

fn input(world: &mut World, parent: Entity, title: &str, initial: &str, max: usize) -> Entity {
    label(world, parent, title, 14.0);
    let bundle = crate::sand::text_editor(initial, world.resource::<crate::theme::Typography>(), 0);
    let entity = world.spawn((bundle, ChildOf(parent))).id();
    world.entity_mut(entity).insert(Node {
        width: percent(100),
        min_height: px(32),
        flex_shrink: 0.0,
        padding: UiRect::all(px(4)),
        ..default()
    });
    let mut text = world.get_mut::<EditableText>(entity).unwrap();
    text.max_characters = Some(max);
    text.allow_newlines = false;
    text.visible_lines = Some(1.0);
    let mut accessible = accesskit::Node::new(accesskit::Role::TextInput);
    accessible.set_label(title);
    world
        .entity_mut(entity)
        .insert(bevy::a11y::AccessibilityNode::from(accessible));
    entity
}

pub(super) fn populate(world: &mut World, sand: Entity) {
    world
        .entity_mut(sand)
        .insert((
            crate::castle::Castle,
            crate::token_style::background(Token::Surface),
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                padding: UiRect::all(px(12)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
        ))
        .observe(
            |mut event: On<Pointer<bevy::picking::events::Scroll>>,
             mut scrolls: Query<&mut ScrollPosition>| {
                if let Ok(mut scroll) = scrolls.get_mut(event.entity) {
                    let step = if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
                        24.0
                    } else {
                        1.0
                    };
                    scroll.0.y = (scroll.0.y - event.y * step).max(0.0);
                    event.propagate(false);
                }
            },
        );
    label(world, sand, "Access Control", 22.0);
    label(world, sand, "Local Organ · owner connection", 14.0);
    let tabs = row(world, sand);
    button(world, tabs, sand, "Users", Command::Tab(Tab::Users));
    button(world, tabs, sand, "Roles", Command::Tab(Tab::Roles));
    button(world, tabs, sand, "Refresh", Command::Refresh);
    let status = label(world, sand, "Loading users and Roles…", 14.0);
    let list = column(world, sand);
    let editor = column(world, sand);
    world.entity_mut(sand).insert(AccessControlSand {
        list,
        editor,
        status,
        tab: Tab::Users,
        page: 0,
        selected: None,
        pending: None,
        dirty: true,
        reload_editor: true,
    });
    self::editor(world, sand);
}

pub(super) fn list(world: &mut World, owner: Entity) {
    let view = world.get::<AccessControlSand>(owner).unwrap();
    let parent = view.list;
    let tab = view.tab;
    let selected = view.selected.clone();
    let kind = if tab == Tab::Users { "user" } else { "role" };
    let mut rows: Vec<_> = world
        .resource::<Catalog>()
        .rows
        .iter()
        .filter(|row| row["kind"] == kind)
        .cloned()
        .collect();
    rows.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    let page = view.page.min(rows.len().saturating_sub(1) / 8);
    world.get_mut::<AccessControlSand>(owner).unwrap().page = page;
    world.entity_mut(parent).despawn_children();
    let heading = row(world, parent);
    label(
        world,
        heading,
        &format!(
            "{} ({})",
            if tab == Tab::Users { "Users" } else { "Roles" },
            rows.len()
        ),
        18.0,
    );
    button(
        world,
        heading,
        owner,
        if tab == Tab::Users {
            "New user"
        } else {
            "New Role"
        },
        Command::New,
    );
    for entry in rows.iter().skip(page * 8).take(8) {
        let Some(id) = entry["id"].as_str() else {
            continue;
        };
        let name = entry["name"].as_str().unwrap_or_default();
        let title = if tab == Tab::Users {
            format!(
                "{name} · {} · {}{}",
                entry["username"].as_str().unwrap_or_default(),
                &entry["roles"]
                    .as_array()
                    .map(|roles| roles
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", "))
                    .filter(|roles| !roles.is_empty())
                    .unwrap_or_else(|| "No Roles".into()),
                if entry["active"] == false {
                    " · soft-deleted"
                } else {
                    ""
                }
            )
        } else {
            name.into()
        };
        button(world, parent, owner, &title, Command::Select(id.into()));
    }
    if rows.is_empty() {
        label(world, parent, "No entries to show.", 14.0);
    }
    if rows.len() > 8 {
        let controls = row(world, parent);
        if page > 0 {
            button(world, controls, owner, "Previous", Command::Page(false));
        }
        label(
            world,
            controls,
            &format!("Page {} of {}", page + 1, rows.len().div_ceil(8)),
            14.0,
        );
        if (page + 1) * 8 < rows.len() {
            button(world, controls, owner, "Next", Command::Page(true));
        }
    }
    if tab == Tab::Roles && selected.is_some() {
        refresh_role(world, owner);
    } else if let Some(form) = world.get::<UserForm>(owner) {
        let chooser = form.chooser;
        role_choices(world, owner, chooser);
        refresh_standing(world, owner);
    }
}

pub(super) fn editor(world: &mut World, owner: Entity) {
    clear_password(world, owner);
    let view = world.get::<AccessControlSand>(owner).unwrap();
    let parent = view.editor;
    let tab = view.tab;
    let selected = view.selected.clone();
    world
        .entity_mut(owner)
        .remove::<(UserForm, StandingEditor, RoleForm, RoleEdit, PolicyForm)>();
    world.entity_mut(parent).despawn_children();
    if !world.resource::<Catalog>().ready {
        label(
            world,
            parent,
            "Waiting for users and Roles. Use Refresh to retry.",
            14.0,
        );
        world
            .get_mut::<AccessControlSand>(owner)
            .unwrap()
            .reload_editor = true;
        return;
    }
    world
        .get_mut::<AccessControlSand>(owner)
        .unwrap()
        .reload_editor = false;
    let entries = world.resource::<Catalog>().rows.clone();
    let kind = if tab == Tab::Users { "user" } else { "role" };
    let entry = selected.as_ref().and_then(|id| {
        entries
            .iter()
            .find(|entry| entry["kind"] == kind && entry["id"].as_str() == Some(id))
    });
    if selected.is_some() && entry.is_none() {
        label(
            world,
            parent,
            "This entry is no longer available. Refresh or select another.",
            14.0,
        );
        return;
    }
    if tab == Tab::Users {
        label(
            world,
            parent,
            if entry.is_some() {
                "Edit user"
            } else {
                "New user"
            },
            18.0,
        );
        let username = input(
            world,
            parent,
            "Username",
            entry
                .and_then(|e| e["username"].as_str())
                .unwrap_or_default(),
            256,
        );
        let name = input(
            world,
            parent,
            "Name",
            entry.and_then(|e| e["name"].as_str()).unwrap_or_default(),
            500,
        );
        let password = input(
            world,
            parent,
            if entry.is_some() {
                "New password · leave blank to keep it"
            } else {
                "Password"
            },
            "",
            1024,
        );
        world
            .entity_mut(password)
            .remove::<crate::token_style::TextToken>()
            .insert(TextColor(Color::NONE));
        world
            .get_mut::<bevy::a11y::AccessibilityNode>(password)
            .unwrap()
            .set_role(accesskit::Role::PasswordInput);
        let font = world.resource::<crate::theme::Typography>().text(18.0);
        world.spawn((
            Text::new(""),
            font,
            crate::token_style::text(Token::Ink),
            PasswordMask(password),
            Pickable::IGNORE,
            Node {
                position_type: PositionType::Absolute,
                left: px(4),
                top: px(4),
                ..default()
            },
            ChildOf(password),
        ));
        label(
            world,
            parent,
            "Assigned Roles — select to add or remove",
            14.0,
        );
        let role = entry
            .and_then(|entry| entry["role"].as_str())
            .unwrap_or_default()
            .to_owned();
        let roles: Vec<String> = entry
            .and_then(|entry| entry["roles"].as_array())
            .map(|roles| {
                roles
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let caption = if roles.is_empty() {
            "No Roles".into()
        } else {
            roles.join(", ")
        };
        let role_label = label(world, parent, &caption, 14.0);
        let chooser = column(world, parent);
        role_choices(world, owner, chooser);
        let controls = row(world, parent);
        let preview_record = input(
            world,
            parent,
            "Record UID for effective authority preview",
            "",
            128,
        );
        if entry.is_some() {
            button(
                world,
                parent,
                owner,
                "Preview this Actor's Record authority",
                Command::Preview,
            );
        }

        if entry.is_none_or(|entry| entry["has_credentials"] == true) {
            button(
                world,
                controls,
                owner,
                if entry.is_some() {
                    "Save user details"
                } else {
                    "Create user"
                },
                Command::SaveUser,
            );
        } else {
            label(
                world,
                parent,
                "This Actor signs in through an Organ grant. Manage its Roles and standing here; password fields do not apply.",
                14.0,
            );
        }
        if entry.is_some() {
            button(
                world,
                controls,
                owner,
                "Save assigned Roles",
                Command::Assign,
            );
        }
        if let Some(entry) = entry {
            for grant in entry["effective_permissions"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let roles = grant["roles"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ");
                label(
                    world,
                    parent,
                    &format!(
                        "{} · supplied by {}",
                        grant["permission"].as_str().unwrap_or(""),
                        roles
                    ),
                    13.0,
                );
            }
            let standing = column(world, parent);
            standing_editor(world, owner, standing, standing_state(entry));
        }
        world.entity_mut(owner).insert(UserForm {
            uid: selected,
            username,
            name,
            password,
            role,
            roles,
            expected_revision: entry
                .and_then(|entry| entry["access_revision"].as_i64())
                .unwrap_or(0),
            role_label,
            chooser,
            preview_record,
        });
    } else if let Some(entry) = entry {
        let name = entry["name"].as_str().unwrap_or_default();
        label(world, parent, "Edit Role", 18.0);
        if let (Some(role), Some(revision)) = (
            entry["id"].as_str().and_then(|id| id.parse::<i64>().ok()),
            entry["revision"].as_i64(),
        ) {
            let field = input(world, parent, "Role name", name, 100);
            let controls = row(world, parent);
            button(world, controls, owner, "Rename Role", Command::RenameRole);
            button(world, controls, owner, "Delete Role…", Command::DeleteRole);
            let confirm = column(world, parent);
            world.get_mut::<Node>(confirm).unwrap().display = Display::None;
            label(
                world,
                confirm,
                "Delete this Role? Reassign everyone using it first.",
                14.0,
            );
            let controls = row(world, confirm);
            button(
                world,
                controls,
                owner,
                "Delete Role",
                Command::ConfirmDeleteRole,
            );
            button(world, controls, owner, "Cancel", Command::CancelDeleteRole);
            let permissions = column(world, parent);
            world.entity_mut(owner).insert((
                RoleForm(field),
                RoleEdit {
                    role,
                    revision,
                    original_name: name.into(),
                    confirm,
                    permissions,
                },
            ));
            role_permissions(world, owner, permissions, entry);
            policy_editor(world, owner, parent, entry);
            return;
        }
        role_permissions(world, owner, parent, entry);
    } else {
        label(world, parent, "New Role", 18.0);
        let name = input(world, parent, "Role name", "", 100);
        world.entity_mut(owner).insert(RoleForm(name));
        button(world, parent, owner, "Create Role", Command::CreateRole);
        label(
            world,
            parent,
            "Create the Role, then select it to set its permissions.",
            14.0,
        );
    }
}

fn policy_editor(world: &mut World, owner: Entity, parent: Entity, entry: &Value) {
    label(world, parent, "Record policy", 18.0);
    label(
        world,
        parent,
        "Role grants add together. Each grant names its operation, selector, properties and assertion changes.",
        14.0,
    );
    let policy = entry
        .get("policy")
        .filter(|value| !value.is_null())
        .cloned()
        .unwrap_or_else(|| serde_json::json!({"read":{"all":[]},"grants":[]}));
    let raw = input(
        world,
        parent,
        "Policy",
        &serde_json::to_string_pretty(&policy).unwrap_or_default(),
        65536,
    );
    world.get_mut::<EditableText>(raw).unwrap().allow_newlines = true;
    world.get_mut::<EditableText>(raw).unwrap().visible_lines = Some(8.0);
    let controls = column(world, parent);
    world.entity_mut(owner).insert(PolicyForm {
        role: entry["name"].as_str().unwrap_or_default().into(),
        revision: entry["policy_revision"].as_i64().unwrap_or(0),
        raw,
        controls,
        signature: String::new(),
    });
    policy_controls(world, owner);
    button(
        world,
        parent,
        owner,
        "Save Record policy",
        Command::SavePolicy,
    );
}

fn open_selector(world: &mut World, owner: Entity, grant: Option<usize>) {
    let Some(form) = world.get::<PolicyForm>(owner) else {
        return;
    };
    let Ok(raw) = value(world, form.raw) else {
        return;
    };
    let Ok(policy) = serde_json::from_str::<protein::authority::RolePolicy>(&raw) else {
        status(world, owner, "Fix the policy before editing its selector.");
        return;
    };
    let selector = if let Some(index) = grant {
        let Some(grant) = policy.grants.get(index) else {
            return;
        };
        grant.selector.clone()
    } else {
        policy.read
    };
    let Some(root) = world.get::<ChildOf>(owner).map(ChildOf::parent) else {
        return;
    };
    let workspace = world
        .get::<crate::workspace::WorkspaceMember>(owner)
        .map_or(1, |member| member.0);
    let position = world
        .get::<crate::canvas::CanvasItem>(owner)
        .map_or(bevy::math::DVec2::ZERO, |item| {
            item.position + bevy::math::DVec2::new(80.0, 40.0)
        });
    let query = protein::Protein {
        source: protein::Source::Record,
        filter: vec![selector],
        fields: None,
        include: Default::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    };
    let castle = crate::protein_castle::spawn(
        world,
        root,
        workspace,
        position,
        crate::protein_castle::ProteinDraft::from_protein(
            "Authority selector".into(),
            String::new(),
            query,
        ),
    );
    let control = world
        .spawn((
            Button,
            Node::default(),
            ChildOf(castle),
            ActionButton::new(
                castle,
                crate::actions![AcceptSelector {
                    target: owner,
                    grant
                }],
            ),
        ))
        .id();
    label(world, control, "Use this selector in the policy", 16.0);
}

fn edit_grant(world: &mut World, owner: Entity, command: &Command) {
    use protein::authority::{MutationGrant, Operation, Property};
    let Some(form) = world.get::<PolicyForm>(owner) else {
        return;
    };
    let field = form.raw;
    let Ok(raw) = value(world, field) else {
        return;
    };
    let Ok(mut policy) = serde_json::from_str::<protein::authority::RolePolicy>(&raw) else {
        status(
            world,
            owner,
            "Fix the policy draft before changing its grants.",
        );
        return;
    };
    match command {
        Command::AddGrant(operation) if policy.grants.len() < 128 => {
            let properties = match operation {
                Operation::Create | Operation::Restore => std::collections::BTreeSet::from([
                    Property::Kind,
                    Property::Organ,
                    Property::Head,
                    Property::Body,
                    Property::Quantity,
                ]),
                Operation::Update => std::collections::BTreeSet::from([
                    Property::Head,
                    Property::Body,
                    Property::Quantity,
                ]),
                Operation::Delete => std::collections::BTreeSet::from([Property::Slug]),
            };
            policy.grants.push(MutationGrant {
                operation: *operation,
                selector: protein::Predicate::KindEq("plain".into()),
                properties,
                assertions_add: vec![],
                assertions_remove: vec![],
            });
        }
        Command::RemoveGrant(index) if *index < policy.grants.len() => {
            policy.grants.remove(*index);
        }
        Command::Property(index, property) => {
            let Some(grant) = policy.grants.get_mut(*index) else {
                return;
            };
            if !grant.properties.remove(property) {
                grant.properties.insert(property.clone());
            }
        }
        Command::Assertion(index, remove, predicate, target) => {
            let Some(grant) = policy.grants.get_mut(*index) else {
                return;
            };
            let predicate_uid = value(world, *predicate)
                .unwrap_or_default()
                .trim()
                .to_owned();
            let target = value(world, *target).unwrap_or_default().trim().to_owned();
            if !nucleus::valid_uid(&predicate_uid, "c")
                || (!target.is_empty() && target != "*" && !nucleus::valid_uid(&target, "r"))
            {
                status(
                    world,
                    owner,
                    "Use a Concept UID and an optional Record UID, or * for any readable Record.",
                );
                return;
            }
            let rule = protein::authority::AssertionGrant {
                predicate_uid,
                target: if target.is_empty() {
                    protein::authority::AssertionTarget::Unary
                } else if target == "*" {
                    protein::authority::AssertionTarget::AnyReadableRecord
                } else {
                    protein::authority::AssertionTarget::Record(target)
                },
                role: protein::authority::AssertionRole::Ordinary,
                properties: Default::default(),
            };
            let rules = if *remove {
                &mut grant.assertions_remove
            } else {
                &mut grant.assertions_add
            };
            if rules.len() >= 128 {
                status(world, owner, "Keep each grant within 128 assertion rules.");
                return;
            }
            rules.push(rule);
        }
        Command::RemoveAssertion(index, remove, rule) => {
            let Some(grant) = policy.grants.get_mut(*index) else {
                return;
            };
            let rules = if *remove {
                &mut grant.assertions_remove
            } else {
                &mut grant.assertions_add
            };
            if *rule >= rules.len() {
                return;
            }
            rules.remove(*rule);
        }
        Command::AssertionProperty(index, remove, rule, property) => {
            let Some(grant) = policy.grants.get_mut(*index) else {
                return;
            };
            let rules = if *remove {
                &mut grant.assertions_remove
            } else {
                &mut grant.assertions_add
            };
            let Some(rule) = rules.get_mut(*rule) else {
                return;
            };
            if !rule.properties.remove(property) {
                rule.properties.insert(*property);
            }
        }
        _ => return,
    }
    let mut field_value = crate::sand::editable(&serde_json::to_string_pretty(&policy).unwrap());
    field_value.visible_lines = Some(8.0);
    field_value.max_characters = Some(65536);
    *world.get_mut::<EditableText>(field).unwrap() = field_value;
    policy_controls(world, owner);
    status(
        world,
        owner,
        "Policy draft updated. Save it to apply these grants.",
    );
}

pub(super) fn policy_controls(world: &mut World, owner: Entity) {
    use protein::authority::{Operation, Property};
    let Some(form) = world.get::<PolicyForm>(owner) else {
        return;
    };
    let parent = form.controls;
    let field = form.raw;
    let Ok(raw) = value(world, form.raw) else {
        return;
    };
    if raw == form.signature {
        return;
    }
    let Ok(policy) = serde_json::from_str::<protein::authority::RolePolicy>(&raw) else {
        return;
    };
    world.get_mut::<PolicyForm>(owner).unwrap().signature = raw;
    crate::sand_panel::clear(world, parent);
    button(
        world,
        parent,
        owner,
        "Edit read selector with Protein",
        Command::Selector(None),
    );
    let add = row(world, parent);
    for (name, operation) in [
        ("Add creation grant", Operation::Create),
        ("Add update grant", Operation::Update),
        ("Add deletion grant", Operation::Delete),
        ("Add restoration grant", Operation::Restore),
    ] {
        button(world, add, owner, name, Command::AddGrant(operation));
    }
    for (index, grant) in policy.grants.iter().enumerate() {
        let controls = column(world, parent);
        label(
            world,
            controls,
            &format!("Grant {} · {:?}", index + 1, grant.operation),
            16.0,
        );
        button(
            world,
            controls,
            owner,
            "Edit selector with Protein",
            Command::Selector(Some(index)),
        );
        {
            let fields = row(world, controls);
            for (name, property) in [
                ("Kind", Property::Kind),
                ("Owner Organ", Property::Organ),
                ("Head", Property::Head),
                ("Body", Property::Body),
                ("Quantity", Property::Quantity),
                ("Slug", Property::Slug),
                ("Unit", Property::Unit),
                ("Place", Property::Place),
            ] {
                button(
                    world,
                    fields,
                    owner,
                    &format!(
                        "{} {}",
                        if grant.properties.contains(&property) {
                            "✓"
                        } else {
                            "+"
                        },
                        name
                    ),
                    Command::Property(index, property),
                );
            }
        }
        let predicate = input(world, controls, "Assertion Concept UID", "", 64);
        let target = input(
            world,
            controls,
            "Assertion target (empty for tag, Record UID, or *)",
            "",
            64,
        );
        let assertion_controls = row(world, controls);
        button(
            world,
            assertion_controls,
            owner,
            "Allow adding this assertion",
            Command::Assertion(index, false, predicate, target),
        );
        button(
            world,
            assertion_controls,
            owner,
            "Allow removing this assertion",
            Command::Assertion(index, true, predicate, target),
        );
        for (remove, rules) in [
            (false, &grant.assertions_add),
            (true, &grant.assertions_remove),
        ] {
            for (rule_index, rule) in rules.iter().enumerate() {
                let entry = row(world, controls);
                label(
                    world,
                    entry,
                    &format!(
                        "{} {} · {:?} · {:?}",
                        if remove { "Remove" } else { "Add" },
                        rule.predicate_uid,
                        rule.target,
                        rule.role
                    ),
                    14.0,
                );
                for (name, property) in [
                    (
                        "Assertion quantity",
                        protein::authority::AssertionProperty::Quantity,
                    ),
                    (
                        "Assertion unit",
                        protein::authority::AssertionProperty::Unit,
                    ),
                ] {
                    button(
                        world,
                        entry,
                        owner,
                        &format!(
                            "{} {name}",
                            if rule.properties.contains(&property) {
                                "✓"
                            } else {
                                "+"
                            }
                        ),
                        Command::AssertionProperty(index, remove, rule_index, property),
                    );
                }
                button(
                    world,
                    entry,
                    owner,
                    "Remove rule",
                    Command::RemoveAssertion(index, remove, rule_index),
                );
            }
        }
        button(
            world,
            controls,
            owner,
            "Remove this grant",
            Command::RemoveGrant(index),
        );
    }
    crate::workspace_sync::policy::mount(world, parent, owner, field, false);
}

fn standing_state(entry: &Value) -> Value {
    serde_json::json!({
        "active": entry["active"] != false,
        "at": entry["deactivated_at"],
        "note": entry["standing_note"],
    })
}

fn standing_editor(world: &mut World, owner: Entity, parent: Entity, state: Value) {
    world.entity_mut(parent).despawn_children();
    if state["active"] == false {
        label(world, parent, "Soft-deleted · access disabled", 14.0);
        if let Some(at) = state["at"].as_str() {
            label(world, parent, &format!("Soft-deleted on {at}"), 14.0);
        }
        if let Some(note) = state["note"].as_str().filter(|note| !note.is_empty()) {
            label(world, parent, &format!("Note: {note}"), 14.0);
        }
        button(world, parent, owner, "Restore user", Command::Restore);
    } else {
        label(world, parent, "Active", 14.0);
        button(world, parent, owner, "Soft-delete user…", Command::Delete);
    }
    let confirm = column(world, parent);
    world.get_mut::<Node>(confirm).unwrap().display = Display::None;
    label(
        world,
        confirm,
        "Soft-delete this user? Access stops. Their login, Role, Person and history stay, and you can restore access here.",
        14.0,
    );
    let note = input(world, confirm, "Optional note", "", 1024);
    let controls = row(world, confirm);
    button(
        world,
        controls,
        owner,
        "Soft-delete user",
        Command::ConfirmDelete,
    );
    button(world, controls, owner, "Cancel", Command::CancelDelete);
    world.entity_mut(owner).insert(StandingEditor {
        parent,
        state,
        note,
        confirm,
    });
}

fn refresh_standing(world: &mut World, owner: Entity) {
    let Some(person) = world
        .get::<UserForm>(owner)
        .and_then(|form| form.uid.as_ref())
    else {
        return;
    };
    let Some(entry) = world
        .resource::<Catalog>()
        .rows
        .iter()
        .find(|entry| entry["kind"] == "user" && entry["id"].as_str() == Some(person))
    else {
        return;
    };
    let state = standing_state(entry);
    if let Some(form) = world.get::<StandingEditor>(owner)
        && form.state != state
    {
        let parent = form.parent;
        standing_editor(world, owner, parent, state);
    }
}

fn refresh_role(world: &mut World, owner: Entity) {
    let selected = world
        .get::<AccessControlSand>(owner)
        .unwrap()
        .selected
        .clone();
    let entry = world
        .resource::<Catalog>()
        .rows
        .iter()
        .find(|entry| entry["kind"] == "role" && entry["id"].as_str() == selected.as_deref())
        .cloned();
    let Some(entry) = entry else {
        editor(world, owner);
        return;
    };
    let Some(edit) = world.get::<RoleEdit>(owner) else {
        editor(world, owner);
        return;
    };
    let permissions = edit.permissions;
    let field = world.get::<RoleForm>(owner).unwrap().0;
    let pristine = value(world, field).is_ok_and(|value| value == edit.original_name)
        && world
            .get::<Node>(edit.confirm)
            .is_some_and(|node| node.display == Display::None);
    if pristine
        && let (Some(name), Some(revision)) = (entry["name"].as_str(), entry["revision"].as_i64())
    {
        world
            .get_mut::<EditableText>(field)
            .unwrap()
            .editor
            .set_text(name);
        let mut edit = world.get_mut::<RoleEdit>(owner).unwrap();
        edit.original_name = name.into();
        edit.revision = revision;
    }
    world.entity_mut(permissions).despawn_children();
    role_permissions(world, owner, permissions, &entry);
}

fn role_permissions(world: &mut World, owner: Entity, parent: Entity, entry: &Value) {
    label(
        world,
        parent,
        "Permissions · changes save immediately for everyone with this Role",
        14.0,
    );
    label(world, parent, "Managing Record visibility and Concepts requires permission:assign. Workspace editing grants no Record authority. Manual and resumed Rules execute as the person invoking them.", 12.0);
    let keys: Vec<String> = world
        .resource::<Catalog>()
        .rows
        .iter()
        .filter(|entry| entry["kind"] == "permission_catalog")
        .filter_map(|entry| entry["keys"].as_array())
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let controls = row(world, parent);
    for key in &keys {
        let granted = entry["permissions"].as_array().is_some_and(|permissions| {
            permissions
                .iter()
                .any(|permission| permission.as_str() == Some(key))
        });
        let title = format!("{} {key}", if granted { "On" } else { "Off" });
        button(
            world,
            controls,
            owner,
            &title,
            Command::Permission(key.clone(), !granted),
        );
    }
    if keys.is_empty() {
        label(world, parent, "Permission catalog unavailable.", 14.0);
    }
}

fn role_choices(world: &mut World, owner: Entity, parent: Entity) {
    let choices = world
        .resource::<Catalog>()
        .rows
        .iter()
        .filter(|entry| entry["kind"] == "role")
        .filter_map(|entry| entry["name"].as_str())
        .map(|name| {
            (
                name.into(),
                crate::actions![Command::ChooseRole(name.into())],
            )
        })
        .collect();
    world.entity_mut(parent).despawn_children();
    crate::dropdown::spawn(
        world,
        parent,
        owner,
        "Choose one Role",
        "Choose Role…",
        choices,
    );
}

pub(super) fn clear_password(world: &mut World, owner: Entity) {
    if let Some(entity) = world.get::<UserForm>(owner).map(|form| form.password)
        && let Some(mut text) = world.get_mut::<EditableText>(entity)
    {
        *text = crate::sand::editable("");
        text.allow_newlines = false;
        text.visible_lines = Some(1.0);
        text.max_characters = Some(1024);
    }
}

pub(super) fn masks(world: &mut World) {
    let changes: Vec<_> = world
        .query::<(Entity, &PasswordMask)>()
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
    for (entity, count) in changes {
        if let Some(mut text) = world.get_mut::<Text>(entity) {
            text.set_if_neq(Text::new("•".repeat(count.min(32))));
        }
    }
}

pub(super) fn protect_passwords(forms: Query<&UserForm>, mut fields: Query<&mut EditableText>) {
    for form in &forms {
        if let Ok(mut field) = fields.get_mut(form.password)
            && field
                .pending_edits
                .iter()
                .any(|edit| matches!(edit, bevy::text::TextEdit::Copy | bevy::text::TextEdit::Cut))
        {
            field.pending_edits.retain(|edit| {
                !matches!(edit, bevy::text::TextEdit::Copy | bevy::text::TextEdit::Cut)
            });
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
