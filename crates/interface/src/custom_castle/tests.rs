use super::*;
use crate::{
    actions::Action,
    canvas::CanvasView,
    edit_mode::{EditAction, EditModePlugin},
    workspace::{WorkspaceFile, WorkspacePlugin},
};

fn fixture(path: &std::path::Path) -> (App, Entity) {
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .insert_resource(WorkspaceFile::new(path.join("interface.json")))
        .add_plugins((WorkspacePlugin, EditModePlugin));
    let root = app.world_mut().spawn(crate::container::BoxRoot).id();
    app.update();
    (app, root)
}

fn sand(world: &mut World, root: Entity, text: &str, x: f64) -> Entity {
    crate::sand_store::spawn_sand(
        world,
        root,
        1,
        SandKind::EditableText,
        text,
        DVec2::new(x, 30.0),
    )
}

#[test]
fn mixed_castle_files_restore_independent_layouts_and_live_area_connections() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture(directory.path());
    let world = app.world_mut();
    let note = sand(world, root, "Plan for this month", 100.0);
    let mut area = InfluenceArea::new(
        crate::area::AreaShape::Square,
        DVec2::new(500.0, 30.0),
        DVec2::splat(600.0),
    );
    area.protein = Some(crate::protein_area::Config {
        enabled: true,
        calendar_dates: true,
        ..default()
    });
    let original_area_id = area.id.clone();
    let area_entity = crate::area::spawn_area(world, root, 1, area).unwrap();
    let calendar = crate::calendar::spawn(
        world,
        root,
        1,
        DVec2::new(800.0, 30.0),
        Calendar {
            area: Some(original_area_id.clone()),
            ..default()
        },
    );
    let protein = crate::protein_castle::spawn(
        world,
        root,
        1,
        DVec2::new(-500.0, 30.0),
        ProteinDraft::default(),
    );
    let parent_layout = LayoutBox::new(Vec2::new(1000.0, 800.0));
    let mut child_layout = LayoutBox::new(Vec2::new(248.0, 184.0));
    child_layout.parent = Some(parent_layout.id);
    world.entity_mut(calendar).insert(parent_layout);
    world.entity_mut(note).insert(child_layout);
    let original_group = SandGroup([7; 16]);
    for entity in [note, calendar, protein] {
        world.entity_mut(entity).insert((
            original_group,
            crate::scoped_events::EventBoundary(vec![crate::calendar::DATE_SELECTED.into()]),
        ));
    }
    world.entity_mut(root).insert(SandSelection(vec![calendar]));
    let saved = CustomCastle::capture(world, root, "Planning").unwrap();
    assert_eq!(saved.parts.len(), 4);
    let library = storage::directory(world).unwrap();
    let path = storage::save(&library, &saved).unwrap();
    let filename = path.file_name().unwrap().to_str().unwrap();
    let reloaded = storage::load(&library, filename).unwrap();
    let note_offset = reloaded
        .parts
        .iter()
        .find(|p| matches!(p.content, Content::Sand { .. }))
        .unwrap()
        .position;
    world.entity_mut(root).insert(CanvasView {
        center: DVec2::new(3000.0, 1000.0),
        zoom: 1.0,
    });
    let first = reloaded.spawn(world, root).unwrap();
    let second = reloaded.spawn(world, root).unwrap();
    let group_a = *world.get::<SandGroup>(first[0]).unwrap();
    let group_b = *world.get::<SandGroup>(second[0]).unwrap();
    assert_ne!(group_a, group_b);
    assert_ne!(group_a, original_group);
    for copy in [&first, &second] {
        let note = *copy
            .iter()
            .find(|e| world.get::<StoredSand>(**e).is_some())
            .unwrap();
        assert_eq!(
            crate::sand_text::snapshot(world, note)[0].text,
            "Plan for this month"
        );
        assert_eq!(
            world.get::<CanvasItem>(note).unwrap().position,
            DVec2::new(3000.0, 1000.0) + DVec2::from_array(note_offset)
        );
        let calendar = *copy
            .iter()
            .find(|e| world.get::<CalendarSand>(**e).is_some())
            .unwrap();
        let area = copy
            .iter()
            .find_map(|e| world.get::<InfluenceArea>(*e))
            .unwrap();
        assert_ne!(area.id, original_area_id);
        assert_eq!(
            world
                .get::<CalendarSand>(calendar)
                .unwrap()
                .0
                .area
                .as_deref(),
            Some(area.id.as_str())
        );
        assert!(area.protein.as_ref().unwrap().enabled);
        assert_eq!(
            world.get::<LayoutBox>(note).unwrap().parent,
            Some(world.get::<LayoutBox>(calendar).unwrap().id)
        );
        assert_ne!(
            world.get::<LayoutBox>(calendar).unwrap().id,
            parent_layout.id
        );
        assert_eq!(
            world
                .get::<crate::scoped_events::EventBoundary>(calendar)
                .unwrap()
                .0,
            vec![crate::calendar::DATE_SELECTED]
        );
    }
    assert_eq!(
        world.get::<InfluenceArea>(area_entity).unwrap().id,
        original_area_id
    );
    world.entity_mut(root).insert(SandSelection(first));
    assert_eq!(
        CustomCastle::capture(world, root, "Composed again")
            .unwrap()
            .parts
            .len(),
        4
    );
    app.world_mut().write_message(AppExit::Success);
    app.update();
    let (mut restarted, new_root) = fixture(directory.path());
    let restored = restarted.world_mut();
    let calendars: Vec<_> = restored
        .query::<&CalendarSand>()
        .iter(restored)
        .map(|c| c.0.area.clone().unwrap())
        .collect();
    let area_ids: HashSet<_> = restored
        .query::<&InfluenceArea>()
        .iter(restored)
        .map(|a| a.id.clone())
        .collect();
    assert_eq!(calendars.len(), 3);
    assert!(calendars.iter().all(|id| area_ids.contains(id)));
    assert_eq!(storage::entries(&library).0[0].1, "Planning");
    assert_eq!(
        storage::load(&library, filename)
            .unwrap()
            .spawn(restarted.world_mut(), new_root)
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn picker_separates_builtins_and_custom_and_saves_through_controls() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture(directory.path());
    let world = app.world_mut();
    let a = sand(world, root, "First", 0.0);
    let b = sand(world, root, "Second", 300.0);
    world.entity_mut(root).insert(SandSelection(vec![a, b]));
    EditAction::Open.apply(world, root);
    EditAction::Store.apply(world, root);
    let texts: Vec<_> = world
        .query::<&Text>()
        .iter(world)
        .map(|t| t.0.clone())
        .collect();
    let sections: Vec<_> = texts
        .iter()
        .filter(|t| matches!(t.as_str(), "Sands" | "Castles" | "Custom"))
        .collect();
    assert_eq!(sections, vec!["Sands", "Castles", "Custom"]);
    assert!(
        world
            .query::<&crate::icons::IconButton>()
            .iter(world)
            .any(|i| i.icon == crate::icons::Icon::Info
                && i.label.contains("Back up these files")
                && i.label.contains("castles"))
    );
    let field = world
        .query::<(Entity, &bevy::text::EditableText)>()
        .iter(world)
        .find(|(_, e)| e.max_characters == Some(80))
        .unwrap()
        .0;
    world
        .get_mut::<bevy::text::EditableText>(field)
        .unwrap()
        .editor
        .set_text("My pair");
    let save = world
        .query::<(Entity, &crate::icons::IconButton)>()
        .iter(world)
        .find(|(_, i)| i.label == "Save selected group as a custom Castle")
        .unwrap()
        .0;
    world.trigger(bevy::ui_widgets::Activate { entity: save });
    app.update();
    let world = app.world_mut();
    let library = storage::directory(world).unwrap();
    assert_eq!(storage::entries(&library).0[0].1, "My pair");
    let add = world
        .query::<(Entity, &crate::icons::IconButton)>()
        .iter(world)
        .find(|(_, i)| i.label == "Add My pair at the camera")
        .unwrap()
        .0;
    world.trigger(bevy::ui_widgets::Activate { entity: add });
    app.update();
    let world = app.world_mut();
    let copied = crate::canvas_selection::selected(world, root);
    assert_eq!(copied.len(), 2);
    assert!(!copied.contains(&a) && !copied.contains(&b));
    assert_eq!(
        world.get::<SandGroup>(copied[0]),
        world.get::<SandGroup>(copied[1])
    );
}

