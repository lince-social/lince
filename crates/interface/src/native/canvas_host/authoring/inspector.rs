use super::*;
use crate::native::actions::Action;

#[derive(Component)]
struct Inspector {
    root: Entity,
    placement: String,
    revision: u64,
    part: String,
    editor: Entity,
    status: Entity,
    name: Entity,
    receipt: Option<String>,
    offset: usize,
}

#[derive(Clone)]
pub(crate) struct Open(pub Entity);

impl Action for Open {
    fn apply(&self, world: &mut World, root: Entity) {
        if let Err(error) = open(world, root, self.0) {
            crate::native::notifications::report(world, "interface::canvas-authoring", &error);
        }
    }
}

fn open(world: &mut World, root: Entity, selected: Entity) -> Result<(), String> {
    let mut host = selected;
    while world.get::<CanvasItem>(host).is_none()
        || world
            .get::<ChildOf>(host)
            .is_none_or(|parent| parent.parent() != root)
    {
        host = world
            .get::<ChildOf>(host)
            .ok_or("Select a canvas item to inspect.")?
            .parent();
        if host == root {
            return Err("Select a canvas item to inspect.".into());
        }
    }
    host = super::owner(world, host);
    capture(world, root)?;
    let placement = world.get::<Identity>(host).unwrap().0.clone();
    let snapshot = capture(world, root)?;
    let item = snapshot
        .placements
        .iter()
        .find(|item| item.id == placement)
        .unwrap()
        .clone();
    let old: Vec<_> = world
        .query::<(Entity, &Inspector)>()
        .iter(world)
        .filter(|(_, inspector)| inspector.root == root)
        .map(|(entity, _)| entity)
        .collect();
    for entity in old {
        world.despawn(entity);
    }
    let panel = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: px(16),
                top: px(16),
                width: px(720),
                max_width: percent(90),
                height: percent(90),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                padding: UiRect::all(px(12)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            crate::native::token_style::background(Token::Surface),
            crate::native::token_style::border(Token::Accent),
            crate::native::inspection::InspectionExcluded,
            ScrollPosition::default(),
            GlobalZIndex(50),
            ChildOf(root),
        ))
        .id();
    crate::native::scroll_sand::attach(world, panel);
    crate::native::edit_mode::label(world, panel, "Native component", 20.0);
    let name = crate::native::sand_panel::field(world, panel, "Component name", "My component");
    let controls = crate::native::sand_panel::row(world, panel);
    for (caption, command) in [
        ("Refresh", Command::Refresh),
        ("Apply changes", Command::Apply),
        ("Reset part", Command::Reset),
        ("Remove part", Command::Remove),
        ("Duplicate", Command::Duplicate),
        ("Undo", Command::Undo),
        ("Save component", Command::Save),
        ("Close", Command::Close),
    ] {
        crate::native::sand_panel::button(world, controls, panel, caption, command);
    }
    let status = crate::native::edit_mode::label(
        world,
        panel,
        "Select a part. Edits apply to this instance; saving creates a library component.",
        13.0,
    );
    crate::native::accessibility::status(world, status);
    let editor = crate::native::sand_text::spawn(
        world,
        panel,
        crate::native::sand_text::SavedText {
            area: crate::native::sand_text::SandText::new(true),
            text: "[]".into(),
        },
    );
    world.entity_mut(editor).insert(Node {
        width: percent(100),
        min_height: px(180),
        max_height: px(320),
        flex_shrink: 0.0,
        overflow: Overflow::scroll_y(),
        ..default()
    });
    world
        .get_mut::<bevy::text::EditableText>(editor)
        .unwrap()
        .max_characters = Some(65536);
    world.entity_mut(panel).insert(Inspector {
        root,
        placement: item.id,
        revision: snapshot.revision,
        part: "root".into(),
        editor,
        status,
        name,
        receipt: None,
        offset: 0,
    });
    render(world, panel)?;
    Ok(())
}

#[derive(Component)]
struct Tree;

#[derive(Component)]
struct Properties;

