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