#[test]
fn invalid_files_are_reported_without_losing_valid_entries_or_spawning_parts() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture(directory.path());
    let world = app.world_mut();
    let a = sand(world, root, "Safe", 0.0);
    world.entity_mut(root).insert(SandSelection(vec![a]));
    let mut castle = CustomCastle::capture(world, root, "Safe").unwrap();
    let library = storage::directory(world).unwrap();
    let path = storage::save(&library, &castle).unwrap();
    let another = storage::save(&library, &castle).unwrap();
    assert_ne!(path, another);
    std::fs::write(library.join("broken.json"), b"{").unwrap();
    assert_eq!(storage::entries(&library).0.len(), 2);
    assert_eq!(storage::entries(&library).1.len(), 1);
    assert!(storage::load(&library, "../interface.json").is_err());
    let before = world.entities().len();
    castle.parts[0].size[0] = -1.0;
    assert!(castle.spawn(world, root).is_err());
    assert_eq!(world.entities().len(), before);
    assert!(storage::save(&library, &castle).is_err());
    let mut boundary = CustomCastle::capture(world, root, "Extreme camera").unwrap();
    let mut area = InfluenceArea::new(
        crate::area::AreaShape::Square,
        DVec2::ZERO,
        DVec2::splat(100.0),
    );
    area.target = crate::area::AttractionTarget::Point([f64::MAX, 0.0]);
    boundary.parts.push(Part {
        position: [0.0; 2],
        size: [100.0; 2],
        placement: Placement::default(),
        tokens: TokenOverrides::default(),
        content: Content::Area(area),
    });
    assert!(boundary.valid());
    world.entity_mut(root).insert(CanvasView {
        center: DVec2::splat(f64::MAX),
        zoom: 1.0,
    });
    assert!(boundary.spawn(world, root).is_err());
    assert_eq!(world.entities().len(), before);
    let file = std::fs::File::create(library.join("oversized.json")).unwrap();
    file.set_len(5 * 1024 * 1024).unwrap();
    assert!(storage::load(&library, "oversized.json").is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&path, library.join("link.json")).unwrap();
        assert!(storage::load(&library, "link.json").is_err());
    }
}

