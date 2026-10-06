use crate::{
    actions::ActionButton,
    canvas::CanvasItem,
    sand_store::{SandPreview, StoreComponent},
};
use bevy::{prelude::*, text::EditableText};

#[derive(Component)]
pub(crate) struct ComponentGallery;

#[derive(Resource)]
pub(crate) struct Fixtures {
    scene: World,
    root: Entity,
    entries: Vec<(Entity, StoreComponent)>,
    previews: std::collections::HashMap<Entity, Entity>,
}

pub(crate) fn count(world: &mut World) -> usize {
    if !world.contains_resource::<Fixtures>() {
        let (mut scene, root) = scene(world);
        let entries = entries(&mut scene);
        world.insert_resource(Fixtures {
            scene,
            root,
            entries,
            previews: default(),
        });
    }
    world.resource::<Fixtures>().entries.len()
}

pub(crate) fn scene(world: &mut World) -> (World, Entity) {
    let (mut scene, root) = crate::sand_store::preview::scene(world);
    let sands = scene.spawn(Node::default()).id();
    let castles = scene.spawn(Node::default()).id();
    crate::edit_mode::builtin_store_entries(&mut scene, root, sands, castles);
    if let Some(images) = scene.remove_resource::<Assets<Image>>() {
        world.init_resource::<Assets<Image>>();
        let handles: std::collections::HashMap<_, _> = images
            .iter()
            .map(|(id, image)| (id, world.resource_mut::<Assets<Image>>().add(image.clone())))
            .collect();
        for mut node in scene.query::<&mut ImageNode>().iter_mut(&mut scene) {
            if let Some(handle) = handles.get(&node.image.id()) {
                node.image = handle.clone();
            }
        }
    }

    (scene, root)
}

pub fn entries(world: &mut World) -> Vec<(Entity, StoreComponent)> {
    let mut entries: Vec<_> = world
        .query::<(Entity, &StoreComponent)>()
        .iter(world)
        .map(|(entity, component)| (entity, component.clone()))
        .collect();
    entries.sort_by(|a, b| a.1.title.cmp(&b.1.title));
    entries
}

pub fn descendants(world: &World, owner: Entity) -> Vec<Entity> {
    let mut result = Vec::new();
    let mut pending = vec![owner];
    while let Some(entity) = pending.pop() {
        result.push(entity);
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.iter());
        }
    }
    result
}

fn preview(scene: &mut World, root: Entity, entry: Entity) -> Entity {
    if let Some(preview) = descendants(scene, entry)
        .into_iter()
        .find(|entity| scene.get::<SandPreview>(*entity).is_some())
    {
        return scene.get::<Children>(preview).unwrap()[0];
    }
    let existing = descendants(scene, root);
    let action = scene.get::<ActionButton>(entry).unwrap().clone();
    action.actions.run(scene, action.target);
    let owner = scene
        .get::<Children>(root)
        .unwrap()
        .iter()
        .find(|entity| !existing.contains(entity) && scene.get::<CanvasItem>(*entity).is_some())
        .unwrap();
    owner
}

