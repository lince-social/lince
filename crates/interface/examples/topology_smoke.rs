use bevy::{
    diagnostic::FrameCount,
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_interface::{
    actions::Action,
    app::interface_app,
    canvas::CanvasItem,
    container::BoxRoot,
    sand_store::{SandKind, spawn_sand},
    topology::{Spatial, view::View},
};

#[derive(Resource)]
struct Content(Entity, Entity);

#[derive(Resource, Default)]
struct Clicks(u32);

#[derive(Clone, Copy)]
struct Click;
impl Action for Click {
    fn apply(&self, world: &mut World, _: Entity) {
        world.resource_mut::<Clicks>().0 += 1;
    }
}

#[derive(Resource)]
struct ButtonSand(Entity);

#[derive(Resource)]
struct ModelDrag(Entity, DVec2);

#[derive(Resource)]
struct CameraBefore([f64; 3]);

fn walk_key(world: &mut World, pressed: bool) {
    let window = world
        .query_filtered::<Entity, With<Window>>()
        .single(world)
        .unwrap();
    world.write_message(bevy::input::keyboard::KeyboardInput {
        window,
        key_code: KeyCode::KeyW,
        logical_key: bevy::input::keyboard::Key::Character("w".into()),
        text: pressed.then(|| "w".into()),
        state: if pressed {
            bevy::input::ButtonState::Pressed
        } else {
            bevy::input::ButtonState::Released
        },
        repeat: false,
    });
}

fn cursor_on(world: &mut World, sand: Entity, pixel: Vec2) {
    let surface = world
        .get::<lince_interface::topology::presentation::Surface>(sand)
        .unwrap();
    let uv = pixel / surface.size;
    let point = world
        .get::<GlobalTransform>(surface.face)
        .unwrap()
        .transform_point(Vec3::new(uv.x - 0.5, 0.5 - uv.y, 0.0));
    cursor_at(world, point);
}

fn cursor_at(world: &mut World, point: Vec3) {
    let camera = world
        .resource::<lince_interface::topology::presentation::SceneCamera>()
        .0;
    let screen = world
        .get::<Camera>(camera)
        .unwrap()
        .world_to_viewport(world.get::<GlobalTransform>(camera).unwrap(), point)
        .unwrap();
    let (window, mut state) = world
        .query::<(Entity, &mut Window)>()
        .single_mut(world)
        .unwrap();
    state
        .bypass_change_detection()
        .set_cursor_position(Some(screen));
    let event = bevy::window::CursorMoved {
        window,
        position: screen,
        delta: None,
    };
    world.write_message(event.clone());
    world.write_message(bevy::window::WindowEvent::CursorMoved(event));
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    let root = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .iter(world)
        .next()
        .unwrap();
    match frame {
        12 => {
            let first = spawn_sand(
                world,
                root,
                1,
                SandKind::EditableText,
                &format!(
                    "Topology: editable content\n{}",
                    "Scrollable line\n".repeat(20)
                ),
                DVec2::new(-180.0, 0.0),
            );
            world.get_mut::<CanvasItem>(first).unwrap().size = Vec2::new(300.0, 100.0);
            world.entity_mut(first).insert(Spatial {
                world_pinned: true,
                ..default()
            });
            world.insert_resource(Content(
                first,
                world
                    .get::<lince_interface::sand_store::StoredSand>(first)
                    .unwrap()
                    .content
                    .unwrap(),
            ));
            let second = spawn_sand(
                world,
                root,
                1,
                SandKind::Square,
                "",
                DVec2::new(160.0, 60.0),
            );
            world.get_mut::<CanvasItem>(second).unwrap().size = Vec2::new(120.0, 180.0);
            world.entity_mut(second).insert(Spatial {
                depth: Some(60.0),
                world_pinned: true,
                ..default()
            });
            world.insert_resource(ButtonSand(second));
            let button = world
                .spawn((
                    lince_interface::sand::button(0),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0),
                        top: px(0),
                        width: px(80),
                        height: px(30),
                        ..default()
                    },
                    lince_interface::actions::ActionButton::new(
                        second,
                        lince_interface::actions![Click],
                    ),
                    ChildOf(second),
                ))
                .id();
            world.spawn((Text::new("Test"), ChildOf(button)));
            let mut colors = lince_interface::tokens::TokenOverrides::default();
            colors.set(
                lince_interface::tokens::Token::SandBackground,
                lince_interface::tokens::TokenValue::Color([48, 140, 150, 255]),
            );
            lince_interface::token_style::set_overrides(world, second, colors);
            let mut area = lince_interface::area::InfluenceArea::new(
                lince_interface::area::AreaShape::Circle,
                DVec2::new(40.0, -180.0),
                DVec2::splat(140.0),
            );
            area.depth = 90.0;
            lince_interface::area::spawn_area(world, root, 1, area).unwrap();
            lince_interface::edit_mode::EditAction::Open.apply(world, root);
            let directory = world
                .resource::<lince_interface::topology::assets::AssetDirectory>()
                .0
                .clone();
            let source = tempfile::tempdir().unwrap();
            let path = source.path().join("triangle.gltf");
            let mut document: serde_json::Value =
                serde_json::from_str(include_str!("../fixtures/open_scene.gltf")).unwrap();
            document["nodes"][0]["translation"] = serde_json::json!([0, 0, 0]);
            document["materials"] = serde_json::json!([{ "doubleSided": true, "pbrMetallicRoughness": { "baseColorFactor": [0.7, 0.4, 0.9, 1.0] } }]);
            document["meshes"][0]["primitives"][0]["material"] = serde_json::json!(0);
            std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
            let asset = lince_interface::topology::assets::copy_import(&path, &directory).unwrap();
            document["buffers"][0]
                .as_object_mut()
                .unwrap()
                .remove("uri");
            let mut json = serde_json::to_vec(&document).unwrap();
            while json.len() % 4 != 0 {
                json.push(b' ');
            }
            let mut binary = Vec::new();
            for number in [0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0] {
                binary.extend(number.to_le_bytes());
            }
            for index in [0_u16, 1, 2, 0] {
                binary.extend(index.to_le_bytes());
            }
            let mut glb = b"glTF".to_vec();
            glb.extend(2_u32.to_le_bytes());
            glb.extend(((28 + json.len() + binary.len()) as u32).to_le_bytes());
            glb.extend((json.len() as u32).to_le_bytes());
            glb.extend(b"JSON");
            glb.extend(json);
            glb.extend((binary.len() as u32).to_le_bytes());
            glb.extend(b"BIN\0");
            glb.extend(binary);
            let binary_path = source.path().join("triangle.glb");
            std::fs::write(&binary_path, glb).unwrap();
            let binary_asset =
                lince_interface::topology::assets::copy_import(&binary_path, &directory).unwrap();
            for x in [-150.0, 150.0] {
                lince_interface::topology::assets::spawn(
                    world,
                    root,
                    1,
                    DVec2::new(x, -250.0),
                    if x < 0.0 {
                        asset.clone()
                    } else {
                        binary_asset.clone()
                    },
                );
            }
        }
        24 => {
            lince_interface::edit_mode::EditAction::Close.apply(world, root);
            let mut view = world.get_mut::<View>(root).unwrap();
            view.spatial = true;
            view.position = [0.0, 480.0, 650.0];
            view.pitch = -0.65;
        }
        30 => {
            lince_interface::edit_mode::EditAction::Close.apply(world, root);
            let sand = world.resource::<Content>().0;
            let item = *world.get::<CanvasItem>(sand).unwrap();
            let camera = world
                .resource::<lince_interface::topology::presentation::SceneCamera>()
                .0;
            let point = Vec3::new(
                item.position.x as f32 - item.size.x * 0.5 + 40.0,
                0.01,
                item.position.y as f32 - item.size.y * 0.5 + 20.0,
            );
            let screen = world
                .get::<Camera>(camera)
                .unwrap()
                .world_to_viewport(world.get::<GlobalTransform>(camera).unwrap(), point)
                .unwrap();
            let (window, mut state) = world
                .query::<(Entity, &mut Window)>()
                .single_mut(world)
                .unwrap();
            state
                .bypass_change_detection()
                .set_cursor_position(Some(screen));
            let event = bevy::window::CursorMoved {
                window,
                position: screen,
                delta: None,
            };
            world.write_message(event.clone());
            world.write_message(bevy::window::WindowEvent::CursorMoved(event));
        }
        35 => {
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("/tmp/lince-topology-input.png"));
        }
        40 | 42 | 185 | 187 | 250 | 252 => {
            let window = world
                .query::<Entity>()
                .iter(world)
                .find(|entity| world.get::<Window>(*entity).is_some())
                .unwrap();
            let event = bevy::input::mouse::MouseButtonInput {
                window,
                button: MouseButton::Left,
                state: if matches!(frame, 40 | 185 | 250) {
                    bevy::input::ButtonState::Pressed
                } else {
                    bevy::input::ButtonState::Released
                },
            };
            world.write_message(event);
            world.write_message(bevy::window::WindowEvent::MouseButtonInput(event));
        }
        48 => {
            let editor = world.resource::<Content>().1;
            assert_eq!(
                world.resource::<bevy::input_focus::InputFocus>().get(),
                Some(editor),
                "Clicking the content face must focus its text editor"
            );
            let window = world
                .query_filtered::<Entity, With<Window>>()
                .single(world)
                .unwrap();
            world.write_message(bevy::input::keyboard::KeyboardInput {
                window,
                key_code: KeyCode::KeyT,
                logical_key: bevy::input::keyboard::Key::Character(" typed".into()),
                text: Some(" typed".into()),
                state: bevy::input::ButtonState::Pressed,
                repeat: false,
            });
            world.insert_resource(CameraBefore(world.get::<View>(root).unwrap().position));
            walk_key(world, true);
        }
        50 => {
            assert_eq!(
                world.get::<View>(root).unwrap().position,
                world.resource::<CameraBefore>().0,
                "Typing movement keys must not fly the camera"
            );
            walk_key(world, false);
        }
        52 => {
            let window = world
                .query_filtered::<Entity, With<Window>>()
                .single(world)
                .unwrap();
            world.write_message(bevy::window::WindowEvent::MouseWheel(
                bevy::input::mouse::MouseWheel {
                    unit: bevy::input::mouse::MouseScrollUnit::Line,
                    x: 0.0,
                    y: -3.0,
                    window,
                    phase: bevy::input::touch::TouchPhase::Moved,
                },
            ));
        }
        60 => {
            assert!(
                world
                    .get::<bevy::ui::widget::TextScroll>(world.resource::<Content>().1)
                    .unwrap()
                    .0
                    .y
                    > 0.0,
                "Perspective scrolling must reach the content"
            );
            let editor = world.resource::<Content>().1;
            assert!(
                world
                    .get::<bevy::text::EditableText>(editor)
                    .unwrap()
                    .editor
                    .text()
                    .to_string()
                    .contains(" typed")
            );
            lince_interface::edit_mode::EditAction::Open.apply(world, root);
        }
        70 => {
            world.get_mut::<View>(root).unwrap().spatial = false;
        }
        80 => {
            assert_eq!(
                world
                    .query::<&lince_interface::topology::presentation::Surface>()
                    .iter(world)
                    .count(),
                2
            );
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("/tmp/lince-topology-top.png"));
        }
        100 => {
            let mut view = world.get_mut::<View>(root).unwrap();
            view.spatial = true;
            view.position = [0.0, 480.0, 650.0];
            view.pitch = -0.65;
        }
        110 => {
            let imported = world
                .query_filtered::<Entity, With<lince_interface::topology::assets::ImportedAsset>>()
                .iter(world)
                .next()
                .unwrap();
            let square = world
                .query::<(Entity, &lince_interface::sand_store::StoredSand)>()
                .iter(world)
                .find(|(_, sand)| sand.kind == SandKind::Square)
                .unwrap()
                .0;
            for entity in [imported, square] {
                world
                    .entity_mut(entity)
                    .insert(lince_interface::canvas_selection::SandGroup([9; 16]));
            }
            lince_interface::topology::groups::attach(world, &[imported, square]);
            lince_interface::topology::groups::transform(
                world,
                imported,
                bevy::math::DVec3::new(0.0, 80.0, 0.0),
                bevy::math::DQuat::from_rotation_y(0.2),
            );
        }
        170 => {
            lince_interface::edit_mode::EditAction::Close.apply(world, root);
        }
        171 => {
            world
                .resource_mut::<bevy::input_focus::InputFocus>()
                .clear();
            world.insert_resource(CameraBefore(world.get::<View>(root).unwrap().position));
            walk_key(world, true);
        }
        173 => {
            let before = bevy::math::DVec3::from_array(world.resource::<CameraBefore>().0);
            let after = bevy::math::DVec3::from_array(world.get::<View>(root).unwrap().position);
            assert!(
                after.distance(before) > 0.1,
                "Movement keys must fly when text is not focused"
            );
            walk_key(world, false);
        }
        180 => {
            cursor_on(
                world,
                world.resource::<ButtonSand>().0,
                Vec2::new(20.0, 15.0),
            );
        }
        195 => {
            assert_eq!(
                world.resource::<Clicks>().0,
                1,
                "Perspective content buttons must receive clicks"
            );
            lince_interface::edit_mode::EditAction::Open.apply(world, root);
        }
        220 => {
            let mut spaces = world
                .get_mut::<lince_interface::workspace::Workspaces>(root)
                .unwrap();
            let mut other = spaces.entries[0].clone();
            other.id = 2;
            other.name = "Other".into();
            spaces.entries.push(other);
            assert!(lince_interface::workspace::switch(world, root, 2));
        }
        225 => {
            assert!(world.query_filtered::<&Visibility, With<lince_interface::topology::assets::ImportedAsset>>()
                .iter(world).all(|visibility| *visibility == Visibility::Hidden));
            assert!(
                world
                    .query::<&lince_interface::topology::presentation::Surface>()
                    .iter(world)
                    .all(|surface| !world.get::<Camera>(surface.camera).unwrap().is_active)
            );
            assert!(lince_interface::workspace::switch(world, root, 1));
        }
        235 => {
            assert!(world.query_filtered::<&Visibility, With<lince_interface::topology::assets::ImportedAsset>>()
                .iter(world).all(|visibility| *visibility == Visibility::Visible));
            world.get_mut::<View>(root).unwrap().spatial = false;
        }
        240 | 270 => {
            let offset =
                bevy::math::DVec3::new(1e9, 0.0, -1e9) * if frame == 240 { 1.0 } else { -1.0 };
            let entities: Vec<_> = world
                .query::<(Entity, &CanvasItem, &ChildOf)>()
                .iter(world)
                .filter(|(_, _, parent)| parent.parent() == root)
                .map(|(entity, _, _)| entity)
                .collect();
            for entity in entities {
                let position = lince_interface::topology::position(world, entity).unwrap();
                lince_interface::topology::set_position(world, entity, position + offset);
                if let Some(mut pose) =
                    world.get_mut::<lince_interface::topology::groups::GroupPose>(entity)
                {
                    pose.position =
                        (bevy::math::DVec3::from_array(pose.position) + offset).to_array();
                }
            }
            world
                .get_mut::<lince_interface::canvas::CanvasView>(root)
                .unwrap()
                .center += DVec2::new(offset.x, offset.z);
            if frame == 270 {
                let mut view = world.get_mut::<View>(root).unwrap();
                view.spatial = true;
                view.position = [0.0, 480.0, 650.0];
                view.pitch = -0.65;
            }
        }
        245 => {
            let model = world
                .query::<(Entity, &lince_interface::topology::assets::ImportedAsset)>()
                .iter(world)
                .find(|(_, asset)| asset.file == "source.glb")
                .unwrap()
                .0;
            world.insert_resource(ModelDrag(
                model,
                world.get::<CanvasItem>(model).unwrap().position,
            ));
            let point = world
                .get::<GlobalTransform>(model)
                .unwrap()
                .transform_point(Vec3::new(0.25, 0.0, 0.25));
            cursor_at(world, point);
        }
        248 => {
            assert_eq!(
                world
                    .resource::<lince_interface::topology::input::PointerState>()
                    .hit
                    .map(|hit| hit.0),
                Some(world.resource::<ModelDrag>().0),
                "Model picking must survive floating-origin rebasing"
            );
        }
        251 => {
            let model = world.resource::<ModelDrag>().0;
            let point = world
                .get::<GlobalTransform>(model)
                .unwrap()
                .transform_point(Vec3::new(0.25, 0.0, 0.25));
            cursor_at(world, point + Vec3::new(30.0, 0.0, 10.0));
        }
        255 => {
            let ModelDrag(model, before) = *world.resource::<ModelDrag>();
            assert!(
                world
                    .get::<CanvasItem>(model)
                    .unwrap()
                    .position
                    .distance(before + DVec2::new(30.0, 10.0))
                    < 0.01,
                "Imported models must drag in canvas coordinates"
            );
            assert!(
                world
                    .get::<lince_interface::canvas_selection::SandSelection>(root)
                    .unwrap()
                    .0
                    .contains(&model)
            );
            lince_interface::topology::ui::TopologyAction::Resize(0, 10.0).apply(world, model);
        }
        260 => {
            let model = world.resource::<ModelDrag>().0;
            let point = world
                .get::<GlobalTransform>(model)
                .unwrap()
                .transform_point(Vec3::new(0.25, 0.0, 0.25));
            cursor_at(world, point);
        }
        265 => {
            assert_eq!(
                world
                    .resource::<lince_interface::topology::input::PointerState>()
                    .hit
                    .map(|hit| hit.0),
                Some(world.resource::<ModelDrag>().0),
                "Resizing must refresh indexed picking"
            );
        }
        300 => {
            assert_eq!(
                world
                    .query::<&lince_interface::topology::assets::Bounds>()
                    .iter(world)
                    .count(),
                2
            );
            assert_eq!(
                world
                    .query::<&lince_interface::topology::physics::Body>()
                    .iter(world)
                    .count(),
                3
            );
            assert_eq!(
                world
                    .query::<&lince_interface::topology::physics::Body>()
                    .iter(world)
                    .filter(|body| body.members.len() == 2)
                    .count(),
                1
            );
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("/tmp/lince-topology-spatial.png"))
                .observe(
                    |event: On<ScreenshotCaptured>, mut exits: MessageWriter<AppExit>| {
                        assert!(
                            event
                                .image
                                .data
                                .as_ref()
                                .unwrap()
                                .chunks_exact(4)
                                .any(|p| p[0] > 100)
                        );
                        println!("Topology rendering smoke passed");
                        exits.write(AppExit::Success);
                    },
                );
        }
        600 => panic!("Topology rendering timed out"),
        _ => {}
    }
}

fn main() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _guard = runtime.enter();
    interface_app()
        .add_plugins(bevy::log::LogPlugin::default())
        .insert_resource(WinitSettings::continuous())
        .init_resource::<Clicks>()
        .add_systems(
            Startup,
            |mut commands: Commands, mut windows: Query<&mut Window>| {
                windows.single_mut().unwrap().resolution.set(1100.0, 800.0);
                commands.spawn(BoxRoot);
            },
        )
        .add_systems(Update, exercise)
        .run();
}
