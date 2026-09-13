use super::*;
use crate::{
    actions::{Action, ActionButton},
    edit_mode::label,
    icons::{Icon, IconButton, Tooltip},
};

#[derive(Clone, Copy)]
enum Change {
    Sort,
    Horizontal,
    Reverse,
    Mode(ForceMode),
    Immunity(Immunity),
    Scale(f32),
    Strength(f32),
}

fn allowed(world: &World, owner: Entity) -> Option<Entity> {
    let root = world.get::<ChildOf>(owner)?.parent();
    (crate::area_panel::owns(world, root, owner)
        && world
            .get::<crate::edit_mode::EditMode>(root)
            .is_some_and(|m| m.enabled && m.areas)
        && world
            .get::<crate::area_panel::AreaEditor>(root)
            .is_some_and(|e| e.selected == Some(owner)))
    .then_some(root)
}

impl Action for Change {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(root) = allowed(world, owner) else {
            return;
        };
        let mut area = world.get::<InfluenceArea>(owner).unwrap().clone();
        match *self {
            Self::Sort => {
                area.sorting = if area.sorting.is_some() {
                    None
                } else {
                    Some(Sorting::default())
                }
            }
            Self::Horizontal => {
                if let Some(sort) = &mut area.sorting {
                    sort.horizontal = !sort.horizontal;
                }
            }
            Self::Reverse => {
                if let Some(sort) = &mut area.sorting {
                    sort.reverse = !sort.reverse;
                }
            }
            Self::Mode(mode) => area.force_mode = mode,
            Self::Immunity(immunity) => area.immunity = immunity,
            Self::Scale(scale) => area.scale = scale,
            Self::Strength(strength) => {
                if let Some(sort) = &mut area.sorting {
                    sort.strength = f64::from(strength);
                }
            }
        }
        if !area.validate() || world.get::<InfluenceArea>(owner) == Some(&area) {
            return;
        }
        crate::area_mutation::disarm(
            world,
            owner,
            "Disarmed after an Area edit. Preview again to arm.",
        );
        world.entity_mut(owner).insert(area);
        let values: Vec<_> = world
            .query::<(Entity, &ValueControl)>()
            .iter(world)
            .filter(|(_, c)| c.owner == owner)
            .map(|(e, c)| (e, c.scale))
            .collect();
        for (entity, scale) in values {
            let area = world.get::<InfluenceArea>(owner).unwrap();
            let value = if scale {
                area.scale
            } else {
                area.sorting.as_ref().map_or(0.0, |s| s.strength as f32)
            };
            crate::slider::set_value(world, entity, value);
        }
        if !matches!(self, Self::Scale(_) | Self::Strength(_)) {
            crate::edit_mode::render_panel(world, root);
        }
    }
}

fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    change: Change,
    icon: Icon,
    tip: &str,
    selected: bool,
) {
    let entity = world
        .spawn((
            IconButton::new(icon, tip),
            ActionButton::new(owner, crate::actions![change]),
            Tooltip(tip.into()),
            ChildOf(parent),
        ))
        .id();
    if selected {
        world.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: percent(25),
                bottom: px(1),
                width: percent(50),
                height: px(3),
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Accent),
            Pickable::IGNORE,
            ChildOf(entity),
        ));
    }
}

#[derive(Component)]
struct ValueControl {
    owner: Entity,
    scale: bool,
}

fn slider(world: &mut World, parent: Entity, owner: Entity, value: f32, scale: bool) {
    let tip = if scale {
        "Size multiplier while inside. Restores on exit; overlapping multipliers combine, limited to 0.05–20. Pinned Sands keep their size."
    } else {
        "Sorting force. Slots follow Protein query order. The line fits the Area bounds; enlarge the Area if Sands overlap. Movement requires workspace physics."
    };
    let config = crate::slider::SliderSand {
        start: if scale { 0.05 } else { 0.0 },
        end: if scale { 20.0 } else { 1000.0_f32.max(value) },
        step: if scale { 0.05 } else { 1.0 },
        decimals: 2,
    };
    let entity = crate::slider::spawn(
        world,
        parent,
        if scale {
            "Size multiplier"
        } else {
            "Sorting strength"
        },
        config,
        value,
        if scale { "×" } else { "" },
    )
    .unwrap();
    world
        .entity_mut(entity)
        .insert((Tooltip(tip.into()), ValueControl { owner, scale }))
        .observe(
            |event: On<crate::slider::SliderChanged>, mut commands: Commands| {
                let entity = event.entity;
                let value = event.value;
                commands.queue(move |world: &mut World| {
                    let Some(control) = world.get::<ValueControl>(entity) else {
                        return;
                    };
                    let owner = control.owner;
                    let scale = control.scale;
                    let Some(value) = world
                        .get::<crate::slider::SliderSand>(entity)
                        .and_then(|s| s.snap(value))
                    else {
                        return;
                    };
                    if scale {
                        Change::Scale(value)
                    } else {
                        Change::Strength(value)
                    }
                    .apply(world, owner);
                });
            },
        );
}

