use super::*;

#[test]
fn appearance_is_saved_and_reduced_motion_overrides_animation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fiote-appearance.json");
    assert_eq!(read_preferences(&path).unwrap(), Preferences::default());
    let settings = Preferences {
        appearance: Appearance::Animated,
        paused: false,
        reduced_motion: false,
    };
    save_preferences(&path, settings).unwrap();
    assert_eq!(read_preferences(&path).unwrap(), settings);
    assert!(settings.animated());
    assert!(
        !Preferences {
            paused: true,
            ..settings
        }
        .animated()
    );
    assert!(
        !Preferences {
            reduced_motion: true,
            ..settings
        }
        .animated()
    );
    std::fs::write(&path, b"{\"appearance\":\"unknown\"}").unwrap();
    assert!(read_preferences(&path).is_err());
    std::fs::write(&path, vec![b' '; 4097]).unwrap();
    assert!(read_preferences(&path).is_err());
}

#[test]
fn static_avatars_share_the_scaled_image_without_loading_3d() {
    let mut world = World::new();
    world.insert_resource(Character::default());
    let parent = world.spawn(Node::default()).id();
    let first = super::super::spawn(&mut world, parent);
    let second = super::super::spawn(&mut world, parent);
    update(&mut world);
    let images: Vec<_> = world
        .query_filtered::<&ImageNode, With<Avatar>>()
        .iter(&world)
        .map(|image| image.image.clone())
        .collect();
    assert_eq!(images.len(), 2);
    assert_eq!(images[0], images[1]);
    assert!(world.resource::<Character>().loading.is_none());
    assert!(world.resource::<Character>().scenes.is_empty());
    assert!(world.get_resource::<AssetServer>().is_none());
    let first_bubble = world.get::<super::super::Fiote>(first).unwrap().bubble;
    let second_bubble = world.get::<super::super::Fiote>(second).unwrap().bubble;
    world.insert_resource(UiScale(3.0));
    update(&mut world);
    assert_eq!(
        world.resource::<Character>().portraits[&Palette::default()].0,
        256
    );
    assert_eq!(
        world.get::<super::super::Fiote>(first).unwrap().bubble,
        first_bubble
    );
    assert_eq!(
        world.get::<super::super::Fiote>(second).unwrap().bubble,
        second_bubble
    );
}

#[test]
fn svg_has_transparency_and_keeps_its_silhouette_at_small_sizes() {
    for edge in [64, 128, 256, 512] {
        let image = rasterize(edge, Palette::default()).unwrap();
        let pixels = image.data.unwrap();
        assert_eq!(pixels.len(), (edge * edge * 4) as usize);
        assert_eq!(pixels[3], 0);
        let opaque = pixels.chunks_exact(4).filter(|rgba| rgba[3] > 200).count();
        assert!(opaque > (edge * edge / 4) as usize);
        assert!(opaque < (edge * edge * 3 / 4) as usize);
    }
}

