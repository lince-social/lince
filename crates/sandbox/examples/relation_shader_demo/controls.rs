use bevy::{
    input_focus::{FocusCause, InputFocus},
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
};
use lince_sandbox::{
    actions::{Action, ActionButton},
    edit_mode::EditAction,
    topology::{
        appearance::{Light, Material},
        assets::AssetDirectory,
        presentation::Surface,
    },
};
use nucleus::canvas::appearance as model;
use std::time::{Duration, Instant};

const TEXTURE: &str = "000000000000000000000000000000d1";

pub(super) struct ReviewPlugin;

impl Plugin for ReviewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<State>().add_systems(
            Update,
            update.after(lince_sandbox::topology::appearance::update),
        );
    }
}

#[derive(Resource, Default)]
struct State(Option<Review>);

struct Review {
    row: Entity,
    binding: lince_sandbox::protein_area::RecordBinding,
    directory: tempfile::TempDir,
    captures: String,
    phase: usize,
    started: Instant,
    capturing: bool,
    focused: Option<Entity>,
}

pub(super) fn active(world: &World) -> bool {
    world.resource::<State>().0.is_some()
}

pub(super) fn start(world: &mut World, root: Entity, row: Entity, captures: String) {
    let directory = tempfile::tempdir().unwrap();
    let binding = world
        .get::<lince_sandbox::protein_area::RecordBinding>(row)
        .unwrap()
        .clone();
    world.insert_resource(AssetDirectory(directory.path().into()));
    world.get_mut::<Material>(row).unwrap().0.texture =
        model::Texture::Asset { id: TEXTURE.into() };
    EditAction::Open.apply(world, root);
    world
        .get_mut::<lince_sandbox::inspection::Inspection>(root)
        .unwrap()
        .selected = Some(row);
    world.resource_mut::<State>().0 = Some(Review {
        row,
        binding,
        directory,
        captures,
        phase: 0,
        started: Instant::now(),
        capturing: false,
        focused: None,
    });
}