pub(crate) fn controls(world: &mut World, _root: Entity, panel: Entity, owner: Entity) {
    let area = world.get::<InfluenceArea>(owner).unwrap().clone();
    label(world, panel, "Sorting", 18.0);
    let row = crate::area_panel::row(world, panel);
    button(
        world,
        row,
        owner,
        Change::Sort,
        if area.sorting.is_some() {
            Icon::Stop
        } else {
            Icon::Play
        },
        "Toggle sorting of existing Sands. Uses the Protein filter order, or the spawning query order. Without a query, uses Record identity. Does not spawn Sands.",
        area.sorting.is_some(),
    );
    if let Some(sort) = area.sorting {
        button(
            world,
            row,
            owner,
            Change::Horizontal,
            if sort.horizontal {
                Icon::Forward
            } else {
                Icon::Scroll
            },
            "Toggle horizontal or vertical sorting",
            sort.horizontal,
        );
        button(
            world,
            row,
            owner,
            Change::Reverse,
            Icon::Backward,
            "Reverse the sorting direction",
            sort.reverse,
        );
        label(world, panel, "Sorting strength", 14.0);
        slider(world, panel, owner, sort.strength as f32, false);
    }
    label(world, panel, "Immunity", 18.0);
    let row = crate::area_panel::row(world, panel);
    for (mode, icon, tip) in [
        (Immunity::None, Icon::Stop, "Immunity off"),
        (
            Immunity::External,
            Icon::Backward,
            "Cancel Areas whose centers are outside this boundary",
        ),
        (
            Immunity::Internal,
            Icon::Forward,
            "Cancel other Areas whose centers are inside this boundary",
        ),
        (
            Immunity::All,
            Icon::Check,
            "Cancel all other Areas on Sands inside: forces, sorting, size and Record entry/exit changes. Immunity Areas do not cancel each other.",
        ),
    ] {
        button(
            world,
            row,
            owner,
            Change::Immunity(mode),
            icon,
            tip,
            area.immunity == mode,
        );
    }
    label(world, panel, "Size multiplier", 18.0);
    slider(world, panel, owner, area.scale, true);
    let row = crate::area_panel::row(world, panel);
    button(
        world,
        row,
        owner,
        Change::Scale(1.0),
        Icon::Reset,
        "Reset size multiplier to 1",
        area.scale == 1.0,
    );
    label(world, panel, "Physics influence", 18.0);
    let row = crate::area_panel::row(world, panel);
    button(
        world,
        row,
        owner,
        Change::Mode(ForceMode::Simple),
        Icon::Forward,
        "Simple: keep the destination and strength, steering toward that point as the Sand moves, or away when repelling. Distance does not weaken force. Force is zero at the destination; momentum can carry the Sand past it. Limited reach and immunity still apply.",
        area.force_mode == ForceMode::Simple,
    );
    button(
        world,
        row,
        owner,
        Change::Mode(ForceMode::Newtonian),
        Icon::Attract,
        "Newtonian: keep steering toward the target during movement, or away when repelling. Force falls with squared distance from the target outside half the Area's smallest side, and is capped at the configured strength.",
        area.force_mode == ForceMode::Newtonian,
    );
}

#[cfg_attr(test, test)]
fn effect_controls_save_values_reset_sliders_and_reject_foreign_or_closed_edits() {
    use crate::{area::ShapeKind, area_panel::AreaAction, edit_mode::EditAction};
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .add_plugins((
            crate::workspace::WorkspacePlugin,
            crate::edit_mode::EditModePlugin,
        ));
    let root = app.world_mut().spawn(crate::container::BoxRoot).id();
    app.update();
    EditAction::Open.apply(app.world_mut(), root);
    EditAction::Areas.apply(app.world_mut(), root);
    EditAction::Area(AreaAction::Add(ShapeKind::Square)).apply(app.world_mut(), root);
    let owner = app
        .world()
        .get::<crate::area_panel::AreaEditor>(root)
        .unwrap()
        .selected
        .unwrap();
    Change::Sort.apply(app.world_mut(), owner);
    Change::Horizontal.apply(app.world_mut(), owner);
    Change::Reverse.apply(app.world_mut(), owner);
    Change::Mode(ForceMode::Newtonian).apply(app.world_mut(), owner);
    Change::Immunity(Immunity::External).apply(app.world_mut(), owner);
    let slider = app
        .world_mut()
        .query::<(Entity, &ValueControl)>()
        .iter(app.world())
        .find(|(_, c)| c.owner == owner && c.scale)
        .unwrap()
        .0;
    for (value, expected) in [(0.5, 0.5), (f32::NAN, 0.5), (2.0, 2.0)] {
        app.world_mut().trigger(crate::slider::SliderChanged {
            entity: slider,
            value,
        });
        app.world_mut().flush();
        assert_eq!(
            app.world().get::<InfluenceArea>(owner).unwrap().scale,
            expected
        );
    }
    Change::Scale(1.0).apply(app.world_mut(), owner);
    assert_eq!(
        app.world()
            .get::<bevy::ui_widgets::SliderValue>(slider)
            .unwrap()
            .0,
        1.0
    );
    let area = app.world().get::<InfluenceArea>(owner).unwrap();
    assert_eq!(area.force_mode, ForceMode::Newtonian);
    assert_eq!(area.immunity, Immunity::External);
    assert!(area.sorting.as_ref().unwrap().horizontal);
    assert!(area.sorting.as_ref().unwrap().reverse);
    assert!(app.world().get::<Tooltip>(slider).is_some());
    let foreign = crate::area::spawn_area(
        app.world_mut(),
        root,
        2,
        InfluenceArea::new(
            crate::area::AreaShape::Square,
            DVec2::ZERO,
            DVec2::splat(100.0),
        ),
    )
    .unwrap();
    Change::Scale(2.0).apply(app.world_mut(), foreign);
    assert_eq!(
        app.world().get::<InfluenceArea>(foreign).unwrap().scale,
        1.0
    );
    EditAction::Close.apply(app.world_mut(), root);
    Change::Scale(2.0).apply(app.world_mut(), owner);
    assert_eq!(app.world().get::<InfluenceArea>(owner).unwrap().scale, 1.0);
}

crate::laboratory_cases! { effect_controls_save_values_reset_sliders_and_reject_foreign_or_closed_edits, }