fn render(world: &mut World, panel: Entity) -> Result<(), String> {
    let inspector = world
        .get::<Inspector>(panel)
        .ok_or("The inspector is closed.")?;
    let (root, placement, offset) = (
        inspector.root,
        inspector.placement.clone(),
        inspector.offset,
    );
    let response = super::super::handle(
        world,
        root,
        &api::Request::InspectElement {
            placement,
            offset,
            limit: 128,
        },
    )?;
    let api::Response::Element { element } = response else {
        return Err("Missing native inspection.".into());
    };
    world.get_mut::<Inspector>(panel).unwrap().revision = element.revision;
    let children: Vec<_> = world
        .get::<Children>(panel)
        .into_iter()
        .flatten()
        .filter(|entity| world.get::<Tree>(**entity).is_some())
        .copied()
        .collect();
    for child in children {
        world.despawn(child);
    }
    let tree = crate::native::sand_panel::column(world, panel);
    world.entity_mut(tree).insert(Tree);
    crate::native::edit_mode::label(
        world,
        tree,
        &format!(
            "{} parts · revision {}",
            element.total_parts, element.revision
        ),
        14.0,
    );
    for part in &element.parts {
        crate::native::sand_panel::button(
            world,
            tree,
            panel,
            &format!(
                "{} · {}{}",
                part.id,
                part.name,
                if part.removed { " (removed)" } else { "" }
            ),
            Select(part.id.clone()),
        );
    }
    let pages = crate::native::sand_panel::row(world, tree);
    if offset > 0 {
        crate::native::sand_panel::button(world, pages, panel, "First parts", Page(0));
    }
    if let Some(offset) = element.next_offset {
        crate::native::sand_panel::button(world, pages, panel, "Next parts", Page(offset));
    }
    Select(world.get::<Inspector>(panel).unwrap().part.clone()).apply(world, panel);
    Ok(())
}

#[derive(Clone)]
struct Page(usize);
impl Action for Page {
    fn apply(&self, world: &mut World, panel: Entity) {
        if let Some(mut inspector) = world.get_mut::<Inspector>(panel) {
            inspector.offset = self.0;
        }
        if let Err(error) = render(world, panel) {
            crate::native::notifications::report(world, "interface::canvas-authoring", &error);
        }
    }
}

#[derive(Clone)]
struct Select(String);
impl Action for Select {
    fn apply(&self, world: &mut World, panel: Entity) {
        let Some(inspector) = world.get::<Inspector>(panel) else {
            return;
        };
        let (root, placement, editor, status) = (
            inspector.root,
            inspector.placement.clone(),
            inspector.editor,
            inspector.status,
        );
        let result = (|| {
            let snapshot = capture(world, root)?;
            let item = snapshot
                .placements
                .iter()
                .find(|item| item.id == placement)
                .ok_or("The placement was removed.")?;
            let edits = api::authoring::edits(&item.component);
            let edit = edits.parts.get(&self.0).cloned().unwrap_or_default();
            let changes = vec![api::authoring::Change::SetPart {
                part: self.0.clone(),
                edit,
            }];
            let body = serde_json::to_string_pretty(&changes).map_err(|error| error.to_string())?;
            world
                .get_mut::<bevy::text::EditableText>(editor)
                .ok_or("The editor is closed.")?
                .editor
                .set_text(&body);
            world.get_mut::<Inspector>(panel).unwrap().part = self.0.clone();
            let host = world
                .get::<Children>(root)
                .into_iter()
                .flatten()
                .find(|entity| {
                    world
                        .get::<Identity>(**entity)
                        .is_some_and(|id| id.0 == placement)
                })
                .copied()
                .ok_or("The placement was removed.")?;
            let editing = index(world, host)?;
            let entity = entities(&editing)
                .get(&self.0)
                .copied()
                .ok_or("This part is no longer available.")?;
            let tokens: Vec<_> = uses(world, entity)
                .into_iter()
                .map(|(usage, token)| {
                    let (value, source) = crate::native::token_style::resolve(world, entity, token);
                    format!(
                        "{usage}: {} = {} ({source})",
                        token_name(token),
                        value.display()
                    )
                })
                .collect();
            Ok::<_, String>(format!(
                "{} · {}. {}",
                self.0,
                role(world, entity),
                tokens.join("; ")
            ))
        })();
        crate::native::sand_panel::status(world, status, &result.unwrap_or_else(|error| error));
        properties(world, panel);
    }
}

