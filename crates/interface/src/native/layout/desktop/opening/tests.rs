use super::*;
use crate::layout::desktop::Width;
use nucleus::property_rule::{Property, PropertyRule};
use serde_json::json;

fn rule(width: [u16; 2]) -> Rule {
    Rule { revision: 1, enabled: true, match_all: true, conditions: vec![PropertyRule { property: Property::ApplicationId, value: "org.example.Editor".into() }], properties: Properties { width: Some(Dimension::Fraction(width)), floating: Some(false), ..Default::default() } }
}

fn entry(payload: Payload) -> Entry {
    Entry { title: "Owner rule".into(), exists: true, value: payload.value(), payload: Ok(payload) }
}

fn fixture() -> (World, Entity, Entity, Entity, String) {
    let (mut world, root, native, application) = super::super::tests::fixture();
    world.init_resource::<Catalog>();
    let uid = nucleus::new_uid("r");
    world.insert_resource(LayoutSettings { opening_rules: vec![uid.clone()], gaps: 0, ..Default::default() });
    world.resource_mut::<Catalog>().rules.entries.insert(uid.clone(), entry(Payload::Rule(rule([1, 2]))));
    world.resource_mut::<Catalog>().rules.ready = true;
    world.get_mut::<Workspaces>(root).unwrap().entries[0].desktop.set_mode(WorkspaceLayout::Tiled);
    crate::native::layout::engine::resolve(&mut world);
    (world, root, native, application, uid)
}

#[test]
fn saved_order_not_subscription_order_selects_one_sparse_match() {
    let (mut world, _, _, _, first) = fixture();
    let later = nucleus::new_uid("r");
    world.resource_mut::<Catalog>().rules.entries.insert(later.clone(), entry(Payload::Rule(rule([2, 3]))));
    world.resource_mut::<LayoutSettings>().opening_rules.push(later);
    let window = nucleus::new_uid("r");
    world.resource_mut::<Catalog>().windows.entries.insert(window.clone(), entry(Payload::Window(Window { revision: 1, properties: Properties { focus: Some(false), ..Default::default() } })));
    let decision = capture(&world, &json!({"app_id":"org.example.Editor"}), Some(&window));
    assert_eq!(decision.matched.as_ref().unwrap().uid, first);
    assert_eq!(decision.properties.width, Some(Dimension::Fraction([1, 2])));
    assert_eq!(decision.properties.focus, Some(false));
    assert_eq!(decision.properties.floating, Some(false));
}

#[test]
fn unavailable_priority_does_not_promote_a_lower_rule_and_window_fault_is_visible() {
    let (mut world, _, _, _, uid) = fixture();
    let missing = nucleus::new_uid("r");
    world.resource_mut::<LayoutSettings>().opening_rules.insert(0, missing);
    let decision = capture(&world, &json!({"app_id":"org.example.Editor"}), None);
    assert!(decision.matched.is_none());
    assert!(decision.warning.is_some());
    world.resource_mut::<LayoutSettings>().opening_rules = vec![uid];
    let window = nucleus::new_uid("r");
    let decision = capture(&world, &json!({"app_id":"org.example.Editor"}), Some(&window));
    assert!(decision.matched.is_some());
    assert!(decision.warning.as_deref().unwrap().contains("overrides"));
}

#[test]
fn explicit_reapply_keeps_identity_and_does_not_select_the_background_application() {
    let (mut world, root, native, application, _) = fixture();
    let id = PlacementId(world.get::<LayoutBox>(application).unwrap().id);
    focus_item(&mut world, native);
    let before = world.get::<Workspaces>(root).unwrap().entries[0].desktop.focused();
    let decision = capture(&world, &json!({"app_id":"org.example.Editor"}), None);
    let prepared = prepare_reapply(&mut world, root, 1, id, 1, &decision).unwrap();
    prepared.commit(&mut world).unwrap();
    let model = &world.get::<Workspaces>(root).unwrap().entries[0].desktop;
    assert_eq!(model.focused(), before);
    assert_eq!(model.item_width(id), Some(Width::Fraction(1, 2)));
    assert_eq!(world.get::<WorkspaceMember>(application).unwrap().0, 1);
    assert_eq!(world.get::<ApplicationWindow>(application).unwrap().surface, SurfaceId(7));
}

