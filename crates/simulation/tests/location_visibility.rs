fn run(transport: bool) {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    if transport {
                        simulation::location_workflows::transport_with_address().await;
                    } else {
                        simulation::location_workflows::visibility_handoff().await;
                    }
                });
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn four_linces_share_record_locations_during_a_map_free_transfer() {
    run(false);
}

#[test]
fn four_linces_transport_with_address_and_configurable_geographic_areas() {
    run(true);
}
