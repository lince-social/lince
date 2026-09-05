#[test]
#[ignore = "Resenha DST: needs three Cells with separate stores"]
fn a_designated_rule_acts_exactly_once_across_three_cells() {
    unimplemented!("scenario: three Cells, one designated executor, 60 days");
}

#[test]
#[ignore = "Resenha DST: needs an asymmetric partition"]
fn an_unreachable_holder_does_not_hand_the_lease_to_whoever_cannot_see_it() {
    unimplemented!("scenario: one-way partition, designated executor unreachable");
}

#[test]
#[ignore = "Resenha DST: needs two Cells sharing one pending delivery"]
fn two_cells_do_not_each_retry_the_same_transfer_delivery() {
    unimplemented!("scenario: two Cells, one pending delivery, both online");
}

#[test]
#[ignore = "Resenha DST: needs a driven clock with skew"]
fn concurrent_cells_under_clock_skew_lose_no_write() {
    unimplemented!("scenario: two Cells, skewed clocks, one backwards jump");
}

#[test]
#[ignore = "Resenha DST: needs three nodes and a restartable relay"]
fn two_unreachable_cells_converge_through_a_restarted_relay() {
    unimplemented!("scenario: two Cells behind one relay, relay restarts mid-run");
}

#[test]
#[ignore = "Resenha DST: needs reorder and duplicate injection"]
fn narrowing_mid_flight_leaves_nothing_outside_the_new_scope() {
    unimplemented!("scenario: narrow scope during an in-flight batch, reordered delivery");
}

#[test]
#[ignore = "Resenha DST: needs three nodes with independent uptime"]
fn an_always_on_cell_serves_the_change_and_no_mail_is_left() {
    unimplemented!("scenario: laptop writes, laptop sleeps, phone wakes, always-on Cell serves");
}

#[test]
#[ignore = "Resenha DST: needs driven uptime windows that never overlap"]
fn mail_delivers_between_two_organs_that_are_never_awake_together() {
    unimplemented!("scenario: disjoint uptime windows, one always-up carrier");
}

#[test]
#[ignore = "Resenha DST: needs a network that can refuse a hole punch"]
fn a_connection_that_never_upgrades_to_direct_still_converges() {
    unimplemented!("scenario: symmetric NAT on both sides, one relay, no upgrade");
}