#[test]
fn changed_rule_order_canvas_mapping_and_surface_retire_the_whole_preparation() {
    for changed in 0..6 {
        let (mut world, root, _, application, _) = fixture();
        let id = PlacementId(world.get::<LayoutBox>(application).unwrap().id);
        let before = world.get::<Workspaces>(root).unwrap().entries[0].desktop.clone();
        let decision = capture(&world, &json!({"app_id":"org.example.Editor"}), None);
        let prepared = prepare_reapply(&mut world, root, 1, id, 1, &decision).unwrap();
        match changed {
            0 => world.resource_mut::<Catalog>().advance(),
            1 => world.resource_mut::<LayoutSettings>().opening_rules.clear(),
            2 => world.get_mut::<Workspaces>(root).unwrap().canvas_id = nucleus::new_uid("r"),
            3 => world.get_mut::<ApplicationWindow>(application).unwrap().surface = SurfaceId(8),
            4 => world.get_mut::<CanvasView>(root).unwrap().zoom = 0.5,
            _ => world.get_mut::<CanvasItem>(application).unwrap().position.x += 12.0,
        }
        assert!(prepared.commit(&mut world).is_err());
        assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop, before);
        assert_eq!(world.get::<WorkspaceMember>(application).unwrap().0, 1);
    }
}

#[test]
fn empty_and_failed_catalogs_settle_without_ongoing_deadlines() {
    let mut world = World::new();
    world.insert_resource(LayoutSettings::default());
    world.init_resource::<Catalog>();
    assert!(ready(&world));
    let uid = nucleus::new_uid("r");
    world.resource_mut::<LayoutSettings>().opening_rules.push(uid.clone());
    assert!(!ready(&world));
    world.resource_mut::<Catalog>().rules.error = Some("Cell unavailable".into());
    world.resource_mut::<Catalog>().rules.selected.insert(uid);
    assert!(ready(&world));
    assert!(world.resource::<Catalog>().clock.is_none());
    let decision = capture(&world, &json!({"app_id":"org.example.Editor"}), None);
    assert!(decision.warning.as_deref().unwrap().contains("unavailable"));
}

#[test]
fn a_queued_command_cannot_adopt_a_new_rule_order_or_cell_generation() {
    let (mut world, root, _, application, _) = fixture();
    let target = crate::host::WindowInspection { owner: root.to_bits(), workspace: 1, position: [0.0; 2], surface: SurfaceId(7), placement: world.get::<LayoutBox>(application).unwrap().id };
    let queued = request(&world, root, target).unwrap();
    assert!(request_current(&world, root, &queued));
    world.resource_mut::<LayoutSettings>().opening_rules.clear();
    assert!(!request_current(&world, root, &queued));
    let queued = request(&world, root, target).unwrap();
    world.resource_mut::<Catalog>().advance();
    assert!(!request_current(&world, root, &queued));
    assert_eq!(queued.target.placement, target.placement);
}