fn properties(world: &mut World, panel: Entity) {
    let old: Vec<_> = world
        .get::<Children>(panel)
        .into_iter()
        .flatten()
        .filter(|entity| world.get::<Properties>(**entity).is_some())
        .copied()
        .collect();
    for entity in old {
        world.despawn(entity);
    }
    let properties = crate::native::sand_panel::column(world, panel);
    world.entity_mut(properties).insert(Properties);
    crate::native::edit_mode::label(world, properties, "Part properties", 16.0);
    let flow = crate::native::sand_panel::row(world, properties);
    for (caption, key, value) in [
        ("Row", "direction", "row"),
        ("Column", "direction", "column"),
        ("Wrap", "wrap", "wrap"),
        ("No wrap", "wrap", "no-wrap"),
        ("Show", "display", "flex"),
        ("Hide", "display", "none"),
    ] {
        crate::native::sand_panel::button(
            world,
            flow,
            panel,
            caption,
            Property::Choice(key.into(), value.into()),
        );
    }
    let row = crate::native::sand_panel::row(world, properties);
    let token = crate::native::sand_panel::field(world, row, "Token", "FontSize");
    let value = crate::native::sand_panel::field(world, row, "Value", "16");
    crate::native::sand_panel::button(
        world,
        row,
        panel,
        "Set token",
        Property::Token(token, value),
    );
    crate::native::sand_panel::button(world, row, panel, "Inherit token", Property::Inherit(token));
    let size = crate::native::sand_panel::row(world, properties);
    let width = crate::native::sand_panel::field(world, size, "Width (px or auto)", "auto");
    let height = crate::native::sand_panel::field(world, size, "Height (px or auto)", "auto");
    crate::native::sand_panel::button(
        world,
        size,
        panel,
        "Set dimensions",
        Property::Dimensions(width, height),
    );
    crate::native::sand_panel::button(
        world,
        properties,
        panel,
        "Add flow group inside this part",
        Property::Container,
    );
}

#[derive(Clone)]
enum Property {
    Choice(String, String),
    Token(Entity, Entity),
    Inherit(Entity),
    Dimensions(Entity, Entity),
    Container,
}