#[test]
fn packaged_model_has_skinning_and_every_named_behavior_clip() {
    assert_eq!(&GLB[..4], b"glTF");
    assert_eq!(u32::from_le_bytes(GLB[4..8].try_into().unwrap()), 2);
    assert_eq!(
        u32::from_le_bytes(GLB[8..12].try_into().unwrap()) as usize,
        GLB.len()
    );
    let length = u32::from_le_bytes(GLB[12..16].try_into().unwrap()) as usize;
    let json: serde_json::Value = serde_json::from_slice(&GLB[20..20 + length]).unwrap();
    assert!(!json["skins"].as_array().unwrap().is_empty());
    let animations = json["animations"].as_array().unwrap();
    for activity in Activity::ALL {
        let clip = animations
            .iter()
            .find(|clip| clip["name"] == activity.clip())
            .unwrap_or_else(|| panic!("Missing {}", activity.clip()));
        assert!(!clip["channels"].as_array().unwrap().is_empty());
    }
    for mesh in json["meshes"].as_array().unwrap() {
        for primitive in mesh["primitives"].as_array().unwrap() {
            assert!(primitive["attributes"]["JOINTS_0"].is_number());
            assert!(primitive["attributes"]["WEIGHTS_0"].is_number());
        }
    }
    assert!(GLB.len() < 4_000_000);
    let materials = json["materials"].as_array().unwrap();
    assert_eq!(
        materials
            .iter()
            .filter(|material| material["name"]
                .as_str()
                .is_some_and(|name| name.starts_with("Motion:")))
            .count(),
        8
    );
    assert!(!json["images"].as_array().unwrap().is_empty());
    assert!(
        json["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["extras"]["fiote_walk_stride"] == 0.18)
    );
}

#[test]
fn theme_and_local_overrides_color_each_avatar_without_changing_the_bubble() {
    use crate::native::tokens::{
        ColorScheme, SandStyleKind, ThemeSettings, Token, TokenOverrides, TokenValue,
    };
    let mut world = World::new();
    world.insert_resource(Character::default());
    let parent = world.spawn(Node::default()).id();
    let first = super::super::spawn(&mut world, parent);
    let second = super::super::spawn(&mut world, parent);
    let bubble = world.get::<super::super::Fiote>(first).unwrap().bubble;
    let original = Palette::resolve(&world, first);
    assert_eq!(original, Palette::default());
    let custom = TokenOverrides(std::collections::BTreeMap::from([
        (Token::SandBackground, TokenValue::Color([31, 70, 62, 255])),
        (Token::SandInk, TokenValue::Color([224, 240, 222, 255])),
        (Token::Accent, TokenValue::Color([211, 134, 44, 255])),
    ]));
    world
        .resource_mut::<ThemeSettings>()
        .kinds
        .insert(SandStyleKind::Fiote, custom.clone());
    let saved = serde_json::to_vec(world.resource::<ThemeSettings>()).unwrap();
    let restored: ThemeSettings = serde_json::from_slice(&saved).unwrap();
    assert!(restored.validate());
    world.insert_resource(restored);
    update(&mut world);
    assert_eq!(Palette::resolve(&world, first).fur, [31, 70, 62, 255]);
    assert_eq!(Palette::resolve(&world, second).accent, [211, 134, 44, 255]);
    crate::native::token_style::set_overrides(
        &mut world,
        first,
        TokenOverrides(std::collections::BTreeMap::from([(
            Token::SandBackground,
            TokenValue::Color([100, 30, 60, 255]),
        )])),
    );
    update(&mut world);
    assert_eq!(Palette::resolve(&world, first).fur, [100, 30, 60, 255]);
    assert_eq!(Palette::resolve(&world, second).fur, [31, 70, 62, 255]);
    assert_eq!(world.resource::<Character>().portraits.len(), 2);
    assert_eq!(
        world.get::<super::super::Fiote>(first).unwrap().bubble,
        bubble
    );
    crate::native::token_style::set_overrides(&mut world, first, TokenOverrides::default());
    world.resource_mut::<ThemeSettings>().kinds.clear();
    world.resource_mut::<ThemeSettings>().scheme = ColorScheme::Light;
    update(&mut world);
    assert_eq!(Palette::resolve(&world, first).fur, [248, 250, 252, 255]);
    assert_eq!(Palette::resolve(&world, first).light, [18, 18, 20, 255]);
    assert_eq!(world.resource::<Character>().portraits.len(), 1);
}

#[test]
fn static_palette_cache_drops_obsolete_styles_during_color_edits() {
    use crate::native::tokens::{Token, TokenOverrides, TokenValue};
    let mut world = World::new();
    world.insert_resource(Character::default());
    let parent = world.spawn(Node::default()).id();
    let owner = super::super::spawn(&mut world, parent);
    for shade in 0..32 {
        crate::native::token_style::set_overrides(
            &mut world,
            owner,
            TokenOverrides(std::collections::BTreeMap::from([(
                Token::SandBackground,
                TokenValue::Color([shade, 20, 30, 255]),
            )])),
        );
        update(&mut world);
        assert_eq!(world.resource::<Character>().portraits.len(), 1);
        assert!(world.resource::<Character>().scenes.is_empty());
    }
}

#[test]
fn motion_tint_is_bounded_and_fades_when_a_joint_stops() {
    let still = Transform::default();
    assert_eq!(palette::motion_strength(still, still, 1.0 / 30.0, 0.0), 0.0);
    let moved = Transform::from_translation(Vec3::new(0.03, 0.0, 0.0));
    let mut strength = palette::motion_strength(still, moved, 1.0 / 30.0, 0.0);
    assert!(strength > 0.0 && strength < 1.0);
    let mut material = StandardMaterial::default();
    Role::Motion.apply(&mut material, Palette::default(), strength);
    assert!(material.emissive.blue > material.emissive.red);
    for _ in 0..120 {
        strength = palette::motion_strength(moved, moved, 1.0 / 30.0, strength);
    }
    assert_eq!(strength, 0.0);
    Role::Motion.apply(&mut material, Palette::default(), strength);
    assert_eq!(material.base_color, Color::srgba_u8(18, 18, 20, 255));
}

#[test]
fn themed_svg_renders_in_every_scheme_with_transparent_edges() {
    use crate::native::tokens::{ColorScheme, ThemeSettings};
    let mut world = World::new();
    let entity = world.spawn(Node::default()).id();
    for scheme in ColorScheme::ALL {
        world.insert_resource(ThemeSettings {
            scheme,
            ..default()
        });
        let palette = Palette::resolve(&world, entity);
        let image = rasterize(128, palette).unwrap();
        let pixels = image.data.unwrap();
        assert_eq!(pixels[3], 0);
        assert!(pixels.chunks_exact(4).any(|rgba| rgba[3] == 255));
        assert!(!palette.svg().contains("FIOTE_COLOR_"));
    }
}

#[test]
fn avatar_and_configuration_open_the_existing_customization_scopes() {
    use crate::native::{
        actions::Action,
        container::BoxRoot,
        customization::{CustomizationAction, Scope},
        edit_mode::EditModePlugin,
        theme::ThemePlugin,
        tokens::{ColorScheme, SandStyleKind, ThemeSettings},
        workspace::WorkspacePlugin,
    };
    let mut app = App::new();
    crate::native::laboratory::isolate(app.world_mut());
    app.init_resource::<Assets<Font>>()
        .add_plugins((ThemePlugin, WorkspacePlugin, EditModePlugin));
    let root = app.world_mut().spawn(BoxRoot).id();
    app.update();
    let owner = super::super::spawn(app.world_mut(), root);
    let bubble = app
        .world()
        .get::<super::super::Fiote>(owner)
        .unwrap()
        .bubble;
    let button = app
        .world_mut()
        .query_filtered::<&crate::native::actions::ActionButton, With<Avatar>>()
        .single(app.world())
        .unwrap()
        .clone();
    button.actions.apply(app.world_mut(), button.target);
    assert_eq!(app.world().get::<Scope>(root), Some(&Scope::Sand(owner)));
    CustomizationAction::ScopedScheme(Some(ColorScheme::Light)).apply(app.world_mut(), root);
    assert_eq!(
        Palette::resolve(app.world(), owner).fur,
        [248, 250, 252, 255]
    );
    assert_eq!(
        app.world()
            .get::<super::super::Fiote>(owner)
            .unwrap()
            .bubble,
        bubble
    );
    Customize(true).apply(app.world_mut(), owner);
    assert_eq!(
        app.world().get::<Scope>(root),
        Some(&Scope::Kind(SandStyleKind::Fiote))
    );
    CustomizationAction::ScopedScheme(Some(ColorScheme::Moss)).apply(app.world_mut(), root);
    assert!(
        app.world()
            .resource::<ThemeSettings>()
            .kinds
            .contains_key(&SandStyleKind::Fiote)
    );
}

#[test]
fn hidden_avatars_do_not_start_the_animated_asset_load() {
    let mut world = World::new();
    world.insert_resource(Character::default());
    let parent = world
        .spawn(Node {
            display: Display::None,
            ..default()
        })
        .id();
    let owner = super::super::spawn(&mut world, parent);
    set_preferences(
        &mut world,
        Preferences {
            appearance: Appearance::Animated,
            ..default()
        },
    );
    update(&mut world);
    assert!(!visible(&world, owner));
    assert!(world.resource::<Character>().loading.is_none());
    assert!(world.resource::<Character>().scenes.is_empty());
}

#[test]
fn failed_model_returns_to_static_and_a_user_choice_retries() {
    let mut world = World::new();
    world.insert_resource(Character::default());
    let parent = world.spawn(Node::default()).id();
    super::super::spawn(&mut world, parent);
    set_preferences(
        &mut world,
        Preferences {
            appearance: Appearance::Animated,
            ..default()
        },
    );
    failed(&mut world, "fixture model failure".into());
    update(&mut world);
    assert!(world.resource::<Character>().failure);
    assert!(world.resource::<Character>().scenes.is_empty());
    assert!(
        world
            .resource::<Character>()
            .error
            .as_ref()
            .unwrap()
            .contains("fixture model failure")
    );
    Choose(Appearance::Animated).apply(&mut world, parent);
    assert!(!world.resource::<Character>().failure);
    assert!(world.resource::<Character>().error.is_none());
    ToggleReducedMotion.apply(&mut world, parent);
    update(&mut world);
    assert!(!preferences(&world).animated());
}

#[test]
fn hidden_scene_pauses_its_player_and_static_mode_removes_its_entities() {
    let mut world = World::new();
    world.insert_resource(Character::default());
    let root = world
        .spawn((Transform::default(), Visibility::default()))
        .id();
    let model = world.spawn((Transform::default(), ChildOf(root))).id();
    let camera = world.spawn((Camera::default(), ChildOf(root))).id();
    let (graph, index) = AnimationGraph::from_clip(Handle::default());
    let mut player = AnimationPlayer::default();
    player.play(index).repeat();
    let entity = world.spawn((player, ChildOf(model))).id();
    world.init_resource::<Assets<AnimationGraph>>();
    let graph = world.resource_mut::<Assets<AnimationGraph>>().add(graph);
    world.resource_mut::<Character>().scenes.push(Scene {
        palette: Palette::default(),
        layer: LAYER,
        running: false,
        rendered: None,
        materials: Vec::new(),
        root,
        model,
        camera,
        image: Handle::default(),
        graph,
        clips: HashMap::from([("sit_idle", (index, 4.0))]),
        players: vec![entity],
        behavior: Behavior::default(),
    });
    animate(&mut world, false);
    assert!(world.get::<AnimationPlayer>(entity).unwrap().all_paused());
    assert!(!world.get::<Camera>(camera).unwrap().is_active);
    assert!(world.resource::<Character>().last.is_none());
    animate(&mut world, true);
    assert!(world.get::<AnimationPlayer>(entity).unwrap().all_paused());
    world.resource_mut::<Character>().last = Some(Instant::now() - FRAME);
    animate(&mut world, true);
    assert!(!world.get::<AnimationPlayer>(entity).unwrap().all_paused());
    assert!(!world.get::<Camera>(camera).unwrap().is_active);
    world.resource_mut::<Character>().scenes[0].rendered =
        Some(Instant::now() - Activity::Sit.frame_period());
    animate(&mut world, true);
    assert!(world.get::<Camera>(camera).unwrap().is_active);
    destroy_scene(&mut world);
    for entity in [root, model, camera, entity] {
        assert!(world.get_entity(entity).is_err());
    }
    assert!(world.resource::<Character>().scenes.is_empty());
}
