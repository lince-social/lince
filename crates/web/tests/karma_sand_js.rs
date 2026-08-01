//! Run the Karma sand's browser-side tests.
//!
//! The sand's Rust tests can only assert that the rendered markup contains a
//! string. The part that actually breaks is the JavaScript: a rule authored in
//! the form, stored, and read back has to arrive as the rule its author wrote,
//! and nothing in Rust can see that. `karma_sand_js.mjs` is that round trip.
//!
//! Skipped when no `node` is on PATH rather than failing, because a missing
//! JavaScript engine is a property of the machine, not of the code under test.
//! It is reported loudly so a green run is never mistaken for a checked one.

use std::process::Command;

#[test]
fn the_rule_form_round_trips_in_a_real_javascript_engine() {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/karma_sand_js.mjs");

    let Ok(output) = Command::new("node").arg(script).output() else {
        eprintln!(
            "SKIPPED: no `node` on PATH, so the Karma sand's JavaScript was NOT checked.\n\
             Run it directly with: node {script}"
        );
        return;
    };

    if !output.status.success() {
        panic!(
            "the Karma sand's rule form failed its round trip:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}
