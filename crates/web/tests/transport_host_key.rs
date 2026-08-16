//! Run the board transport's host-resolution test.
//!
//! The Rust side of this crate cannot see which Cell a card's binding reaches
//! — that decision is made in `transport.js`, and getting it wrong is how
//! every write from an ordinary local card came back "not a contact" while the
//! host picker sat on "Local Lince".
//!
//! Skipped when no `node` is on PATH rather than failing, because a missing
//! JavaScript engine is a property of the machine, not of the code under test.
//! Reported loudly so a green run is never mistaken for a checked one.

use std::process::Command;

#[test]
fn a_local_or_unknown_binding_resolves_to_our_own_cell() {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/transport_host_key.mjs");

    let Ok(output) = Command::new("node").arg(script).output() else {
        eprintln!(
            "SKIPPED: no `node` on PATH, so the board transport's host resolution was NOT \
             checked.\nRun it directly with: node {script}"
        );
        return;
    };

    if !output.status.success() {
        panic!(
            "a card's binding did not resolve to the Cell it names:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}
