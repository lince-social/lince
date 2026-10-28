use super::*;
use crate::layout::{
    Rules,
    desktop::{Item, ItemKind},
};

struct Fixture {
    world: World,
    model: Workspace,
    entities: BTreeMap<PlacementId, Entity>,
    windows: [Entity; 3],
}

impl Fixture {
    fn new(mode: WorkspaceLayout) -> Self {
        let mut world = World::new();
        let mut model = Workspace::new(mode);
        let mut entities = BTreeMap::new();
        let windows = [[300.0, 200.0], [200.0, 100.0], [80.0, 60.0]].map(|size| {
            let index = entities.len();
            let id = PlacementId([index as u8 + 1; 16]);
            let mut layout = LayoutBox::new(Vec2::from_array(size));
            layout.id = id.0;
            let entity = world.spawn(layout).id();
            model
                .insert_inactive(Item {
                    id,
                    kind: ItemKind::Application,
                    rules: Rules::fixed(size),
                    free: Geometry {
                        origin: if index == 0 {
                            [-750.0, -500.0]
                        } else {
                            [0.0; 2]
                        },
                        size: size.map(f64::from),
                    },
                    free_width: None,
                    floating: index > 0,
                })
                .unwrap();
            entities.insert(id, entity);
            entity
        });
        world
            .entity_mut(windows[1])
            .insert(TransientParent(windows[0]));
        world
            .entity_mut(windows[2])
            .insert(TransientParent(windows[1]));
        Self {
            world,
            model,
            entities,
            windows,
        }
    }

    fn solve(&mut self, usable: Rectangle) -> Solution {
        let mut solution = self.model.solve(usable, usable).unwrap();
        place(
            &self.world,
            &self.entities,
            &mut self.model,
            &mut solution,
            usable,
        );
        solution
    }
}

fn rectangle(solution: &Solution, index: u8) -> Rectangle {
    solution
        .items
        .iter()
        .find(|item| item.id == PlacementId([index; 16]))
        .unwrap()
        .rectangle
}

#[test]
fn free_dialogs_stay_centered_in_canvas_coordinates_when_output_bounds_change() {
    let mut fixture = Fixture::new(WorkspaceLayout::Free);
    let id = PlacementId([2; 16]);
    let initial = fixture.model.items()[0].clone();
    for (origin, size) in [
        ([0.0; 2], [1000.0, 700.0]),
        ([-900.0, 48.0], [640.0, 600.0]),
    ] {
        let usable = Rectangle::new(origin, size).unwrap();
        fixture.model.focus(id, usable).unwrap();
        let solution = fixture.solve(usable);
        assert_eq!(rectangle(&solution, 1).origin(), [-750.0, -500.0]);
        assert_eq!(rectangle(&solution, 2).origin(), [-700.0, -450.0]);
        assert_eq!(rectangle(&solution, 3).origin(), [-640.0, -430.0]);
        assert_eq!(fixture.model.focused(), Some(id));
        assert_eq!(fixture.model.items()[0], initial);
    }
}

#[test]
fn free_dialog_signed_offsets_and_nested_preview_do_not_accumulate_viewport_coordinates() {
    let mut fixture = Fixture::new(WorkspaceLayout::Free);
    fixture
        .world
        .entity_mut(fixture.windows[1])
        .insert(Placement {
            parent: fixture.windows[0],
            offset: bevy::math::DVec2::new(37.0, -19.0),
        });
    fixture
        .world
        .entity_mut(fixture.windows[2])
        .insert(Placement {
            parent: fixture.windows[1],
            offset: bevy::math::DVec2::new(-20.0, 40.0),
        });
    let usable = Rectangle::new([64.0, 40.0], [1000.0, 640.0]).unwrap();
    for _ in 0..3 {
        let solution = fixture.solve(usable);
        assert_eq!(rectangle(&solution, 2).origin(), [-663.0, -469.0]);
        assert_eq!(rectangle(&solution, 3).origin(), [-623.0, -409.0]);
    }
    let before = fixture.model.clone();
    let mut preview = before.clone();
    let mut solution = preview.solve(usable, usable).unwrap();
    place_with(
        &fixture.world,
        &fixture.entities,
        &mut preview,
        &mut solution,
        usable,
        Some((
            fixture.windows[1],
            Placement {
                parent: fixture.windows[0],
                offset: bevy::math::DVec2::new(-44.0, 15.0),
            },
        )),
    );
    assert_eq!(rectangle(&solution, 2).origin(), [-744.0, -435.0]);
    assert_eq!(rectangle(&solution, 3).origin(), [-704.0, -375.0]);
    assert_eq!(fixture.model, before);
}

#[test]
fn tiled_maximized_and_fullscreen_dialogs_retain_their_output_space_limits() {
    let usable = Rectangle::new([64.0, 40.0], [1000.0, 640.0]).unwrap();
    for state in 0..3 {
        let mut fixture = Fixture::new(if state == 0 {
            WorkspaceLayout::Tiled
        } else {
            WorkspaceLayout::Free
        });
        if state != 0 {
            assert!(fixture.model.request_state(
                PlacementId([1; 16]),
                crate::host::WindowRequest {
                    maximized: (state == 1).then_some(true),
                    fullscreen: (state == 2).then_some(true),
                    ..Default::default()
                }
            ));
        }
        let solution = fixture.solve(usable);
        for id in [2, 3] {
            let rectangle = rectangle(&solution, id);
            for axis in 0..2 {
                assert!(rectangle.origin()[axis] >= usable.origin()[axis]);
                assert!(
                    rectangle.origin()[axis] + rectangle.size()[axis]
                        <= usable.origin()[axis] + usable.size()[axis]
                );
            }
        }
    }
}
