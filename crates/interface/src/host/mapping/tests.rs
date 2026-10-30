use super::*;

fn output(physical: [u32; 2], rotation: QuarterTurn, scale: f64) -> OutputViewport {
    let size = rotation.source_size(physical).map(|size| f64::from(size) / scale);
    let bounds = Rectangle::new([1000.0, 50.0], size).unwrap();
    OutputViewport { rotation, id: OutputId(1), bounds, usable: Some(bounds), physical_size: physical,
        raster: RasterMapping { origin: bounds.origin(), scale: [scale; 2] }, workspace: 1 }
}

#[test]
fn the_identity_mapping_keeps_nominal_density_and_independent_asymmetric_targets_match_all_quarter_turns() {
    for (rotation, physical, expected) in [
        (QuarterTurn::Normal, [1200, 800], [100.0, 200.0]),
        (QuarterTurn::Clockwise90, [800, 1200], [600.0, 100.0]),
        (QuarterTurn::UpsideDown, [1200, 800], [1100.0, 600.0]),
        (QuarterTurn::Clockwise270, [800, 1200], [200.0, 1100.0]),
    ] {
        let output = output(physical, rotation, 1.0);
        let mapping = Mapping::new(&output, rotation, 1.0, [1100.0, 250.0], 1).unwrap();
        assert_eq!(mapping.logical_to_physical([1100.0, 250.0]), Some(expected));
        assert_eq!(mapping.physical_to_logical(expected), Some([1100.0, 250.0]));
        assert_eq!(mapping.map_rectangle(output.bounds).unwrap().size(), physical.map(f64::from));
        assert_eq!(mapping.raster, output.raster);
    }
    let mut rounded = output([1000, 720], QuarterTurn::Normal, 1.5);
    rounded.bounds = Rectangle::new(rounded.bounds.origin(), rounded.bounds.size().map(f64::round)).unwrap();
    rounded.usable = Some(rounded.bounds);
    let mapping = Mapping::new(&rounded, QuarterTurn::Normal, 1.0, [1100.0, 250.0], 1).unwrap();
    assert_eq!(mapping.logical_bounds(), rounded.bounds);
    assert_eq!(mapping.raster, rounded.raster);
    assert_eq!(mapping.visible_rectangle(rounded.bounds).unwrap().size(), [1000.0, 720.0]);
    assert_eq!(mapping.logical_to_physical([1100.0, 250.0]), Some([150.0, 300.0]));
    assert_eq!(mapping.physical_to_logical([150.0, 300.0]), Some([1100.0, 250.0]));
    rounded.bounds = Rectangle::new(rounded.bounds.origin(), [668.0, 480.0]).unwrap();
    assert!(Mapping::new(&rounded, QuarterTurn::Normal, 1.0, [1100.0, 250.0], 1).is_err());

}

#[test]
fn magnification_clamps_the_viewport_and_preserves_the_inverse_without_changing_the_nominal_raster() {
    for factor in [1.0, 2.0, 4.0] {
        for scale in [1.0, 1.25, 2.0] {
            let output = output([1200, 800], QuarterTurn::Normal, scale);
            let original = output.clone();
            for anchor in [output.bounds.origin(), [1000.0 + 600.0 / scale, 50.0 + 400.0 / scale], [1000.0 + 1200.0 / scale, 50.0 + 800.0 / scale]] {
                let mapping = Mapping::new(&output, QuarterTurn::Normal, factor, anchor, 4).unwrap();
                for point in [[0.0, 0.0], [123.5, 412.25], [1200.0, 800.0]] {
                    let logical = mapping.physical_to_logical(point).unwrap();
                    assert!((0..2).all(|axis| logical[axis] >= output.bounds.origin()[axis] - 1e-9
                        && logical[axis] <= output.bounds.origin()[axis] + output.bounds.size()[axis] + 1e-9));
                    let restored = mapping.logical_to_physical(logical).unwrap();
                    assert!((0..2).all(|axis| (restored[axis] - point[axis]).abs() < 1e-9));
                }
                assert_eq!(output, original);
            }
        }
    }
}

