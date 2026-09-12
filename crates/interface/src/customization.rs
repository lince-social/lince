use crate::{
    actions::{Action, ActionButton},
    edit_mode::{EditMode, label},
    token_style,
    tokens::{
        ColorScheme, SandStyleKind, TOKENS, ThemeSettings, Token, TokenOverrides, TokenValue,
    },
    workspace::{WorkspaceMember, Workspaces},
};
use bevy::{a11y::AccessibilityNode, prelude::*, text::EditableText};

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    #[default]
    All,
    Kind(SandStyleKind),
    Sand(Entity),
    Workspace(Entity, u64),
}

#[derive(EntityEvent, Clone, Copy)]
pub struct GlobalCustomizationPanelToggle {
    pub entity: Entity,
}

#[derive(Clone, Copy, Debug)]
pub enum CustomizationAction {
    Scope(Scope),
    Scheme(ColorScheme),
    ClearOverrides,
    Reset(Token),
    ResetPattern,
}

impl Action for CustomizationAction {
    fn apply(&self, world: &mut World, root: Entity) {
        if !world.get::<EditMode>(root).is_some_and(|mode| mode.enabled) {
            return;
        }
        let scope = world.get::<Scope>(root).copied().unwrap_or_default();
        match *self {
            Self::Scope(scope) => {
                world.entity_mut(root).insert(scope);
            }
            Self::Scheme(scheme) => {
                world.resource_mut::<ThemeSettings>().scheme = scheme;
            }
            Self::ClearOverrides => {
                let scheme = world.resource::<ThemeSettings>().scheme;
                *world.resource_mut::<ThemeSettings>() = ThemeSettings {
                    scheme,
                    ..default()
                };
                let entities: Vec<_> = world.query_filtered::<Entity, Or<(With<TokenOverrides>, With<crate::sand_text::SandText>)>>().iter(world).collect();
                for entity in entities {
                    token_style::set_overrides(world, entity, TokenOverrides::default());
                }
                let mut spaces = world.query::<&mut Workspaces>();
                for mut spaces in spaces.iter_mut(world) {
                    for space in &mut spaces.entries {
                        space.color_overrides = [false; 2];
                    }
                    for record in spaces.saved_records.values_mut() {
                        record.tokens = TokenOverrides::default();
                    }
                }
            }
            Self::Reset(token) => {
                edit(world, scope, token, None);
            }
            Self::ResetPattern => {
                edit(world, Scope::All, Token::CanvasPattern, None);
            }
        }
        crate::edit_mode::render_panel(world, root);
        if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
            wake.ring();
        }
    }
}

#[derive(Component)]
struct TokenField {
    root: Entity,
    scope: Scope,
    token: Token,
    observed: String,
    status: Entity,
    preview: Entity,
}

#[derive(Component)]
struct OverrideSummary {
    root: Entity,
    scope: Scope,
    token: Token,
}

#[derive(Component)]
struct InvalidValue;

