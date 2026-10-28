use super::*;
use lince_interface::host::{OutputId, OutputViewport, Rectangle, mapping::{Mapping, QuarterTurn, Snapshot}, raster::RasterMapping};

#[test]
fn exported_bounds_use_displayed_desktop_coordinates_and_clip_only_the_visible_view() {
    let mut world = World::new();
    let bounds = Rectangle::new([1000.0, 50.0], [960.0, 640.0]).unwrap();
    let output = OutputViewport { rotation: Default::default(), id: OutputId(1), bounds, usable: Some(bounds), physical_size: [1200, 800], raster: RasterMapping { origin: bounds.origin(), scale: [1.25; 2] }, workspace: 1 };
    let mapping = Mapping::new(&output, QuarterTurn::Normal, 2.0, [1480.0, 370.0], 1).unwrap();
    world.insert_resource(Snapshot::new(1, [mapping]).unwrap());
    let view = crate::native_input::projection::View { root: None, output: output.id, image: Handle::<Image>::default().id(), raster: output.raster, origin: output.bounds.origin(), viewport: Some(output) };
    let node = Rectangle::new([1360.0, 290.0], [40.0, 20.0]).unwrap();
    assert_eq!(display_bounds(&world, &view, Some(node)), Rectangle::new([1240.0, 210.0], [80.0, 40.0]));
    assert!(display_bounds(&world, &view, Rectangle::new(bounds.origin(), [10.0; 2])).is_none());
    assert_eq!(display_bounds(&world, &view, Some(bounds)), Some(bounds));
}