impl Action for Property {
    fn apply(&self, world: &mut World, panel: Entity) {
        let Some(inspector) = world.get::<Inspector>(panel) else {
            return;
        };
        let (root, placement, part, revision, status) = (
            inspector.root,
            inspector.placement.clone(),
            inspector.part.clone(),
            inspector.revision,
            inspector.status,
        );
        let result = (|| {
            let snapshot = capture(world, root)?;
            let item = snapshot
                .placements
                .iter()
                .find(|item| item.id == placement)
                .ok_or("The placement is no longer available.")?;
            let mut edit = api::authoring::edits(&item.component)
                .parts
                .get(&part)
                .cloned()
                .unwrap_or_default();
            let mut changes = Vec::new();
            match self {
                Self::Choice(key, value) => {
                    edit.style.choices.insert(key.clone(), value.clone());
                }
                Self::Token(name, input) => {
                    let name = crate::native::sand_panel::value(world, *name)?;
                    let token = token(&name)?;
                    let parsed = token
                        .parse(&crate::native::sand_panel::value(world, *input)?)
                        .ok_or(
                            "Enter a value within the token's native range or a hexadecimal color.",
                        )?;
                    edit.tokens.insert(name, value(parsed));
                }
                Self::Inherit(name) => {
                    let name = crate::native::sand_panel::value(world, *name)?;
                    token(&name)?;
                    edit.tokens.remove(&name);
                }
                Self::Dimensions(width, height) => {
                    for (key, field) in [("width", width), ("height", height)] {
                        let text = crate::native::sand_panel::value(world, *field)?;
                        let value = if text.trim() == "auto" {
                            api::authoring::Measure::Auto
                        } else {
                            api::authoring::Measure::Px(
                                text.trim()
                                    .parse()
                                    .map_err(|_| "Use a number of pixels or auto.")?,
                            )
                        };
                        edit.style.dimensions.insert(key.into(), value);
                    }
                }
                Self::Container => {
                    changes.push(api::authoring::Change::InsertPart {
                        part: format!("user/{}", nucleus::new_uid("part")),
                        definition: AddedPart {
                            parent: part.clone(),
                            component: None,
                            size: [240.0, 48.0],
                        },
                    });
                }
            }
            if changes.is_empty() {
                changes.push(api::authoring::Change::SetPart { part, edit });
            }
            let request = api::Request::Mutate {
                request_id: nucleus::new_uid("request"),
                expected_revision: revision,
                mutation: api::Mutation::Patch { placement, changes },
            };
            let api::Response::Receipt { receipt } = super::super::handle(world, root, &request)?
            else {
                return Err("Missing authored change receipt.".into());
            };
            world.get_mut::<Inspector>(panel).unwrap().receipt = Some(receipt.request_id);
            render(world, panel)?;
            Ok::<_, String>(format!("Applied revision {}", receipt.revision))
        })();
        crate::native::sand_panel::status(world, status, &result.unwrap_or_else(|error| error));
    }
}

