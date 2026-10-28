use super::*;

fn source() -> (NodeId, Node) {
    let mut node = Node::new(Role::TextRun);
    node.set_value("aé界\n");
    node.set_character_lengths([1, 2, 3, 1]);
    node.set_character_positions([0.0, 8.0, 18.0, 34.0]);
    node.set_character_widths([8.0, 10.0, 16.0, 0.0]);
    node.set_word_starts([0]);
    node.set_text_direction(TextDirection::LeftToRight);
    node.set_bounds(accesskit::Rect::new(10.0, 20.0, 44.0, 40.0));
    (NodeId(40_000), node)
}

#[test]
fn character_rectangles_use_parleys_clusters_and_keep_rtl_and_line_break_geometry() {
    let (_, mut node) = source();
    assert_eq!(cluster_bounds(&node, 1), Some(accesskit::Rect::new(18.0, 20.0, 28.0, 40.0)));
    assert_eq!(cluster_bounds(&node, 2), Some(accesskit::Rect::new(28.0, 20.0, 44.0, 40.0)));
    assert_eq!(cluster_bounds(&node, 3), Some(accesskit::Rect::new(44.0, 20.0, 44.0, 40.0)));
    node.set_text_direction(TextDirection::RightToLeft);
    assert_eq!(cluster_bounds(&node, 1), Some(accesskit::Rect::new(26.0, 20.0, 36.0, 40.0)));
    node.set_character_widths([8.0, f32::NAN, 16.0, 0.0]);
    assert_eq!(cluster_bounds(&node, 1), None);
}

#[test]
fn exported_unicode_clusters_round_trip_to_the_original_field_selection_without_byte_indices() {
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    let foreign = world.spawn_empty().id();
    let image = Handle::<Image>::default().id();
    let view = crate::native_input::projection::View { root: None, output: lince_interface::host::OutputId(1), image,
        raster: Default::default(), origin: [0.0; 2], viewport: None };
    let (source_id, node) = source();
    let original = TextSelection {
        anchor: TextPosition { node: source_id, character_index: 1 },
        focus: TextPosition { node: source_id, character_index: 4 },
    };
    let mut field = Node::new(Role::TextInput);
    field.set_children([source_id]);
    field.set_text_selection(original);
    let mut nodes = Vec::new();
    let mut spans = BTreeMap::new();
    let mut identities = BTreeMap::new();
    let mut alive = BTreeSet::new();
    project(&world, &view, None, entity, &mut field, vec![(source_id, node.clone())], &mut nodes, &mut spans, &mut identities, &mut alive).unwrap();
    assert_eq!(nodes.iter().map(|(_, node)| node.value().unwrap()).collect::<Vec<_>>(), ["a", "é", "界", "\n"]);
    assert_eq!(nodes.iter().map(|(_, node)| node.character_lengths()[0]).collect::<Vec<_>>(), [1, 2, 3, 1]);
    assert_eq!(field.children().len(), 4);
    let selection = *field.text_selection().unwrap();
    assert_eq!(selection.anchor, TextPosition { node: field.children()[1], character_index: 0 });
    assert_eq!(selection.focus, TextPosition { node: field.children()[3], character_index: 1 });
    assert_eq!(translate_selection(&spans, entity, selection), Some(original));
    assert_eq!(translate_selection(&spans, foreign, selection), None);
    let invalid = TextSelection { anchor: TextPosition { character_index: 2, ..selection.anchor }, ..selection };
    assert_eq!(translate_selection(&spans, entity, invalid), None);
    assert_eq!(nodes[0].1.next_on_line(), Some(field.children()[1]));
    assert_eq!(nodes[3].1.previous_on_line(), Some(field.children()[2]));
    assert!(nodes.iter().all(|(_, node)| node.bounds().is_none() && node.character_positions().is_none()));
    let saved = identities.clone();
    let mut second_field = Node::new(Role::TextInput);
    second_field.set_children([source_id]);
    project(&world, &view, None, entity, &mut second_field, vec![(source_id, node)], &mut Vec::new(), &mut BTreeMap::new(), &mut identities, &mut BTreeSet::new()).unwrap();
    assert_eq!(identities, saved);
    assert_eq!(second_field.children(), field.children());
}

