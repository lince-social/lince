use super::*;
use crate::actions::Action;

fn world() -> (World, Entity) {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::theme::Typography>();
    world.init_resource::<crate::tokens::ThemeSettings>();
    let root = world.spawn(crate::workspace::Workspaces::default()).id();
    (world, root)
}

#[test]
fn restores_two_books_positions_modes_zoom_and_placement() {
    let (mut world, root) = world();
    let mut document = DocumentViewer::with_path("/books/one.pdf");
    document.positions.insert(
        document.path.clone(),
        Position {
            section: 12,
            fraction: 0.625,
            mode: Mode::Pages,
        },
    );
    document.positions.insert(
        "/books/two.epub".into(),
        Position {
            section: 3,
            fraction: 0.375,
            mode: Mode::Scroll,
        },
    );
    document.zoom = 1.5;
    let owner = spawn(
        &mut world,
        root,
        1,
        DVec2::new(50.0, -20.0),
        document.clone(),
    );
    let saved = snapshot(&mut world, root).remove(0);
    assert!(saved.valid());
    let bytes = serde_json::to_vec(&saved).unwrap();
    world.despawn(owner);
    let saved: SavedDocumentViewer = serde_json::from_slice(&bytes).unwrap();
    saved.restore(&mut world, root);
    let (owner, restored, item) = world
        .query::<(Entity, &DocumentViewer, &crate::canvas::CanvasItem)>()
        .single(&world)
        .unwrap();
    assert_eq!(restored.positions, document.positions);
    assert_eq!(restored.zoom, 1.5);
    assert_eq!(item.position, DVec2::new(50.0, -20.0));
    ui::open(&mut world, owner, "/books/two.epub".into());
    assert_eq!(
        world
            .get::<DocumentViewer>(owner)
            .unwrap()
            .position()
            .section,
        3
    );
    ui::Control::Mode.apply(&mut world, owner);
    assert_eq!(
        world.get::<DocumentViewer>(owner).unwrap().position().mode,
        Mode::Pages
    );
    assert_eq!(
        world.get::<DocumentViewer>(owner).unwrap().positions["/books/one.pdf"].fraction,
        0.625
    );
}

#[test]
fn invalid_saved_positions_do_not_enter_workspace() {
    let mut state = DocumentViewer::with_path("/books/book.pdf");
    state.positions.insert(
        state.path.clone(),
        Position {
            fraction: f32::INFINITY,
            ..default()
        },
    );
    assert!(!state.valid());
    state.positions.clear();
    state.zoom = f32::NAN;
    assert!(!state.valid());
    state.zoom = 1.0;
    state.path = "bad\0path".into();
    assert!(!state.valid());
}