#[derive(Clone)]
enum Command {
    Refresh,
    Apply,
    Reset,
    Remove,
    Duplicate,
    Undo,
    Save,
    Close,
}
impl Action for Command {
    fn apply(&self, world: &mut World, panel: Entity) {
        if matches!(self, Self::Close) {
            world.despawn(panel);
            return;
        }
        let Some(inspector) = world.get::<Inspector>(panel) else {
            return;
        };
        let (root, placement, part, revision, editor, status, name, receipt) = (
            inspector.root,
            inspector.placement.clone(),
            inspector.part.clone(),
            inspector.revision,
            inspector.editor,
            inspector.status,
            inspector.name,
            inspector.receipt.clone(),
        );
        let result = (|| {
            if matches!(self, Self::Refresh) {
                render(world, panel)?;
                return Ok("Refreshed.".into());
            }
            if matches!(self, Self::Save) {
                let name = crate::native::sand_panel::value(world, name)?;
                let response = super::super::handle(
                    world,
                    root,
                    &api::Request::Export {
                        placement,
                        name: name.clone(),
                    },
                )?;
                let api::Response::Export { document, hash, .. } = response else {
                    return Err("Missing component export.".into());
                };
                crate::native::sand_panel::send(
                    world,
                    panel,
                    cell::ClientMessage::Act {
                        id: nucleus::new_uid("canvas-component-save"),
                        action: engine::actions::Action::CreateCustomComponent {
                            head: name,
                            body: serde_json::to_string(&document)
                                .map_err(|error| error.to_string())?,
                        },
                    },
                )?;
                return Ok(format!("Saving component · {hash}"));
            }
            let mutation = match self {
                Self::Apply => api::Mutation::Patch {
                    placement,
                    changes: serde_json::from_str(&crate::native::sand_panel::value(
                        world, editor,
                    )?)
                    .map_err(|error| error.to_string())?,
                },
                Self::Reset => api::Mutation::Patch {
                    placement,
                    changes: vec![api::authoring::Change::ResetPart { part }],
                },
                Self::Remove => api::Mutation::Patch {
                    placement,
                    changes: vec![api::authoring::Change::RemovePart {
                        part,
                        cascade: false,
                    }],
                },
                Self::Duplicate => api::Mutation::Duplicate {
                    placement,
                    copy: nucleus::new_uid("placement"),
                    workspace: world.get::<Workspaces>(root).unwrap().active,
                    position: world
                        .get::<CanvasView>(root)
                        .copied()
                        .unwrap_or_default()
                        .center
                        .to_array(),
                },
                Self::Undo => api::Mutation::Undo {
                    receipt: receipt.ok_or("No retained inspector operation to undo.")?,
                },
                _ => unreachable!(),
            };
            let response = super::super::handle(
                world,
                root,
                &api::Request::Mutate {
                    request_id: nucleus::new_uid("request"),
                    expected_revision: revision,
                    mutation,
                },
            )?;
            let api::Response::Receipt { receipt } = response else {
                return Err("Missing change receipt.".into());
            };
            world.get_mut::<Inspector>(panel).unwrap().receipt = Some(receipt.request_id);
            render(world, panel)?;
            Ok::<_, String>(format!(
                "Applied revision {} · {:?}",
                receipt.revision, receipt.persistence
            ))
        })();
        crate::native::sand_panel::status(world, status, &result.unwrap_or_else(|error| error));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click(world: &mut World, panel: Entity, caption: &str) {
        let button = world
            .query::<(
                &crate::native::icons::Tooltip,
                &crate::native::actions::ActionButton,
            )>()
            .iter(world)
            .find(|(tooltip, button)| tooltip.0 == caption && button.target == panel)
            .unwrap()
            .1
            .clone();
        button.actions.run(world, button.target);
    }

    #[test]
    fn native_inspector_controls_edit_and_undo_without_replacing_the_target_editor() {
        let (mut app, root) = crate::native::canvas_host::tests::fixture();
        let host = crate::native::sand_store::spawn_sand(
            app.world_mut(),
            root,
            1,
            SandKind::EditableText,
            "Initial",
            DVec2::ZERO,
        );
        let editor = app
            .world()
            .get::<StoredSand>(host)
            .unwrap()
            .content
            .unwrap();
        app.world_mut()
            .get_mut::<bevy::text::EditableText>(editor)
            .unwrap()
            .editor
            .set_text("Keep this draft");
        app.world_mut()
            .resource_mut::<bevy::input_focus::InputFocus>()
            .set(editor, bevy::input_focus::FocusCause::Pressed);
        Open(host).apply(app.world_mut(), root);
        let panel = app
            .world_mut()
            .query_filtered::<Entity, With<Inspector>>()
            .single(app.world())
            .unwrap();
        click(app.world_mut(), panel, "Row");
        click(app.world_mut(), panel, "Wrap");
        assert_eq!(
            app.world().get::<Node>(host).unwrap().flex_direction,
            FlexDirection::Row
        );
        assert_eq!(
            app.world().get::<Node>(host).unwrap().flex_wrap,
            FlexWrap::Wrap
        );
        let field = app
            .world_mut()
            .query::<(Entity, &crate::native::icons::Tooltip)>()
            .iter(app.world())
            .find(|(_, tooltip)| tooltip.0 == "Value")
            .unwrap()
            .0;
        app.world_mut()
            .get_mut::<bevy::text::EditableText>(field)
            .unwrap()
            .editor
            .set_text("21");
        click(app.world_mut(), panel, "Set token");
        assert_eq!(
            crate::native::token_style::resolve(app.world(), host, Token::FontSize).0,
            TokenValue::Number(21.0)
        );
        click(app.world_mut(), panel, "Undo");
        assert_ne!(
            crate::native::token_style::resolve(app.world(), host, Token::FontSize).0,
            TokenValue::Number(21.0)
        );
        click(app.world_mut(), panel, "Duplicate");
        assert_eq!(capture(app.world_mut(), root).unwrap().placements.len(), 2);
        assert_eq!(
            app.world().get::<StoredSand>(host).unwrap().content,
            Some(editor)
        );
        assert_eq!(text(app.world(), editor).unwrap(), "Keep this draft");
        assert_eq!(
            app.world()
                .resource::<bevy::input_focus::InputFocus>()
                .get(),
            Some(editor)
        );
        click(app.world_mut(), panel, "Close");
        assert!(app.world().get_entity(panel).is_err());
        assert!(app.world().get_entity(editor).is_ok());
    }
}