#[test]
fn queued_reapply_rejects_changed_canvas_workspace_output_assignment_and_mapping_before_preparation() {
    use crate::{host::{OutputId, OutputViewport, raster::RasterMapping}, native::output_views::OutputViews};
    for change in 0..7 {
        let (mut world, root, _, application, _) = fixture();
        let bounds = Rectangle::new([0.0; 2], [1000.0, 720.0]).unwrap();
        world.insert_resource(OutputViews { canvas: Some(root), focused: OutputId(1), outputs: vec![OutputViewport {
            id: OutputId(1), bounds, usable: Some(bounds), physical_size: [1000, 720], rotation: Default::default(),
            raster: RasterMapping { origin: [0.0; 2], scale: [1.0; 2] }, workspace: 1,
        }] });
        let target = crate::host::WindowInspection { owner: root.to_bits(), workspace: 1, position: [0.0; 2], surface: SurfaceId(7), placement: world.get::<LayoutBox>(application).unwrap().id };
        let queued = request(&world, root, target).unwrap();
        assert!(request_current(&world, root, &queued));
        match change {
            0 => world.get_mut::<Workspaces>(root).unwrap().canvas_id = nucleus::new_uid("canvas"),
            1 => world.get_mut::<Workspaces>(root).unwrap().active = 2,
            2 => { world.remove_resource::<OutputViews>(); }
            3 => world.resource_mut::<OutputViews>().focused = OutputId(2),
            4 => world.resource_mut::<OutputViews>().outputs[0].workspace = 2,
            5 => world.resource_mut::<OutputViews>().outputs[0].raster.origin = [100.0, 0.0],
            _ => world.resource_mut::<OutputViews>().outputs[0].usable = Rectangle::new([0.0, 40.0], [1000.0, 680.0]),
        }
        assert!(!request_current(&world, root, &queued));
        assert_eq!(queued.target.placement, target.placement);
    }
}

#[test]
fn initial_preferences_are_not_reapplied_by_title_focus_or_rule_changes() {
    let (mut world, root, native, application, uid) = fixture();
    world.resource_mut::<Catalog>().rules.entries.insert(uid.clone(), entry(Payload::Rule(rule([1, 3]))));
    let decision = capture(&world, &json!({"app_id":"org.example.Editor"}), None);
    world.entity_mut(application).insert((decision, ApplicationOpeningPending));
    crate::native::layout::engine::resolve(&mut world);
    let id = PlacementId(world.get::<LayoutBox>(application).unwrap().id);
    world.resource_mut::<Catalog>().rules.entries.insert(uid, entry(Payload::Rule(rule([2, 3]))));
    world.resource_mut::<Catalog>().advance();
    for index in 0..100 {
        world.entity_mut(application).insert(crate::native::area::RecordProperties(json!({"head":format!("Document {index}"),"app_id":"org.example.Editor"})));
        focus_item(&mut world, if index % 2 == 0 { application } else { native });
        crate::native::layout::engine::resolve(&mut world);
        assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop.item_width(id), Some(Width::Fraction(1, 3)));
        assert!(world.get::<ApplicationOpeningPending>(application).is_none());
    }
    let decision = capture(&world, &json!({"app_id":"org.example.Editor"}), None);
    prepare_reapply(&mut world, root, 1, id, 1, &decision).unwrap().commit(&mut world).unwrap();
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop.item_width(id), Some(Width::Fraction(2, 3)));
}

#[test]
fn an_equal_pixel_width_request_replaces_the_retained_fraction_in_free_mode() {
    let (mut world, root, _, application, uid) = fixture();
    let id = PlacementId(world.get::<LayoutBox>(application).unwrap().id);
    {
        let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
        let model = &mut spaces.entries[0].desktop;
        model.set_mode(WorkspaceLayout::Free);
        model.set_free_width(id, [1, 2], 1.0).unwrap();
    }
    crate::native::layout::engine::resolve(&mut world);
    let pixels = world.get::<Workspaces>(root).unwrap().entries[0].desktop.items().iter().find(|item| item.id == id).unwrap().free.size[0] as u32;
    let mut fixed = rule([1, 2]);
    fixed.properties.width = Some(Dimension::Pixels(pixels));
    world.resource_mut::<Catalog>().rules.entries.insert(uid, entry(Payload::Rule(fixed)));
    world.resource_mut::<Catalog>().advance();
    let decision = capture(&world, &json!({"app_id":"org.example.Editor"}), None);
    prepare_reapply(&mut world, root, 1, id, 1, &decision).unwrap().commit(&mut world).unwrap();
    let model = &world.get::<Workspaces>(root).unwrap().entries[0].desktop;
    let item = model.items().iter().find(|item| item.id == id).unwrap();
    assert!(item.free_width.is_none());
    assert_eq!(item.free.size[0], f64::from(pixels));
}