#[test]
fn capture_includes_layout_children_and_rejects_partial_or_cyclic_compositions() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture(directory.path());
    let world = app.world_mut();
    let parent = sand(world, root, "Parent", 0.0);
    let child = sand(world, root, "Child", 300.0);
    let parent_layout = LayoutBox::new(Vec2::splat(500.0));
    let mut child_layout = LayoutBox::new(Vec2::splat(200.0));
    child_layout.parent = Some(parent_layout.id);
    world.entity_mut(parent).insert(parent_layout);
    world.entity_mut(child).insert(child_layout);
    world.entity_mut(root).insert(SandSelection(vec![parent]));
    let mut castle = CustomCastle::capture(world, root, "Layout").unwrap();
    assert_eq!(castle.parts.len(), 2);
    castle.parts[0].placement.layout.as_mut().unwrap().parent = Some(child_layout.id);
    assert!(!castle.valid());
    let unknown = world
        .spawn((
            CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::splat(100.0),
            },
            ChildOf(root),
            WorkspaceMember(1),
        ))
        .id();
    world
        .entity_mut(root)
        .insert(SandSelection(vec![parent, unknown]));
    assert!(CustomCastle::capture(world, root, "No partial file").is_err());
    world.entity_mut(root).insert(SandSelection(vec![parent]));
    assert!(CustomCastle::capture(world, root, " ").is_err());
    castle.parts = vec![castle.parts[1].clone(); MAX_PARTS + 1];
    assert!(!castle.valid());
}

#[test]
fn instinct_castles_keep_their_page_when_saved_and_placed_again() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture(directory.path());
    let world = app.world_mut();
    let reader = crate::instinct::spawn(
        world,
        root,
        1,
        DVec2::ZERO,
        crate::instinct::Instinct { page: Some("tool".into()) },
    );
    world.entity_mut(root).insert(SandSelection(vec![reader]));
    let castle = CustomCastle::capture(world, root, "Reading").unwrap();
    let encoded = serde_json::to_string(&castle).unwrap();
    let castle: CustomCastle = serde_json::from_str(&encoded).unwrap();
    let copies = castle.spawn(world, root).unwrap();
    assert_eq!(copies.len(), 1);
    assert_eq!(world.get::<crate::instinct::Instinct>(copies[0]).unwrap().page.as_deref(), Some("tool"));
    assert_ne!(reader, copies[0]);
}
