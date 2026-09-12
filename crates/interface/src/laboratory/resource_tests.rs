use super::resources::*;
use crate::{canvas::CanvasItem, workspace::WorkspaceMember};
use bevy::{ecs::entity_disabling::Disabled, math::DVec2, prelude::*, text::EditableText};

fn item() -> CanvasItem {
    CanvasItem {
        position: DVec2::ZERO,
        size: Vec2::splat(100.0),
    }
}

#[cfg_attr(test, test)]
fn resources_attribute_nested_sands_and_deduplicate_shared_images() {
    let mut world = World::new();
    crate::laboratory::isolate(&mut world);
    world.init_resource::<Assets<Image>>();
    let image = world.resource_mut::<Assets<Image>>().add(Image::default());
    let image_bytes = world
        .resource::<Assets<Image>>()
        .get(&image)
        .unwrap()
        .data
        .as_ref()
        .unwrap()
        .len();
    let workspace = world.spawn(crate::workspace::Workspaces::default()).id();
    let root = world
        .spawn((
            item(),
            Name::new("Document"),
            WorkspaceMember(1),
            ChildOf(workspace),
        ))
        .id();
    let text = world
        .spawn((Text::new("stale"), EditableText::new("é🙂"), ChildOf(root)))
        .id();
    world.spawn((ImageNode::new(image.clone()), ChildOf(root)));
    world.spawn((ImageNode::new(image.clone()), ChildOf(root)));
    world.spawn((
        crate::effect::HoverEvents,
        crate::area::RecordProperties(serde_json::json!({"title": "private"})),
        ChildOf(root),
    ));
    world
        .entity_mut(text)
        .insert(crate::record_view::RecordEditor {
            uid: "record".into(),
            confirmed: "old".into(),
            pending: Some(("request".into(), "new".into())),
            status: text,
        });
    let nested = world
        .spawn((
            item(),
            Name::new("Nested image"),
            ImageNode::new(image),
            ChildOf(root),
            Disabled,
        ))
        .id();
    world.spawn((item(), crate::inspection::InspectionExcluded));
    world.spawn((
        item(),
        crate::area::InfluenceArea::new(
            crate::area::AreaShape::Square,
            DVec2::ZERO,
            DVec2::splat(100.0),
        ),
    ));
    let snapshot = capture(&mut world);
    assert_eq!(snapshot.sands.len(), 2);
    assert_eq!(snapshot.unique_image_assets, 1);
    assert_eq!(snapshot.retained_image_bytes, image_bytes);
    let row = snapshot
        .sands
        .iter()
        .find(|row| row.entity == root.to_string())
        .unwrap();
    assert_eq!(row.name, "Document");
    assert_eq!(row.workspace, "Home");
    assert_eq!(row.entities, 5);
    assert_eq!(row.ui_nodes, 5);
    assert_eq!(row.text_bytes, "é🙂".len());
    assert_eq!(row.image_assets, 1);
    assert_eq!(row.retained_image_bytes, image_bytes);
    assert_eq!(row.event_sources, 1);
    assert_eq!(row.pending_writes, 1);
    assert_eq!(
        row.record_json_bytes,
        serde_json::json!({"title": "private"}).to_string().len()
    );
    let nested = snapshot
        .sands
        .iter()
        .find(|row| row.entity == nested.to_string())
        .unwrap();
    assert_eq!(nested.entities, 1);
    assert_eq!(nested.retained_image_bytes, image_bytes);
    assert!(nested.suspended);
    assert!(
        !serde_json::to_string(&snapshot)
            .unwrap()
            .contains("private")
    );
}

#[cfg_attr(test, test)]
fn missing_assets_report_loader_errors_and_clear_after_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        bevy::asset::AssetPlugin {
            file_path: directory.path().to_str().unwrap().into(),
            ..default()
        },
    ))
    .init_asset::<Font>()
    .init_asset_loader::<bevy::text::FontLoader>();
    let font: Handle<Font> = app.world().resource::<AssetServer>().load("missing.ttf");
    let root = app
        .world_mut()
        .spawn((
            item(),
            Text::new("Hello"),
            TextFont {
                font: bevy::text::FontSource::Handle(font.clone()),
                ..default()
            },
        ))
        .id();
    let started = std::time::Instant::now();
    loop {
        app.update();
        if matches!(
            app.world()
                .resource::<AssetServer>()
                .get_load_state(font.id()),
            Some(bevy::asset::LoadState::Failed(_))
        ) {
            break;
        }
        assert!(started.elapsed().as_secs() < 5, "Missing font did not fail");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let snapshot = capture(app.world_mut());
    let error = snapshot.sands[0]
        .startup
        .iter()
        .find(|issue| issue.failed)
        .unwrap();
    assert_eq!(error.entity, root.to_string());
    assert!(error.reason.contains("Cannot load font"));
    assert!(error.reason.contains("missing.ttf"));
    app.world_mut().entity_mut(root).remove::<TextFont>();
    assert!(capture(app.world_mut()).sands[0].startup.is_empty());
}

#[cfg_attr(test, test)]
fn failure_priority_sorting_and_paging_keep_every_sand_reachable() {
    let mut snapshot = ResourceSnapshot::default();
    for i in 0..10 {
        snapshot.sands.push(SandResources {
            entity: i.to_string(),
            name: format!("Sand {i}"),
            entities: i + 1,
            text_bytes: 10 - i,
            ..default()
        });
    }
    snapshot.sands[0].startup.push(StartupIssue {
        entity: "0".into(),
        failed: true,
        reason: "Missing content".into(),
    });
    let first = snapshot.lines(ResourceSort::Entities, 0);
    assert!(first[3].starts_with("Sand 0 ["));
    assert!(first[7].starts_with("Sand 9 ["));
    let second = snapshot.lines(ResourceSort::Entities, 1);
    assert!(second[3].starts_with("Sand 2 ["));
    assert!(second[7].starts_with("Sand 1 ["));
    assert_eq!(snapshot.lines(ResourceSort::Entities, 2), first);
    assert!(snapshot.lines(ResourceSort::Text, 0)[7].starts_with("Sand 1 ["));
    assert_eq!(snapshot.sands.len(), 10);
}

#[cfg_attr(test, test)]
fn headless_resources_do_not_invent_a_graphics_device() {
    let snapshot = capture(&mut World::new());
    assert_eq!(snapshot.graphics.name, None);
    assert_eq!(snapshot.graphics.backend, None);
    assert_eq!(snapshot.graphics.label(), "Headless: no graphics device");
    assert!(snapshot.sands.is_empty());
    assert!(snapshot.lines(ResourceSort::Entities, usize::MAX)[0].contains("page 1 / 1"));
}

crate::laboratory_cases! {
    resources_attribute_nested_sands_and_deduplicate_shared_images,
    missing_assets_report_loader_errors_and_clear_after_replacement,
    failure_priority_sorting_and_paging_keep_every_sand_reachable,
    headless_resources_do_not_invent_a_graphics_device,
}