fn overwrites(world: &World, root: Entity, scope: Scope, token: Token) -> String {
    let settings = world.resource::<ThemeSettings>();
    let mut layers = vec![format!(
        "{} {}",
        settings.scheme.name(),
        token.default_value(settings.scheme).display()
    )];
    if let Some(value) = settings.global.0.get(&token) {
        layers.push(format!("All Sands {}", value.display()));
    }
    for (kind, overrides) in &settings.kinds {
        if let Some(value) = overrides.0.get(&token) {
            layers.push(format!("{} {}", kind.name(), value.display()));
        }
    }
    if matches!(token, Token::CanvasBackground | Token::CanvasGrid)
        && let Some(spaces) = world.get::<Workspaces>(root)
    {
        let index = usize::from(token == Token::CanvasGrid);
        for space in &spaces.entries {
            if space.color_overrides[index] {
                let [r, g, b] = if index == 0 {
                    space.colors.background
                } else {
                    space.colors.grid
                };
                layers.push(format!(
                    "{} {}",
                    space.name,
                    TokenValue::Color([r, g, b, 255]).display()
                ));
            }
        }
    }
    let active = world.get::<Workspaces>(root).map(|spaces| spaces.active);
    let mut sands = world
        .get::<Children>(root)
        .map(|children| {
            children
                .iter()
                .filter(|entity| {
                    world
                        .get::<WorkspaceMember>(*entity)
                        .is_some_and(|member| Some(member.0) == active)
                        && token_style::kind(world, *entity).is_some()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    sands.sort();
    for (index, entity) in sands.into_iter().enumerate() {
        if let Some(value) = token_style::overrides(world, entity).0.get(&token) {
            layers.push(format!(
                "{} {} {}",
                token_style::kind(world, entity).unwrap().name(),
                index + 1,
                value.display()
            ));
        }
        if let Some(children) = world.get::<Children>(entity) {
            for (block, child) in children
                .iter()
                .filter(|child| world.get::<crate::sand_text::SandText>(*child).is_some())
                .enumerate()
            {
                if let Some(value) = token_style::overrides(world, child).0.get(&token) {
                    layers.push(format!(
                        "Text {}.{} {}",
                        index + 1,
                        block + 1,
                        value.display()
                    ));
                }
            }
        }
    }
    if scope != Scope::All {
        let (current, source) = value(world, scope, token);
        if matches!(source, "Default" | "Parent Sand") {
            layers.push(format!("{source}: {}", current.display()));
        }
    }
    layers.join(" · ")
}

fn edit(world: &mut World, scope: Scope, token: Token, value: Option<TokenValue>) {
    if let Scope::Workspace(root, id) = scope {
        let index = match token {
            Token::CanvasBackground => 0,
            Token::CanvasGrid => 1,
            _ => return,
        };
        if value.is_some_and(|value| !token.accepts(value)) {
            return;
        }
        let Some(mut spaces) = world.get_mut::<Workspaces>(root) else {
            return;
        };
        let Some(space) = spaces.entries.iter_mut().find(|space| space.id == id) else {
            return;
        };
        space.color_overrides[index] = value.is_some();
        if let Some(TokenValue::Color([r, g, b, _])) = value {
            if index == 0 {
                space.colors.background = [r, g, b];
            } else {
                space.colors.grid = [r, g, b];
            }
        }
        return;
    }
    let mut values = match scope {
        Scope::Workspace(..) => unreachable!(),
        Scope::All => world.resource::<ThemeSettings>().global.clone(),
        Scope::Kind(kind) => world
            .resource::<ThemeSettings>()
            .kinds
            .get(&kind)
            .cloned()
            .unwrap_or_default(),
        Scope::Sand(entity) => {
            if token_style::kind(world, entity).is_none() {
                return;
            }
            token_style::overrides(world, entity)
        }
    };
    if let Some(value) = value {
        if !values.set(token, value) {
            return;
        }
    } else {
        values.0.remove(&token);
    }
    match scope {
        Scope::Workspace(..) => unreachable!(),
        Scope::All => world.resource_mut::<ThemeSettings>().global = values,
        Scope::Kind(kind) => {
            world
                .resource_mut::<ThemeSettings>()
                .kinds
                .insert(kind, values);
        }
        Scope::Sand(entity) => token_style::set_overrides(world, entity, values),
    }
}

fn value(world: &World, scope: Scope, token: Token) -> (TokenValue, &'static str) {
    match scope {
        Scope::Workspace(root, id) => {
            if let Some(space) = world
                .get::<Workspaces>(root)
                .and_then(|spaces| spaces.entries.iter().find(|space| space.id == id))
            {
                let index = usize::from(token == Token::CanvasGrid);
                if space.color_overrides[index] {
                    let [r, g, b] = if index == 0 {
                        space.colors.background
                    } else {
                        space.colors.grid
                    };
                    return (TokenValue::Color([r, g, b, 255]), "Workspace");
                }
            }
            value(world, Scope::All, token)
        }
        Scope::All => {
            world
                .resource::<ThemeSettings>()
                .resolve(token, None, &TokenOverrides::default())
        }
        Scope::Kind(kind) => {
            world
                .resource::<ThemeSettings>()
                .resolve(token, Some(kind), &TokenOverrides::default())
        }
        Scope::Sand(entity) => token_style::resolve(world, entity, token),
    }
}

fn control(
    world: &mut World,
    root: Entity,
    parent: Entity,
    action: CustomizationAction,
    title: &str,
) {
    let entity = world
        .spawn((
            crate::sand::button(0),
            ActionButton::new(root, crate::actions![action]),
            token_style::background(Token::Surface),
            token_style::border(Token::Accent),
            token_style::OutlineToken(Token::Accent),
            Outline {
                width: px(0),
                offset: px(2),
                color: crate::theme::PURPLE,
            },
            Node {
                padding: UiRect::axes(px(10), px(7)),
                border: UiRect::all(px(1)),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .observe(
            |event: On<bevy::input_focus::FocusGained>, mut outlines: Query<&mut Outline>| {
                if event.cause == bevy::input_focus::FocusCause::Navigated
                    && let Ok(mut outline) = outlines.get_mut(event.entity)
                {
                    outline.width = px(2);
                }
            },
        )
        .observe(
            |event: On<bevy::input_focus::FocusLost>, mut outlines: Query<&mut Outline>| {
                if let Ok(mut outline) = outlines.get_mut(event.entity) {
                    outline.width = px(0);
                }
            },
        )
        .id();
    world
        .get_mut::<AccessibilityNode>(entity)
        .unwrap()
        .set_label(title);
    label(world, entity, title, 14.0);
}

fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                column_gap: px(8),
                row_gap: px(8),
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

fn pattern_control(world: &mut World, root: Entity, panel: Entity) {
    let heading = row(world, panel);
    world.get_mut::<Node>(heading).unwrap().justify_content = JustifyContent::SpaceBetween;
    label(world, heading, "Pattern", 16.0);
    control(
        world,
        root,
        heading,
        CustomizationAction::ResetPattern,
        "Reset",
    );
    let status = label(
        world,
        panel,
        &overwrites(world, root, Scope::All, Token::CanvasPattern),
        12.0,
    );
    world.entity_mut(status).insert(OverrideSummary {
        root,
        scope: Scope::All,
        token: Token::CanvasPattern,
    });
    let value = world
        .resource::<ThemeSettings>()
        .resolve(Token::CanvasPattern, None, &TokenOverrides::default())
        .0
        .number();
    let slider = crate::slider::spawn(
        world,
        panel,
        "Pattern",
        crate::slider::SliderSand {
            start: 0.0,
            end: 100.0,
            step: 5.0,
            decimals: 0,
        },
        value,
        "%",
    )
    .unwrap();
    world.entity_mut(slider).observe(
        |event: On<crate::slider::SliderChanged>, mut settings: ResMut<ThemeSettings>| {
            settings
                .global
                .set(Token::CanvasPattern, TokenValue::Number(event.value));
        },
    );
}

pub(crate) fn render(world: &mut World, root: Entity, panel: Entity) {
    let mut scope = world.get::<Scope>(root).copied().unwrap_or_default();
    if let Scope::Sand(entity) = scope
        && token_style::kind(world, entity).is_none()
    {
        scope = Scope::All;
        world.entity_mut(root).insert(scope);
    }
    let heading = row(world, panel);
    {
        let mut node = world.get_mut::<Node>(heading).unwrap();
        node.width = percent(100);
        node.justify_content = JustifyContent::SpaceBetween;
    }
    label(world, heading, "Customization", 22.0);
    control(
        world,
        root,
        heading,
        CustomizationAction::ClearOverrides,
        "Reset",
    );
    let scheme = world.resource::<ThemeSettings>().scheme;
    crate::dropdown::spawn(
        world,
        panel,
        root,
        "Colorscheme",
        scheme.name(),
        ColorScheme::ALL
            .into_iter()
            .map(|scheme| {
                (
                    scheme.name().into(),
                    crate::actions![CustomizationAction::Scheme(scheme)],
                )
            })
            .collect(),
    );
    let mut choices = vec![(
        "All Sands".into(),
        crate::actions![CustomizationAction::Scope(Scope::All)],
    )];
    for kind in SandStyleKind::ALL {
        choices.push((
            kind.name().into(),
            crate::actions![CustomizationAction::Scope(Scope::Kind(kind))],
        ));
    }
    let active = world.get::<Workspaces>(root).unwrap().active;
    let mut sands: Vec<_> = world
        .query::<(Entity, &ChildOf, &WorkspaceMember)>()
        .iter(world)
        .filter(|(entity, parent, member)| {
            parent.parent() == root
                && member.0 == active
                && token_style::kind(world, *entity).is_some()
        })
        .map(|(entity, _, _)| entity)
        .collect();
    sands.sort();
    choices.push((
        "Workspace".into(),
        crate::actions![CustomizationAction::Scope(Scope::Workspace(root, active))],
    ));
    for (index, entity) in sands.iter().enumerate() {
        let kind = token_style::kind(world, *entity).unwrap();
        choices.push((
            format!("{} {}", kind.name(), index + 1),
            crate::actions![CustomizationAction::Scope(Scope::Sand(*entity))],
        ));
        let children: Vec<_> = world
            .get::<Children>(*entity)
            .into_iter()
            .flatten()
            .copied()
            .filter(|child| world.get::<crate::sand_text::SandText>(*child).is_some())
            .collect();
        for (block, child) in children.into_iter().enumerate() {
            choices.push((
                format!("Text {}.{}", index + 1, block + 1),
                crate::actions![CustomizationAction::Scope(Scope::Sand(child))],
            ));
        }
    }
    let scope_name = match scope {
        Scope::Workspace(..) => "Workspace".into(),
        Scope::All => "All Sands".to_string(),
        Scope::Kind(kind) => kind.name().into(),
        Scope::Sand(entity) => {
            if let Some(index) = sands.iter().position(|sand| *sand == entity) {
                format!(
                    "{} {}",
                    token_style::kind(world, entity).unwrap().name(),
                    index + 1
                )
            } else {
                "Selected text area".into()
            }
        }
    };
    crate::dropdown::spawn(world, panel, root, "Overrides", &scope_name, choices);
    if scope == Scope::All {
        pattern_control(world, root, panel);
    }
    for definition in TOKENS {
        let token = definition.token;
        if token == Token::CanvasPattern {
            continue;
        }
        if matches!(scope, Scope::Workspace(..))
            && !matches!(token, Token::CanvasBackground | Token::CanvasGrid)
        {
            continue;
        }
        if matches!(scope, Scope::Kind(_) | Scope::Sand(_))
            && !matches!(
                token,
                Token::SandBackground
                    | Token::SandBorder
                    | Token::SandInk
                    | Token::Accent
                    | Token::Width
                    | Token::Height
                    | Token::Roundness
                    | Token::BorderWidth
                    | Token::Spacing
                    | Token::Padding
                    | Token::FontSize
            )
        {
            continue;
        }
        if let Scope::Sand(entity) = scope
            && world.get::<crate::sand_text::SandText>(entity).is_some()
            && !matches!(token, Token::SandInk | Token::Accent | Token::FontSize)
        {
            continue;
        }
        let group = world
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    flex_shrink: 0.0,
                    ..default()
                },
                ChildOf(panel),
            ))
            .id();
        label(world, group, definition.name, 16.0);
        let (current, _) = value(world, scope, token);
        let status = label(world, group, &overwrites(world, root, scope, token), 12.0);
        world
            .entity_mut(status)
            .insert(OverrideSummary { root, scope, token });
        let fields = row(world, group);
        let bundle = crate::sand::text_editor(
            &current.display(),
            world.resource::<crate::theme::Typography>(),
            0,
        );
        let editor = world
            .spawn((bundle, AccessibilityNode::default(), ChildOf(fields)))
            .id();
        world.get_mut::<Node>(editor).unwrap().width = px(160);
        let mut text = world.get_mut::<EditableText>(editor).unwrap();
        text.allow_newlines = false;
        text.visible_lines = Some(1.0);
        text.max_characters = Some(24);
        world
            .get_mut::<AccessibilityNode>(editor)
            .unwrap()
            .set_label(definition.name);
        let preview = world
            .spawn((
                Node {
                    width: px(40),
                    height: px(32),
                    flex_shrink: 0.0,
                    ..default()
                },
                ChildOf(fields),
            ))
            .id();
        preview_value(world, preview, token, current);
        control(
            world,
            root,
            fields,
            CustomizationAction::Reset(token),
            "Reset",
        );
        world.entity_mut(editor).insert(TokenField {
            root,
            scope,
            token,
            observed: current.display(),
            status,
            preview,
        });
    }
}

fn preview_value(world: &mut World, entity: Entity, token: Token, value: TokenValue) {
    let mut node = world.get_mut::<Node>(entity).unwrap();
    match value {
        TokenValue::Color(_) => {}
        TokenValue::Number(value) => match token {
            Token::Roundness => node.border_radius = BorderRadius::all(px(value.min(16.0))),
            Token::Width => node.width = px(value.min(96.0)),
            Token::Height => node.height = px(value.min(64.0)),
            Token::BorderWidth => node.border = UiRect::all(px(value.min(12.0))),
            _ => {}
        },
    }
    let color = match value {
        TokenValue::Color(_) => value.color(),
        TokenValue::Number(_) => world
            .resource::<ThemeSettings>()
            .resolve(Token::Accent, None, &TokenOverrides::default())
            .0
            .color(),
    };
    world
        .entity_mut(entity)
        .insert((BackgroundColor(color), BorderColor::all(crate::theme::INK)));
}

pub(crate) fn autosave(world: &mut World) {
    let edits: Vec<_> = world
        .query::<(Entity, &EditableText, &TokenField)>()
        .iter(world)
        .filter(|(_, text, field)| {
            !text.is_composing()
                && text.pending_paste.is_none()
                && text.value().to_string() != field.observed
                && world
                    .get::<EditMode>(field.root)
                    .is_some_and(|mode| mode.enabled)
        })
        .map(|(entity, text, field)| {
            (
                entity,
                text.value().to_string(),
                field.scope,
                field.token,
                field.status,
                field.preview,
            )
        })
        .collect();
    let changed = !edits.is_empty();
    for (entity, text, scope, token, status, preview) in edits {
        world.get_mut::<TokenField>(entity).unwrap().observed = text.clone();
        if let Some(parsed) = token.parse(&text) {
            edit(world, scope, token, Some(parsed));
            world.entity_mut(status).remove::<InvalidValue>();
            preview_value(world, preview, token, parsed);
        } else {
            world.entity_mut(status).insert(InvalidValue);
            world.get_mut::<Text>(status).unwrap().0 = match token.definition().range {
                Some((min, max)) => format!("{min}–{max}"),
                None => "Invalid color".into(),
            };
        }
    }
    let summaries: Vec<_> = world
        .query_filtered::<(Entity, &OverrideSummary), Without<InvalidValue>>()
        .iter(world)
        .map(|(entity, summary)| {
            (
                entity,
                overwrites(world, summary.root, summary.scope, summary.token),
            )
        })
        .collect();
    for (entity, value) in summaries {
        if let Some(mut text) = world.get_mut::<Text>(entity)
            && text.0 != value
        {
            text.0 = value;
        }
    }
    let focus = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get());
    let updates: Vec<_> = world
        .query::<(Entity, &EditableText, &TokenField)>()
        .iter(world)
        .filter(|(entity, text, field)| {
            Some(*entity) != focus
                && !text.is_composing()
                && text.pending_paste.is_none()
                && world.get::<InvalidValue>(field.status).is_none()
                && text.value().to_string() == field.observed
        })
        .filter_map(|(entity, _, field)| {
            let current = value(world, field.scope, field.token).0;
            (current.display() != field.observed).then_some((
                entity,
                current,
                field.token,
                field.preview,
            ))
        })
        .collect();
    for (entity, current, token, preview) in updates {
        let value = current.display();
        world
            .get_mut::<EditableText>(entity)
            .unwrap()
            .editor
            .set_text(&value);
        world.get_mut::<TokenField>(entity).unwrap().observed = value;
        preview_value(world, preview, token, current);
    }
    if changed && let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
        wake.ring();
    }
}

pub(crate) fn toggle(event: On<GlobalCustomizationPanelToggle>, mut commands: Commands) {
    let root = event.entity;
    commands.queue(move |world: &mut World| {
        crate::edit_mode::toggle_customization(world, root);
    });
}

pub(crate) mod tests {
    use super::*;
    use crate::{
        actions::Action,
        container::BoxRoot,
        edit_mode::{EditAction, EditModePlugin},
        sand_store::{SandKind, spawn_sand},
        theme::ThemePlugin,
        workspace::WorkspacePlugin,
    };
    use bevy::math::DVec2;

    #[cfg_attr(test, test)]
    fn panel_lists_compiled_tokens_saves_valid_edits_and_resets_each_scope() {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>().add_plugins((
            ThemePlugin,
            WorkspacePlugin,
            EditModePlugin,
        ));
        let root = app.world_mut().spawn(BoxRoot).id();
        app.update();
        let sand = spawn_sand(app.world_mut(), root, 1, SandKind::Square, "", DVec2::ZERO);
        app.world_mut()
            .trigger(GlobalCustomizationPanelToggle { entity: root });
        app.update();
        let count = app
            .world_mut()
            .query::<&TokenField>()
            .iter(app.world())
            .count();
        assert_eq!(count + 1, TOKENS.len());
        let dropdown = app
            .world_mut()
            .query::<(Entity, &crate::dropdown::Dropdown, &AccessibilityNode)>()
            .iter(app.world())
            .find(|(_, _, node)| node.label() == Some("Colorscheme"))
            .map(|(entity, dropdown, _)| (entity, dropdown.menu))
            .unwrap();
        assert_eq!(
            app.world().get::<Node>(dropdown.1).unwrap().display,
            Display::None
        );
        assert_eq!(
            app.world().get::<Children>(dropdown.1).unwrap().len(),
            ColorScheme::ALL.len()
        );
        app.world_mut()
            .trigger(bevy::ui_widgets::Activate { entity: dropdown.0 });
        app.update();
        assert_eq!(
            app.world().get::<Node>(dropdown.1).unwrap().display,
            Display::Flex
        );
        let light = app
            .world()
            .get::<Children>(dropdown.1)
            .unwrap()
            .iter()
            .find(|entity| {
                app.world()
                    .get::<AccessibilityNode>(*entity)
                    .unwrap()
                    .label()
                    == Some("Lince Light")
            })
            .unwrap();
        app.world_mut()
            .trigger(bevy::ui_widgets::Activate { entity: light });
        app.update();
        assert_eq!(
            app.world().resource::<ThemeSettings>().scheme,
            ColorScheme::Light
        );
        let heading = app
            .world_mut()
            .query::<(&Text, &ChildOf)>()
            .iter(app.world())
            .find(|(text, _)| text.0 == "Customization")
            .unwrap()
            .1
            .parent();
        assert_eq!(
            app.world().get::<Node>(heading).unwrap().justify_content,
            JustifyContent::SpaceBetween
        );
        assert_eq!(app.world().get::<Children>(heading).unwrap().len(), 2);
        assert!(
            !app.world_mut()
                .query::<&Text>()
                .iter(app.world())
                .any(|text| {
                    text.0.contains("Changing the colorscheme")
                        || text.0.contains("Colors accept")
                        || text.0 == "Canvas colors"
                })
        );
        let slider = app
            .world_mut()
            .query_filtered::<Entity, With<crate::slider::SliderSand>>()
            .single(app.world())
            .unwrap();
        app.world_mut().trigger(bevy::ui_widgets::ValueChange {
            source: slider,
            value: 33.0_f32,
            is_final: false,
        });
        app.update();
        assert_eq!(
            app.world().resource::<ThemeSettings>().global.0[&Token::CanvasPattern],
            TokenValue::Number(35.0)
        );
        let field = app
            .world_mut()
            .query::<(Entity, &TokenField)>()
            .iter(app.world())
            .find(|(_, field)| field.token == Token::SandBackground)
            .unwrap()
            .0;
        app.world_mut()
            .get_mut::<EditableText>(field)
            .unwrap()
            .editor
            .set_text("#123456");
        app.update();
        app.update();
        assert_eq!(
            app.world().get::<BackgroundColor>(sand).unwrap().0,
            Color::srgb_u8(18, 52, 86)
        );
        app.world_mut()
            .get_mut::<EditableText>(field)
            .unwrap()
            .editor
            .set_text("#invalid");
        app.update();
        assert_eq!(
            app.world().resource::<ThemeSettings>().global.0[&Token::SandBackground],
            TokenValue::Color([18, 52, 86, 255])
        );
        let status = app.world().get::<TokenField>(field).unwrap().status;
        assert!(
            app.world()
                .get::<Text>(status)
                .unwrap()
                .0
                .contains("Invalid color")
        );
        CustomizationAction::Scheme(ColorScheme::Light).apply(app.world_mut(), root);
        app.update();
        assert_eq!(
            app.world().get::<BackgroundColor>(sand).unwrap().0,
            Color::srgb_u8(18, 52, 86)
        );
        CustomizationAction::Scope(Scope::Sand(sand)).apply(app.world_mut(), root);
        edit(
            app.world_mut(),
            Scope::Sand(sand),
            Token::Width,
            Some(TokenValue::Number(360.0)),
        );
        app.update();
        assert_eq!(
            app.world()
                .get::<crate::canvas::CanvasItem>(sand)
                .unwrap()
                .size
                .x,
            360.0
        );
        CustomizationAction::Reset(Token::Width).apply(app.world_mut(), root);
        app.update();
        assert_eq!(
            app.world()
                .get::<crate::canvas::CanvasItem>(sand)
                .unwrap()
                .size
                .x,
            248.0
        );
        CustomizationAction::ClearOverrides.apply(app.world_mut(), root);
        app.update();
        assert_eq!(
            app.world().get::<BackgroundColor>(sand).unwrap().0,
            Token::SandBackground
                .default_value(ColorScheme::Light)
                .color()
        );
        EditAction::Customization.apply(app.world_mut(), root);
        app.update();
        assert!(!app.world().get::<EditMode>(root).unwrap().enabled);
    }

    #[cfg_attr(test, test)]
    fn global_scheme_keeps_workspace_colors_until_overwrite_is_requested() {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>().add_plugins((
            ThemePlugin,
            WorkspacePlugin,
            EditModePlugin,
        ));
        let root = app.world_mut().spawn(BoxRoot).id();
        app.update();
        let mut spaces = app.world_mut().get_mut::<Workspaces>(root).unwrap();
        spaces.entries[0].colors.background = [1, 2, 3];
        spaces.entries[0].color_overrides[0] = true;
        app.world_mut()
            .trigger(GlobalCustomizationPanelToggle { entity: root });
        app.update();
        CustomizationAction::Scheme(ColorScheme::Light).apply(app.world_mut(), root);
        let colors = crate::canvas_background::current(app.world(), root);
        assert_eq!(colors.background, [1, 2, 3]);
        assert_eq!(colors.grid, [210, 214, 222]);
        assert!(
            overwrites(app.world(), root, Scope::All, Token::CanvasBackground).contains("#010203")
        );
        edit(
            app.world_mut(),
            Scope::Workspace(root, 1),
            Token::CanvasGrid,
            Some(TokenValue::Color([4, 5, 6, 255])),
        );
        assert_eq!(
            crate::canvas_background::current(app.world(), root).grid,
            [4, 5, 6]
        );
        edit(
            app.world_mut(),
            Scope::Workspace(root, 1),
            Token::CanvasGrid,
            None,
        );
        assert_eq!(
            crate::canvas_background::current(app.world(), root).grid,
            [210, 214, 222]
        );
        CustomizationAction::ClearOverrides.apply(app.world_mut(), root);
        assert_eq!(
            crate::canvas_background::current(app.world(), root).background,
            [248, 250, 252]
        );
    }

    crate::laboratory_cases! {
        panel_lists_compiled_tokens_saves_valid_edits_and_resets_each_scope,
        global_scheme_keeps_workspace_colors_until_overwrite_is_requested,
    }
}