#[test]
fn empty_text_keeps_its_caret_and_malformed_source_cannot_export_inconsistent_spans() {
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    let view = crate::native_input::projection::View { root: None, output: lince_interface::host::OutputId(1),
        image: Handle::<Image>::default().id(), raster: Default::default(), origin: [0.0; 2], viewport: None };
    let source_id = NodeId(50_000);
    let mut field = Node::new(Role::TextInput);
    field.set_children([source_id]);
    let position = TextPosition { node: source_id, character_index: 0 };
    field.set_text_selection(TextSelection { anchor: position, focus: position });
    let mut spans = BTreeMap::new();
    let mut nodes = Vec::new();
    project(&world, &view, None, entity, &mut field, vec![(source_id, Node::new(Role::TextRun))], &mut nodes, &mut spans, &mut BTreeMap::new(), &mut BTreeSet::new()).unwrap();
    let exported = *field.text_selection().unwrap();
    assert_eq!(translate_selection(&spans, entity, exported), Some(TextSelection { anchor: position, focus: position }));
    assert!(nodes[0].1.character_lengths().is_empty());
    let (source_id, mut malformed) = source();
    malformed.set_character_lengths([1, 1, 3, 1]);
    field.set_children([source_id]);
    let mut nodes = Vec::new();
    project(&world, &view, None, entity, &mut field, vec![(source_id, malformed)], &mut nodes, &mut BTreeMap::new(), &mut BTreeMap::new(), &mut BTreeSet::new()).unwrap();
    assert!(nodes.is_empty() && field.children().is_empty() && field.text_selection().is_none());
}

#[test]
fn displayed_character_extents_use_final_density_rotation_and_all_projected_corners() {
    use lince_interface::host::{OutputId, OutputViewport, Rectangle, mapping::{Mapping, QuarterTurn}, raster::RasterMapping};
    let bounds = Rectangle::new([1000.0, 50.0], [960.0, 640.0]).unwrap();
    let output = OutputViewport { rotation: Default::default(), id: OutputId(1), bounds, usable: Some(bounds), physical_size: [1200, 800],
        raster: RasterMapping { origin: bounds.origin(), scale: [1.25; 2] }, workspace: 1 };
    let mapping = Mapping::new(&output, QuarterTurn::Normal, 2.0, [1480.0, 370.0], 1).unwrap();
    let points = [[1360.0, 290.0], [1370.0, 290.0], [1370.0, 310.0], [1360.0, 310.0]];
    assert_eq!(displayed_points(points, Some(&mapping), TextDirection::LeftToRight),
        Some((accesskit::Rect::new(1240.0, 210.0, 1260.0, 250.0), TextDirection::LeftToRight)));
    let caret = [[1370.0, 290.0], [1370.0, 290.0], [1370.0, 310.0], [1370.0, 310.0]];
    assert_eq!(displayed_points(caret, Some(&mapping), TextDirection::LeftToRight).unwrap().0,
        accesskit::Rect::new(1260.0, 210.0, 1260.0, 250.0));
    let offscreen = [[1000.0, 50.0], [1010.0, 50.0], [1010.0, 70.0], [1000.0, 70.0]];
    assert_eq!(displayed_points(offscreen, Some(&mapping), TextDirection::LeftToRight), None);
    let bounds = Rectangle::new([1000.0, 50.0], [1200.0, 800.0]).unwrap();
    let output = OutputViewport { rotation: QuarterTurn::Clockwise90, bounds, usable: Some(bounds), physical_size: [800, 1200],
        raster: RasterMapping { origin: bounds.origin(), scale: [1.0; 2] }, ..output };
    let mapping = Mapping::new(&output, QuarterTurn::Clockwise90, 1.0, [1600.0, 450.0], 2).unwrap();
    let points = [[1100.0, 250.0], [1110.0, 250.0], [1110.0, 270.0], [1100.0, 270.0]];
    assert_eq!(displayed_points(points, Some(&mapping), TextDirection::LeftToRight),
        Some((accesskit::Rect::new(1100.0, 250.0, 1110.0, 270.0), TextDirection::LeftToRight)));
    let perspective = [[40.0, 50.0], [70.0, 40.0], [80.0, 100.0], [45.0, 90.0]];
    assert_eq!(displayed_points(perspective, None, TextDirection::LeftToRight).unwrap().0, accesskit::Rect::new(40.0, 40.0, 80.0, 100.0));
}