#[test]
fn oriented_logical_input_keeps_its_axes_while_physical_rotation_happens_once_after_magnification() {
    for (rotation, physical, expected) in [
        (QuarterTurn::Normal, [1200, 800], [300.0, 200.0]),
        (QuarterTurn::Clockwise90, [800, 1200], [600.0, 300.0]),
        (QuarterTurn::UpsideDown, [1200, 800], [900.0, 600.0]),
        (QuarterTurn::Clockwise270, [800, 1200], [200.0, 900.0]),
    ] {
        let viewport = output(physical, rotation, 1.25);
        let mapping = Mapping::new(&viewport, rotation, 2.0, [1480.0, 370.0], 1).unwrap();
        let source = [1360.0, 290.0];
        assert_eq!(viewport.source_size(), [1200, 800]);
        assert_eq!(mapping.logical_to_physical(source), Some(expected));
        assert_eq!(mapping.physical_to_logical(expected), Some(source));
        assert_eq!(mapping.logical_to_displayed(source), Some([1240.0, 210.0]));
        assert_eq!(mapping.displayed_to_logical([1240.0, 210.0]), Some(source));
        assert_eq!(mapping.composition_raster(viewport.raster).unwrap().point(source), [300.0, 200.0]);
        let displayed = mapping.visible_displayed_rectangle(Rectangle::new([1360.0, 290.0], [10.0, 20.0]).unwrap()).unwrap();
        assert_eq!(displayed.origin(), [1240.0, 210.0]);
        assert_eq!(displayed.size(), [20.0, 40.0]);
    }
}

#[test]
fn malformed_and_nonfinite_candidates_are_rejected_before_publication() {
    let output = output([1200, 800], QuarterTurn::Normal, 1.0);
    for factor in [0.0, 0.5, 4.1, f64::NAN, f64::INFINITY] {
        assert!(Mapping::new(&output, QuarterTurn::Normal, factor, [1100.0, 150.0], 1).is_err());
    }
    assert!(Mapping::new(&output, QuarterTurn::Normal, 1.0, [f64::NAN, 0.0], 1).is_err());
    assert!(Mapping::new(&output, QuarterTurn::Normal, 1.0, [1100.0, 150.0], 0).is_err());
    assert!(Mapping::new(&output, QuarterTurn::Clockwise90, 1.0, [1100.0, 150.0], 1).is_err());
}

#[test]
fn snapshots_are_atomic_bounded_and_retain_unchanged_output_revisions_without_accepting_future_mappings() {
    let viewport = output([1200, 800], QuarterTurn::Normal, 1.0);
    let first = Mapping::new(&viewport, QuarterTurn::Normal, 2.0, [1200.0, 200.0], 7).unwrap();
    let second = Mapping::new(&viewport, QuarterTurn::Normal, 2.0, [1200.0, 200.0], 8).unwrap();
    assert!(Snapshot::new(0, []).is_err());
    assert!(Snapshot::new(7, [second]).is_err());
    assert!(Snapshot::new(7, [first.clone(), first.clone()]).is_err());
    let snapshot = Snapshot::new(7, [first.clone()]).unwrap();
    assert_eq!(snapshot.revision(), 7);
    assert_eq!(snapshot.outputs().len(), 1);
    assert_eq!(snapshot.get(viewport.id), Some(&first));
    let retained = Snapshot::new(8, [first.clone()]).unwrap();
    assert_eq!(retained.get(viewport.id).unwrap().revision(), 7);
    let candidates = (1..=17).map(|id| {
        let mut viewport = viewport.clone();
        viewport.id = OutputId(id);
        Mapping::new(&viewport, QuarterTurn::Normal, 1.0, viewport.bounds.origin(), 9).unwrap()
    }).collect::<Vec<_>>();
    assert!(Snapshot::new(9, candidates).is_err());
    assert_eq!(snapshot.get(viewport.id).unwrap().magnification(), 2.0);
    assert_eq!(Snapshot::new(8, []).unwrap().outputs().len(), 0);
}

#[test]
fn composition_and_display_input_use_the_same_final_mapping_without_changing_nominal_density() {
    let output = output([1200, 800], QuarterTurn::Normal, 1.25);
    let mapping = Mapping::new(&output, QuarterTurn::Normal, 2.0, [1480.0, 370.0], 1).unwrap();
    let source = [1360.0, 290.0];
    let displayed = [1240.0, 210.0];
    assert_eq!(mapping.logical_to_physical(source), Some([300.0, 200.0]));
    assert_eq!(mapping.logical_to_displayed(source), Some(displayed));
    assert_eq!(mapping.displayed_to_logical(displayed), Some(source));
    assert_eq!(mapping.composition_raster(output.raster).unwrap().point(source), [300.0, 200.0]);
    assert_eq!(mapping.composition_raster(RasterMapping::default()).unwrap().point(source), displayed);
    assert_eq!(mapping.raster(), output.raster);
    let identity = Mapping::new(&output, QuarterTurn::Normal, 1.0, source, 2).unwrap();
    assert_eq!(identity.composition_raster(output.raster), Some(output.raster));
    assert_eq!(mapping.visible_rectangle(output.bounds).unwrap().size(), [1200.0, 800.0]);
    assert!(mapping.visible_rectangle(Rectangle::new(output.bounds.origin(), [10.0; 2]).unwrap()).is_none());
}
