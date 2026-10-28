use super::*;

#[test]
fn twenty_tablet_lifetimes_release_every_held_edge_and_recover_with_independent_tool_identity() {
    let mut harness = Harness::new();
    let _seat = subscribe(&mut harness);
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    let surface = harness.server.windows()[0].surface.clone();
    let baseline_tools = harness.server.state.seat.tablet_seat().count_tools();
    let baseline_devices = harness.server.state.seat.tablet_seat().count_tablets();
    let mut identities = BTreeSet::new();
    for cycle in 0..20u64 {
        let device = cycle * 2 + 1;
        let id = cycle * 2 + 100;
        let serial = 0x1234567800000000 + cycle;
        let tool = admit(&mut harness, device, id, serial);
        let object = tool.id().protocol_id();
        let tablet = harness.client.tablet.devices.last().unwrap().id().protocol_id();
        assert!(identities.insert(serial));
        assert_eq!(harness.client.tablet.hardware[&object], serial);
        assert_eq!(harness.server.state.seat.tablet_seat().count_tools(), baseline_tools + 1);
        assert_eq!(harness.server.state.seat.tablet_seat().count_tablets(), baseline_devices + 1);
        let pressure = 0.12345 + cycle as f64 * 0.01;
        let distance = 0.23456;
        let tilt = [-11.234, 16.789];
        let rotation = 43.123;
        let slider = -0.34567;
        assert!(harness.server.tablet_motion(id, OutputId(1), [30.0, 40.0], Some((surface.clone(), [10.125, 20.375])), false,
            TabletAxes { pressure: Some(pressure), distance: Some(distance), tilt: Some(tilt), rotation: Some(rotation), slider: Some(slider), wheel: Some((15.125, 1)) }, 1));
        assert!(harness.server.tablet_tip(id, true, 2).is_some());
        assert!(harness.server.tablet_button(id, 331, true, 3).is_some());
        harness.sync();
        assert_eq!(harness.client.tablet.enters.last(), Some(&(object, tablet)));
        let samples = harness.client.tablet.samples.iter().filter_map(|(owner, sample)| (*owner == object).then_some(sample)).collect::<Vec<_>>();
        assert!(samples.iter().any(|sample| matches!(sample, Sample::Pressure(value) if (*value as f64 / 65535.0 - pressure).abs() <= 1.0 / 65535.0)));
        assert!(samples.iter().any(|sample| matches!(sample, Sample::Distance(value) if (*value as f64 / 65535.0 - distance).abs() <= 1.0 / 65535.0)));
        assert!(samples.iter().any(|sample| matches!(sample, Sample::Tilt(value) if value.iter().zip(tilt).all(|(actual, expected)| (*actual - expected).abs() <= 1.0 / 256.0))));
        assert!(samples.iter().any(|sample| matches!(sample, Sample::Rotation(value) if (*value - rotation).abs() <= 1.0 / 256.0)));
        assert!(samples.iter().any(|sample| matches!(sample, Sample::Slider(value) if (*value as f64 / 65535.0 - slider).abs() <= 1.0 / 65535.0)));
        harness.client.tablet.samples.clear();
        harness.client.tablet.removed.clear();
        if cycle % 2 == 0 { harness.server.tablet_cancel(4); }
        harness.server.tablet_remove_device(device, 5);
        harness.sync();
        harness.sync();
        let samples = harness.client.tablet.samples.iter().filter_map(|(owner, sample)| (*owner == object).then_some(sample)).collect::<Vec<_>>();
        for expected in [Sample::Button(331, false), Sample::Up, Sample::Out, Sample::Removed] {
            assert_eq!(samples.iter().filter(|sample| ***sample == expected).count(), 1, "Cycle {cycle}: {expected:?}");
        }
        assert_eq!(harness.client.tablet.removed, [tablet]);
        assert_eq!(harness.server.state.seat.tablet_seat().count_tools(), baseline_tools);
        assert_eq!(harness.server.state.seat.tablet_seat().count_tablets(), baseline_devices);
        assert!(harness.server.tablet_cursors().is_empty());
        assert!(!harness.server.tablet_active(id));
        assert!(harness.server.tablet_tip(id, true, 6).is_none());
        assert!(!harness.server.tablet_motion(id, OutputId(1), [30.0, 40.0], Some((surface.clone(), [10.0, 20.0])), false, Default::default(), 7));
        harness.client.tablet.devices.retain(Proxy::is_alive);
        harness.client.tablet.tools.retain(Proxy::is_alive);
        harness.client.tablet.hardware.clear();
        harness.client.tablet.serials.clear();
        harness.client.tablet.enters.clear();
        assert!(harness.client.tablet.devices.is_empty());
        assert!(harness.client.tablet.tools.is_empty());
    }
    assert_eq!(identities.len(), 20);
}
