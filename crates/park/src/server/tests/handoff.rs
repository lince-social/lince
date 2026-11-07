use super::*;

#[test]
fn handoff_inventory_counts_real_protocol_state_without_retiring_the_client() {
    let mut harness = Harness::new();
    let first = harness.window();
    let _first_buffer = harness.attach(&first, [64, 48]);
    let second = harness.window();
    let _second_buffer = harness.attach(&second, [64, 48]);
    let before = harness.server.handoff_inventory().unwrap();
    assert_eq!(before.clients, 1);
    assert_eq!(before.surfaces, 2);
    assert_eq!(before.mapped_windows, 2);
    assert!(before.objects > 2);
    assert!(
        before
            .protocols
            .iter()
            .any(|protocol| protocol.interface == "wl_surface" && protocol.objects == 2)
    );
    assert!(
        before
            .protocols
            .iter()
            .any(|protocol| protocol.interface == "xdg_toplevel" && protocol.objects == 2)
    );
    assert!(!before.supported);
    assert!(!before.blockers.is_empty());
    assert_eq!(
        before.objects,
        before
            .protocols
            .iter()
            .map(|protocol| protocol.objects)
            .sum::<usize>()
    );
    let json = serde_json::to_string(&before).unwrap();
    assert!(!json.contains("A real Wayland window"));
    for _ in 0..3 {
        harness.sync();
        assert_eq!(
            harness.server.handoff_inventory().unwrap().mapped_windows,
            2
        );
    }
    first.surface.damage_buffer(0, 0, 32, 24);
    first.surface.commit();
    harness.sync();
    let after = harness.server.handoff_inventory().unwrap();
    assert_eq!(after.clients, before.clients);
    assert_eq!(after.mapped_windows, before.mapped_windows);
}
