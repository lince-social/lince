use super::*;
use lince_interface::host::{ContentMapping, GeometrySnapshot, ItemGeometry, ItemId, Rectangle as CanvasRectangle};

#[test]
fn idle_inhibition_stops_when_its_presented_output_disappears() {
    let mut harness = Harness::new();
    let monitors = [
        Monitor { id: OutputId(1), name: "First".into(), origin: [0, 0], size: [640, 480] },
        Monitor { id: OutputId(2), name: "Second".into(), origin: [640, 0], size: [640, 480] },
    ];
    harness.server.configure_outputs(&monitors).unwrap();
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    let id = harness.server.windows()[0].id;
    let rectangle = CanvasRectangle::new([640.0, 0.0], [320.0, 240.0]).unwrap();
    let geometry = GeometrySnapshot { revision: 1, outputs: harness.server.output_viewports().into_iter().map(|(_, viewport)| viewport).collect(), focused: None,
        items: vec![ItemGeometry { id: ItemId::Application(id), output: OutputId(2), mapping: ContentMapping { canvas: rectangle, surface: rectangle },
            floating: false, clip: None }] };
    harness.server.presented_outputs(&geometry, Some(OutputId(2)));
    let qh = harness.queue.handle();
    let inhibitor = harness.client.idle_inhibit.as_ref().unwrap().create_inhibitor(&window.surface, &qh, ());
    harness.sync();
    let notification = harness.client.idle.as_ref().unwrap().get_idle_notification(0, harness.client.seat.as_ref().unwrap(), &qh, ());
    harness.sync();
    assert!(harness.client.idle_events.is_empty());
    harness.server.configure_outputs(&monitors[..1]).unwrap();
    harness.sync();
    assert_eq!(harness.client.idle_events, [true]);
    harness.server.configure_outputs(&monitors).unwrap();
    harness.server.presented_outputs(&geometry, Some(OutputId(2)));
    harness.sync();
    assert_eq!(harness.client.idle_events, [true, false]);
    inhibitor.destroy();
    harness.sync();
    assert_eq!(harness.client.idle_events, [true, false, true]);
    notification.destroy();
    harness.sync();
}
