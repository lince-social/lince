use super::*;
use lince_interface::host::mapping::QuarterTurn;
use lince_interface::host::{OutputId, raster::RasterMapping};

fn viewport() -> OutputViewport {
    let bounds = Rectangle::new([1000.0, 40.0], [960.0, 640.0]).unwrap();
    OutputViewport { rotation: Default::default(), id: OutputId(1), bounds, usable: Some(bounds), physical_size: [1200, 800], raster: RasterMapping { origin: bounds.origin(), scale: [1.25; 2] }, workspace: 1 }
}

#[test]
fn an_existing_magnified_view_does_not_chase_a_pointer_inside_its_safe_region() {
    let output = viewport();
    let mapping = Mapping::new(&output, QuarterTurn::Normal, 2.0, [1480.0, 360.0], 1).unwrap();
    let center = anchor(&output, 2.0, Some(&mapping), Some([1500.0, 370.0]));
    let next = Mapping::new(&output, QuarterTurn::Normal, 2.0, center, 2).unwrap();
    assert!(mapping.same_mapping(&next));
    let center = anchor(&output, 2.0, Some(&mapping), Some([1720.0, 360.0]));
    let next = Mapping::new(&output, QuarterTurn::Normal, 2.0, center, 2).unwrap();
    assert!(!mapping.same_mapping(&next));
    let point = next.logical_to_physical([1720.0, 360.0]).unwrap();
    assert!((point[0] - 1020.0).abs() < 1e-9);
    assert_eq!(point[1], 400.0);
}

#[test]
fn a_factor_change_anchors_once_and_reset_keeps_the_nominal_output_unchanged() {
    let output = viewport();
    let mapping = Mapping::new(&output, QuarterTurn::Normal, 2.0, [1480.0, 360.0], 1).unwrap();
    let target = [1500.0, 380.0];
    assert_eq!(anchor(&output, 4.0, Some(&mapping), Some(target)), target);
    let reset = Mapping::new(&output, QuarterTurn::Normal, 1.0, anchor(&output, 1.0, Some(&mapping), Some(target)), 2).unwrap();
    assert_eq!(reset.raster(), output.raster);
    assert_eq!(reset.logical_to_physical(output.bounds.origin()), Some([0.0; 2]));
    assert_eq!(reset.logical_to_physical([1960.0, 680.0]), Some([1200.0, 800.0]));
}

#[test]
fn unchanged_fractional_views_retain_their_exact_mapping_instead_of_reconstructing_a_drifting_inverse() {
    let output = viewport();
    let factor = 1.333333333333;
    let point = [1475.1, 380.7];
    let mapping = Mapping::new(&output, QuarterTurn::Normal, factor, point, 7).unwrap();
    for _ in 0..120 { assert!(retain(&output, factor, &mapping, Some(point))); }
    assert!(retain(&output, factor, &mapping, None));
    assert!(!retain(&output, 2.0, &mapping, Some(point)));
    assert_eq!(mapping.revision(), 7);
}

#[test]
fn keyboard_reveal_fits_a_whole_control_once_and_keeps_the_existing_view_when_already_visible() {
    let output = viewport();
    let old = Mapping::new(&output, QuarterTurn::Normal, 2.0, [1480.0, 360.0], 1).unwrap();
    let target = Rectangle::new([1740.0, 450.0], [60.0, 30.0]).unwrap();
    let center = reveal_anchor(&output, 2.0, Some(&old), target).unwrap();
    assert_eq!(center, [1584.0, 360.0]);
    let mapping = Mapping::new(&output, QuarterTurn::Normal, 2.0, center, 2).unwrap();
    let physical = mapping.map_rectangle(target).unwrap();
    assert_eq!(physical.origin(), [990.0, 625.0]);
    assert_eq!(physical.size(), [150.0, 75.0]);
    for _ in 0..120 { assert_eq!(reveal_anchor(&output, 2.0, Some(&mapping), target), None); }
    assert_eq!(mapping.raster(), output.raster);
    assert_eq!(old.revision(), 1);
}

#[test]
fn a_control_larger_than_the_lens_reveals_its_leading_edge_without_changing_world_geometry() {
    let output = viewport();
    let old = Mapping::new(&output, QuarterTurn::Normal, 2.0, [1480.0, 360.0], 1).unwrap();
    let target = Rectangle::new([1240.0, 200.0], [500.0, 360.0]).unwrap();
    let center = reveal_anchor(&output, 2.0, Some(&old), target).unwrap();
    assert_eq!(center, [1456.0, 344.0]);
    let mapping = Mapping::new(&output, QuarterTurn::Normal, 2.0, center, 2).unwrap();
    assert_eq!(mapping.logical_to_physical(target.origin()), Some([60.0, 40.0]));
    assert_eq!(mapping.logical_bounds(), output.bounds);
    assert!(compatible(&output, 2.0, &mapping));
    assert!(!compatible(&output, 1.0, &mapping));
}
