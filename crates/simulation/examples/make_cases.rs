fn main() -> simulation::Result<()> {
    let directory = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("provide an output directory")?,
    );
    std::fs::create_dir_all(&directory)?;
    for case in [
        simulation::fixtures::daily(),
        simulation::fixtures::network(false),
        simulation::fixtures::network(true),
        simulation::fixtures::transfer::sale(),
        simulation::fixtures::transfer::visibility(),
        simulation::fixtures::transfer::three_parties(),
        simulation::fixtures::transfer::competing_reservations(false),
        simulation::fixtures::transfer::competing_reservations(true),
        simulation::fixtures::transfer::independent_donation(false),
        simulation::fixtures::transfer::independent_donation(true),
        simulation::fixtures::transfer::donation_with_lost_acknowledgements(false),
        simulation::fixtures::transfer::donation_with_lost_acknowledgements(true),
        simulation::fixtures::transfer::trade(false),
        simulation::fixtures::transfer::trade(true),
        simulation::fixtures::transfer::private_trade(false),
        simulation::fixtures::transfer::private_trade(true),
        simulation::fixtures::transfer::counteroffer(false),
        simulation::fixtures::transfer::counteroffer(true),
        simulation::fixtures::transfer::open_offer(false),
        simulation::fixtures::transfer::open_offer(true),
        simulation::fixtures::transfer::partial_cancellation(false),
        simulation::fixtures::transfer::partial_cancellation(true),
        simulation::fixtures::transfer::grouped_needs(false, false, false),
        simulation::fixtures::transfer::grouped_needs(true, false, true),
        simulation::fixtures::transfer::temporary_loan(false, false),
        simulation::fixtures::transfer::temporary_loan(true, true),
        simulation::fixtures::transfer::extended_loan(false),
        simulation::fixtures::transfer::extended_loan(true),
        simulation::fixtures::transfer::nested_parents(false),
        simulation::fixtures::transfer::nested_parents(true),
        simulation::fixtures::transfer::observer_outcomes(false),
        simulation::fixtures::transfer::observer_outcomes(true),
    ] {
        let path = directory.join(format!("{}.json", case.name));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        serde_json::to_writer_pretty(file, &case)?;
    }
    Ok(())
}