pub(crate) fn show(world: &mut World, root: Entity, index: usize) {
    let previous: Vec<_> = world
        .query_filtered::<Entity, With<ComponentGallery>>()
        .iter(world)
        .collect();
    for entity in previous {
        world.despawn(entity);
    }
    count(world);
    let mut fixtures = world.remove_resource::<Fixtures>().unwrap();
    let (entry, component) = fixtures.entries[index % fixtures.entries.len()].clone();
    let source = if let Some(source) = fixtures.previews.get(&entry) {
        *source
    } else {
        let source = preview(&mut fixtures.scene, fixtures.root, entry);
        fixtures.previews.insert(entry, source);
        source
    };
    let scene = &fixtures.scene;
    let size = scene
        .get::<CanvasItem>(source)
        .map_or(component.size, |item| item.size);
    let gallery = world
        .spawn((
            ComponentGallery,
            ChildOf(root),
            GlobalZIndex(45),
            crate::token_style::background(crate::tokens::Token::Surface),
            Node {
                position_type: PositionType::Absolute,
                top: percent(43),
                bottom: px(12),
                left: px(12),
                right: px(12),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12)),
                row_gap: px(8),
                overflow: Overflow::clip(),
                ..default()
            },
        ))
        .id();
    let controls = crate::sand_panel::row(world, gallery);
    for (title, action) in [
        (
            "Previous component",
            super::LaboratoryAction::PreviousComponent,
        ),
        ("Next component", super::LaboratoryAction::NextComponent),
    ] {
        crate::information::action_button(world, controls, root, title, crate::actions![action]);
    }
    crate::edit_mode::label(
        world,
        gallery,
        &format!(
            "{} / {} · {}",
            index % fixtures.entries.len() + 1,
            fixtures.entries.len(),
            component.title
        ),
        20.0,
    );
    crate::edit_mode::label(world, gallery, &component.description, 14.0);
    crate::edit_mode::label(
        world,
        gallery,
        "Appearance preview · Behavior checks use isolated data",
        12.0,
    );
    let viewport = world
        .get::<ComputedUiRenderTargetInfo>(root)
        .map(ComputedUiRenderTargetInfo::logical_size)
        .filter(|size| size.is_finite() && size.min_element() > 0.0)
        .unwrap_or_else(|| {
            world
                .query_filtered::<&Window, With<bevy::window::PrimaryWindow>>()
                .iter(world)
                .next()
                .map_or(Vec2::new(1000.0, 800.0), |window| window.resolution.size())
        });
    let scale = ((viewport.x - 56.0) / size.x).clamp(0.1, 1.0);
    let scroll = world
        .spawn((
            ChildOf(gallery),
            Node {
                width: percent(100),
                flex_grow: 1.0,
                min_height: px(0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
        ))
        .observe(
            |mut event: On<Pointer<bevy::picking::events::Scroll>>,
             mut scrolls: Query<&mut ScrollPosition>| {
                if let Ok(mut scroll) = scrolls.get_mut(event.entity) {
                    let scale = if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
                        24.0
                    } else {
                        1.0
                    };
                    scroll.0.y -= event.y * scale;
                    event.propagate(false);
                }
            },
        )
        .id();
    let container = world
        .spawn((
            ChildOf(scroll),
            Node {
                width: px(size.x * scale),
                height: px(size.y * scale),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    let miniature = crate::sand_store::preview::snapshot(scene, source, world, container);
    world.entity_mut(miniature).insert((
        Name::new("Laboratory component preview"),
        UiTransform::from_scale(Vec2::splat(scale)),
        Node {
            position_type: PositionType::Absolute,
            left: px((size.x * scale - size.x) * 0.5),
            top: px((size.y * scale - size.y) * 0.5),
            width: px(size.x),
            height: px(size.y),
            flex_direction: FlexDirection::Column,
            overflow: Overflow::clip(),
            ..scene.get::<Node>(source).unwrap().clone()
        },
    ));
    world.insert_resource(fixtures);
}

#[cfg_attr(test, test)]
fn every_builtin_store_component_has_a_safe_nonempty_preview() {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::theme::Typography>();
    world.init_resource::<crate::tokens::ThemeSettings>();
    let (mut scene, root) = scene(&mut world);
    let entries = entries(&mut scene);
    assert!(entries.len() >= crate::sand_store::SandKind::ALL.len() + 20);
    let mut titles = std::collections::HashSet::new();
    for (entry, component) in entries {
        assert!(
            titles.insert(component.title.clone()),
            "Duplicate: {}",
            component.title
        );
        assert!(
            component.size.is_finite() && component.size.min_element() > 0.0,
            "Invalid size: {}",
            component.title
        );
        let source = preview(&mut scene, root, entry);
        let target = world.spawn(Node::default()).id();
        let snapshot = crate::sand_store::preview::snapshot(&scene, source, &mut world, target);
        let nodes = descendants(&world, snapshot);
        for entity in &nodes {
            assert!(
                world.get::<ActionButton>(*entity).is_none(),
                "Live control in {} preview",
                component.title
            );
            assert!(world.get::<EditableText>(*entity).is_none());
            if component.title == "Command Castle"
                && let Some(node) = world.get::<Node>(*entity)
            {
                for border in [
                    node.border.left,
                    node.border.right,
                    node.border.top,
                    node.border.bottom,
                ] {
                    if let Val::Px(width) = border {
                        assert!(
                            width <= 2.0,
                            "Thumbnail border leaked into {}",
                            component.title
                        );
                    }
                }
            }
        }
        let text: Vec<_> = nodes
            .iter()
            .filter_map(|entity| world.get::<Text>(*entity))
            .map(|text| text.0.as_str())
            .collect();
        if component.title != "Square" {
            assert!(
                text.iter().any(|text| !text.is_empty()),
                "Empty preview: {}",
                component.title
            );
        }
        for expected in match component.title.as_str() {
            "Command Castle" => &["Run", "Bash script"][..],
            "Fiote Castle" => &["Manage Fiote"][..],
            _ => &[],
        } {
            assert!(
                text.contains(expected),
                "{} preview is missing {expected}",
                component.title
            );
        }
        world.despawn(target);
    }
}

crate::laboratory_cases! {
    every_builtin_store_component_has_a_safe_nonempty_preview,
}
