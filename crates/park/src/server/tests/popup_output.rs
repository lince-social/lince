use super::*;
use lince_interface::host::{ContentMapping, GeometrySnapshot, ItemGeometry, ItemId, Rectangle as CanvasRectangle};

#[test]
fn reactive_popup_follows_parent_output_and_fractional_usable_region() {
    let mut harness = Harness::new();
    harness.server.configure_scaled_outputs(&[
        Monitor { id: OutputId(1), name: "First".into(), origin: [0, 0], size: [640, 480] },
        Monitor { id: OutputId(2), name: "Second".into(), origin: [640, 0], size: [640, 480] },
    ], &BTreeMap::from([(OutputId(2), 1.25)])).unwrap();
    harness.sync();
    let window = harness.window();
    let _parent_buffer = harness.attach(&window, [320, 240]);
    let id = harness.server.windows()[0].id;
    let rectangle = CanvasRectangle::new([0.0; 2], [320.0, 240.0]).unwrap();
    let mut geometry = GeometrySnapshot { revision: 1, outputs: harness.server.output_viewports().into_iter().map(|(_, viewport)| viewport).collect(), focused: None,
        items: vec![ItemGeometry { id: ItemId::Application(id), output: OutputId(1),
            mapping: ContentMapping { corners: None, canvas: rectangle, surface: rectangle }, floating: false, clip: None }] };
    harness.server.presented_outputs(&geometry, Some(OutputId(1)));
    let qh = harness.queue.handle();
    let positioner = harness.client.shell.as_ref().unwrap().create_positioner(&qh, ());
    positioner.set_size(160, 80);
    positioner.set_anchor_rect(300, 200, 1, 1);
    positioner.set_anchor(xdg_positioner::Anchor::BottomRight);
    positioner.set_gravity(xdg_positioner::Gravity::BottomRight);
    positioner.set_constraint_adjustment(xdg_positioner::ConstraintAdjustment::SlideX | xdg_positioner::ConstraintAdjustment::SlideY);
    positioner.set_reactive();
    let surface = harness.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let xdg = harness.client.shell.as_ref().unwrap().get_xdg_surface(&surface, &qh, ());
    let popup = xdg.get_popup(Some(&window.xdg), &positioner, &qh, ());
    surface.commit();
    harness.sync();
    let buffer = harness.buffer([160, 80]);
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    harness.sync();
    let before = *harness.client.popup_configures.last().unwrap();
    let count = harness.client.popup_configures.len();
    geometry.revision += 1;
    geometry.items[0].output = OutputId(2);
    geometry.items[0].mapping.canvas = CanvasRectangle::new([1040.0, 200.0], [320.0, 240.0]).unwrap();
    let usable = CanvasRectangle::new([640.0, 40.0], [512.0, 200.0]).unwrap();
    geometry.outputs.iter_mut().find(|output| output.id == OutputId(2)).unwrap().usable = Some(usable);
    harness.server.presented_outputs(&geometry, Some(OutputId(2)));
    harness.sync();
    assert!(harness.client.popup_configures.len() > count);
    let after = *harness.client.popup_configures.last().unwrap();
    assert_ne!(after, before);
    assert_eq!([after[2], after[3]], [160, 80]);
    assert!(1040 + after[0] >= 640);
    assert!(1040 + after[0] + after[2] <= 1152);
    assert!(200 + after[1] >= 40);
    assert!(200 + after[1] + after[3] <= 240);
    assert_eq!(harness.client.popup_done, 0);
    popup.destroy();
    xdg.destroy();
    surface.destroy();
    positioner.destroy();
    harness.sync();
}
