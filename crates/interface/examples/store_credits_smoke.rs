use bevy::{
    diagnostic::FrameCount,
    input::{ButtonState, mouse::MouseButtonInput},
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::{CursorMoved, PrimaryWindow, WindowEvent},
    winit::WinitSettings,
};
use lince_interface::{
    actions::dispatch,
    app::interface_app,
    container::BoxRoot,
    credits::LicenseAccordion,
    edit_mode::{EditAction, EditField, EditMode},
    icons::{IconButton, Tooltip},
    sand_store::StoreEntry,
};

#[derive(Resource)]
struct Captures {
    path: String,
    count: usize,
    accordion: Option<(Entity, Entity)>,
}

fn rect(world: &World, entity: Entity) -> Rect {
    let node = world.get::<ComputedNode>(entity).unwrap();
    Rect::from_center_size(
        world.get::<UiGlobalTransform>(entity).unwrap().translation * node.inverse_scale_factor(),
        node.size() * node.inverse_scale_factor(),
    )
}

fn pointer(world: &mut World, entity: Entity) {
    let position = rect(world, entity).center();
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap();
    world.write_message(WindowEvent::CursorMoved(CursorMoved {
        window,
        position,
        delta: None,
    }));
}

fn press(world: &mut World, down: bool) {
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap();
    world.write_message(WindowEvent::MouseButtonInput(MouseButtonInput {
        window,
        button: MouseButton::Left,
        state: if down {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
    }));
}

fn capture(world: &mut World, name: &str) {
    let path = format!("{}.{}.png", world.resource::<Captures>().path, name);
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path))
        .observe(|_: On<ScreenshotCaptured>, mut captures: ResMut<Captures>| captures.count += 1);
}

fn store_layout(world: &mut World, root: Entity) {
    let panel = world.get::<EditMode>(root).unwrap().panel;
    assert_eq!(
        world.get::<Node>(panel).unwrap().justify_content,
        JustifyContent::FlexStart
    );
    let editor = world
        .query::<(Entity, &EditField)>()
        .iter(world)
        .find(|(_, field)| **field == EditField::StartingText)
        .unwrap()
        .0;
    let mut entries: Vec<_> = world
        .query_filtered::<Entity, With<StoreEntry>>()
        .iter(world)
        .collect();
    entries.sort_by(|a, b| rect(world, *a).min.y.total_cmp(&rect(world, *b).min.y));
    assert_eq!(entries.len(), 3);
    assert!((rect(world, entries[0]).min.y - rect(world, editor).max.y - 10.0).abs() <= 2.0);
    for pair in entries.windows(2) {
        assert!((rect(world, pair[1]).min.y - rect(world, pair[0]).max.y - 10.0).abs() <= 2.0);
    }
    for entity in world.query_filtered::<Entity, With<Tooltip>>().iter(world) {
        let mut cursor = Some(entity);
        while let Some(entity) = cursor {
            assert!(
                world.get::<StoreEntry>(entity).is_none(),
                "Store Sands must not show tooltips"
            );
            cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
    }
    let title = world
        .query::<(Entity, &Text)>()
        .iter(world)
        .find(|(_, text)| text.0 == "Sand store")
        .unwrap()
        .0;
    let credits = world
        .query::<(Entity, &IconButton)>()
        .iter(world)
        .find(|(_, icon)| icon.label == "Sand credits and licenses")
        .unwrap()
        .0;
    assert_eq!(
        world.get::<ChildOf>(title).unwrap().parent(),
        world.get::<ChildOf>(credits).unwrap().parent()
    );
    assert!(rect(world, credits).min.x > rect(world, title).max.x);
    for node in world
        .query_filtered::<&Node, With<bevy::ui_widgets::Button>>()
        .iter(world)
    {
        assert_eq!(
            node.border,
            UiRect::all(px(lince_interface::sand::BUTTON_BORDER_WIDTH))
        );
    }
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    if frame < 12 {
        return;
    }
    let root = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(world)
        .unwrap();
    match frame {
        12 => dispatch(
            world,
            root,
            lince_interface::actions![EditAction::Open, EditAction::Store],
        ),
        24 => {
            store_layout(world, root);
            capture(world, "store");
        }
        36 => {
            let credits = world
                .query::<(Entity, &IconButton)>()
                .iter(world)
                .find(|(_, icon)| icon.label == "Sand credits and licenses")
                .unwrap()
                .0;
            pointer(world, credits);
        }
        38 | 48 | 58 => press(world, true),
        40 | 50 | 60 => press(world, false),
        44 => {
            let accordions: Vec<_> = world
                .query::<(Entity, &LicenseAccordion)>()
                .iter(world)
                .map(|(entity, accordion)| (entity, accordion.body))
                .collect();
            assert_eq!(
                accordions.len(),
                lince_interface::credits::ATTRIBUTIONS.len()
            );
            assert!(
                accordions
                    .iter()
                    .all(|(_, body)| world.get::<Node>(*body).unwrap().display == Display::None)
            );
            world.resource_mut::<Captures>().accordion = Some(accordions[0]);
            capture(world, "licenses");
        }
        46 | 56 => {
            let (button, _) = world.resource::<Captures>().accordion.unwrap();
            pointer(world, button);
        }
        54 => {
            let (button, body) = world.resource::<Captures>().accordion.unwrap();
            assert_eq!(world.get::<Node>(body).unwrap().display, Display::Flex);
            assert_eq!(world.get::<LicenseAccordion>(button).unwrap().body, body);
            assert!(rect(world, body).height() > 100.0);
            capture(world, "expanded");
        }
        64 => {
            let (_, body) = world.resource::<Captures>().accordion.unwrap();
            assert_eq!(world.get::<Node>(body).unwrap().display, Display::None);
            world
                .query_filtered::<&mut Window, With<PrimaryWindow>>()
                .single_mut(world)
                .unwrap()
                .resolution
                .set(440.0, 620.0);
        }
        70 => dispatch(world, root, lince_interface::actions![EditAction::Store]),
        78 => {
            store_layout(world, root);
            capture(world, "narrow");
        }
        100 => {
            assert_eq!(world.resource::<Captures>().count, 4);
            println!(
                "Store credits smoke passed: top alignment, title-row credits, no Sand tooltips, single button borders, and real clicks expanding and collapsing licenses."
            );
            world.write_message(AppExit::Success);
        }
        _ => {}
    }
}

fn main() {
    interface_app()
        .insert_resource(Captures {
            path: std::env::args().nth(1).expect("provide screenshot path"),
            count: 0,
            accordion: None,
        })
        .insert_resource(WinitSettings::continuous())
        .add_systems(
            Startup,
            |mut commands: Commands, mut windows: Query<&mut Window>| {
                windows.single_mut().unwrap().resolution.set(1000.0, 800.0);
                commands.spawn(BoxRoot);
            },
        )
        .add_systems(Update, exercise)
        .run();
}