fn button(world: &mut World, row: Entity, caption: &str) -> Option<Entity> {
    let labels: Vec<_> = world
        .query::<(Entity, &Text)>()
        .iter(world)
        .filter(|(_, text)| text.0 == caption)
        .map(|(entity, _)| entity)
        .collect();
    for label in labels {
        let mut current = Some(label);
        while let Some(entity) = current {
            if world
                .get::<ActionButton>(entity)
                .is_some_and(|button| button.target == row)
            {
                return Some(entity);
            }
            current = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
    }
    None
}

fn press(world: &mut World, row: Entity, caption: &str) {
    let entity = button(world, row, caption)
        .unwrap_or_else(|| panic!("Missing native appearance control: {caption}"));
    let action = world.get::<ActionButton>(entity).unwrap().clone();
    action.actions.run(world, action.target);
}

fn field(world: &mut World, caption: &str, value: &str) {
    let entity = world
        .query::<(
            Entity,
            &bevy::a11y::AccessibilityNode,
            &bevy::text::EditableText,
        )>()
        .iter(world)
        .find(|(_, node, _)| node.label() == Some(caption))
        .map(|(entity, _, _)| entity)
        .unwrap_or_else(|| panic!("Missing native appearance field: {caption}"));
    world
        .get_mut::<bevy::text::EditableText>(entity)
        .unwrap()
        .editor
        .set_text(value);
}

fn revealed(world: &World, button: Entity) -> bool {
    let center = world
        .get::<UiGlobalTransform>(button)
        .unwrap()
        .translation
        .y;
    let size = world.get::<ComputedNode>(button).unwrap().size().y;
    let mut current = world.get::<ChildOf>(button).map(ChildOf::parent);
    while let Some(entity) = current {
        if world.get::<ScrollPosition>(entity).is_some() {
            let panel = world.get::<ComputedNode>(entity).unwrap();
            let target = world
                .get::<UiGlobalTransform>(entity)
                .unwrap()
                .translation
                .y;
            let top = target - panel.size().y * 0.5 + 8.0;
            let bottom = target + panel.size().y * 0.5 - 8.0;
            return size > 0.0
                && center - size * 0.5 >= top - 1.0
                && center + size * 0.5 <= bottom + 1.0;
        }
        current = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    panic!("Appearance controls must have a scrollable native panel");
}

fn loaded(world: &World, row: Entity) -> bool {
    world
        .get::<Surface>(row)
        .and_then(|surface| world.get::<MeshMaterial3d<StandardMaterial>>(surface.body))
        .and_then(|handle| world.resource::<Assets<StandardMaterial>>().get(&handle.0))
        .and_then(|material| material.base_color_texture.as_ref())
        .is_some()
}

fn act(world: &World, phase: usize, action: engine::actions::Action) {
    world
        .get_non_send::<lince_sandbox::cell_bridge::CellBridge>()
        .unwrap()
        .outgoing
        .try_send(cell::ClientMessage::Act {
            id: format!("relation-light-driver-{phase}"),
            action,
        })
        .unwrap();
}

fn quantity(world: &World, uid: &str) -> Option<f64> {
    let engine = &world.resource::<lince_sandbox::app::CellHandle>().0.engine;
    tokio::runtime::Handle::current()
        .block_on(store::records::get(&engine.store.pool, uid))
        .unwrap()
        .map(|row| row.quantity_f64())
}

fn freeze(world: &mut World, binding: &lince_sandbox::protein_area::RecordBinding, frozen: bool) {
    let mut area = world
        .get_mut::<lince_sandbox::area::InfluenceArea>(binding.area)
        .unwrap();
    let defaults = area.protein.as_ref().unwrap().shader_placement.clone();
    let saved = area.records.entry(binding.uid.clone()).or_default();
    let mut settings = saved.shader_placement.clone().unwrap_or(defaults);
    settings.frozen = frozen;
    saved.shader_placement = Some(settings);
}

fn update(world: &mut World) {
    let Some(review) = world.resource::<State>().0.as_ref() else {
        return;
    };
    assert!(
        review.started.elapsed() < Duration::from_secs(45),
        "Appearance controls replay timed out at phase {}",
        review.phase
    );
    if review.capturing || review.started.elapsed() < Duration::from_secs(4) {
        return;
    }
    let (row, phase, captures, binding) = (
        review.row,
        review.phase,
        review.captures.clone(),
        review.binding.clone(),
    );
    for mut text in world
        .query_filtered::<&mut Text, With<super::DemoLabel>>()
        .iter_mut(world)
    {
        text.0 = match phase {
            0 => "Native controls · missing texture fallback · Reset / Disable stay available",
            1 => "Retry recovered the local texture package",
            2 => "Native controls disabled material and attached light",
            3 => "Real Record quantity −1.5 · Progress drives the light to 50%",
            4 => "Freeze keeps the light at 50% while the real Record reaches quantity 0",
            5 => "Unfreeze follows quantity 0 · Progress light reaches 100%",
            _ => "Source Record removed · its light and WGSL geometry are released",
        }
        .into();
    }
    if (3..=5).contains(&phase) {
        if quantity(world, &binding.uid) != Some(if phase == 3 { -1.5 } else { 0.0 }) {
            return;
        }
        let expected = world.get::<Light>(row).unwrap().0.intensity
            * 10000.0
            * if phase == 5 { 1.0 } else { 0.5 };
        let actual = world
            .query::<(
                &PointLight,
                &lince_sandbox::topology::presentation::VisualOwner,
            )>()
            .iter(world)
            .find(|(_, owner)| owner.0 == row)
            .map(|(light, _)| light.intensity);
        if actual.is_none_or(|value| (value - expected).abs() > expected * 0.001) {
            return;
        }
    } else if phase == 6 {
        if quantity(world, &binding.uid).is_some() || world.get_entity(row).is_ok() {
            return;
        }
        if world
            .query::<(
                &PointLight,
                &lince_sandbox::topology::presentation::VisualOwner,
            )>()
            .iter(world)
            .any(|(_, owner)| owner.0 == row)
        {
            return;
        }
        assert!(
            !world
                .query::<&lince_sandbox::shader_effects::RecordMesh>()
                .iter(world)
                .any(|mesh| mesh.0 == row)
        );
    }
    let caption = if phase == 1 {
        "Retry missing texture"
    } else {
        "Reset material"
    };
    if phase == 0 {
        assert!(
            world
                .query::<&Text>()
                .iter(world)
                .any(|text| text.0.starts_with("Material:") && text.0.lines().count() > 3),
            "Missing texture must report recovery status"
        );
        assert!(!loaded(world, row));
    } else if phase == 1 && !loaded(world, row) {
        return;
    } else if phase == 2 {
        assert!(world.get::<Material>(row).is_none());
        assert!(!world.get::<Light>(row).unwrap().0.enabled);
    }
    if phase < 6 {
        let Some(control) = button(world, row, caption) else {
            return;
        };
        if world.resource::<State>().0.as_ref().unwrap().focused != Some(control) {
            world
                .resource_mut::<InputFocus>()
                .set(control, FocusCause::Navigated);
            world.resource_mut::<State>().0.as_mut().unwrap().focused = Some(control);
            return;
        }
        if !revealed(world, control) {
            return;
        }
    }
    let intensity = world
        .query::<(
            &PointLight,
            &lince_sandbox::topology::presentation::VisualOwner,
        )>()
        .iter(world)
        .find(|(_, owner)| owner.0 == row)
        .map(|(light, _)| light.intensity);
    eprintln!(
        "Appearance controls phase {phase}: quantity={:?}; light intensity={intensity:?}",
        quantity(world, &binding.uid)
    );
    world.resource_mut::<State>().0.as_mut().unwrap().capturing = true;
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(format!(
            "{captures}/appearance-controls-{phase}.png"
        )))
        .observe(move |_: On<ScreenshotCaptured>, mut commands: Commands| {
            let binding = binding.clone();
            commands.queue(move |world: &mut World| {
                match phase {
                    0 => {
                        press(world, row, "Reset material");
                        assert_eq!(
                            world.get::<Material>(row).unwrap().0.texture,
                            model::Texture::Plain
                        );
                        press(world, row, "Disable material");
                        assert!(world.get::<Material>(row).is_none());
                        press(world, row, "Enable material");
                        world.get_mut::<Material>(row).unwrap().0.texture =
                            model::Texture::Asset { id: TEXTURE.into() };
                        let path = world
                            .resource::<State>()
                            .0
                            .as_ref()
                            .unwrap()
                            .directory
                            .path()
                            .join(TEXTURE);
                        std::fs::create_dir(&path).unwrap();
                        image::RgbaImage::from_pixel(8, 8, image::Rgba([100, 180, 220, 255]))
                            .save(path.join("source.png"))
                            .unwrap();
                        press(world, row, "Retry missing texture");
                    }
                    1 => {
                        assert!(loaded(world, row));
                        press(world, row, "Reset attached light");
                        press(world, row, "Disable attached light");
                        press(world, row, "Disable material");
                    }
                    2 => {
                        press(world, row, "Enable material");
                        field(world, "Emission (0–32)", "2");
                        press(world, row, "Apply material and light fields");
                        press(world, row, "Enable attached light");
                        for _ in 0..3 {
                            press(world, row, "Next light driver");
                        }
                        assert_eq!(
                            world.get::<Light>(row).unwrap().0.drive,
                            model::LightDrive::Progress
                        );
                        act(
                            world,
                            phase,
                            engine::actions::Action::SetQuantity {
                                target: binding.uid.clone(),
                                value: -1.5,
                            },
                        );
                    }
                    3 => {
                        freeze(world, &binding, true);
                        act(
                            world,
                            phase,
                            engine::actions::Action::SetQuantity {
                                target: binding.uid.clone(),
                                value: 0.0,
                            },
                        );
                    }
                    4 => {
                        freeze(world, &binding, false);
                    }
                    5 => {
                        act(
                            world,
                            phase,
                            engine::actions::Action::DeleteRecord {
                                target: binding.uid.clone(),
                            },
                        );
                    }
                    _ => {
                        world.write_message(AppExit::Success);
                    }
                }
                let review = world
                    .resource_mut::<State>()
                    .into_inner()
                    .0
                    .as_mut()
                    .unwrap();
                review.phase += 1;
                review.started = Instant::now();
                review.capturing = false;
                review.focused = None;
            });
        });
}
