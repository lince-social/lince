//! The board reaching ANOTHER Lince (Ontology §11 "live mode", guest half).
//!
//! The host half — a contact with a login driving a real session over iroh —
//! has worked for a while and is covered by `transport/tests/live_workflow.rs`.
//! What did not exist was any way for a board to be the guest:
//! `/live/{organ}/connect` was a route with no caller anywhere, and
//! `transport.js` hardcoded `window.location.host`.
//!
//! The mechanism is deliberately one line of routing rather than a second
//! client: the remote Cell speaks EXACTLY the frames our own does, so pointing
//! the board's single socket at the relay makes every subscription, Action,
//! lane and collab doc follow. This runs the SHIPPED `transport.js` in node to
//! prove the routing and, more importantly, the two ways it could silently
//! corrupt someone else's Organ.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Stage the real module beside a harness that fakes only the browser globals
/// it touches. Copying the shipped file (never a transcription of it) is the
/// whole point — a divergence between this and what is served makes the test
/// worthless.
fn run(label: &str, body: &str) {
    if !node_available() {
        eprintln!("SKIP live mode client test `{label}`: node is not on PATH");
        return;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir: PathBuf = std::env::temp_dir().join(format!(
        "lince-live-{label}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create dir");

    let transport = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("static/presentation/board/transport.js");
    fs::copy(&transport, dir.join("transport.js")).expect("stage transport.js");

    let harness = r#"
// Only what transport.js actually reaches for. Sockets record instead of
// connecting, so the test can read back exactly where the board was pointed.
export const opened = [];
class FakeWebSocket {
  constructor(url) {
    this.url = url;
    this.readyState = 1;
    this.sent = [];
    this.listeners = {};
    opened.push(this);
  }
  addEventListener(type, fn) { (this.listeners[type] ||= []).push(fn); }
  send(data) { this.sent.push(data); }
  close() {
    this.readyState = 3;
    for (const fn of this.listeners.close || []) fn();
  }
  fire(type, event) { for (const fn of this.listeners[type] || []) fn(event); }
}
FakeWebSocket.OPEN = 1;
globalThis.WebSocket = FakeWebSocket;
globalThis.window = {
  location: { protocol: "http:", host: "127.0.0.1:6174" },
  setTimeout: () => 0,
  btoa: (s) => Buffer.from(s, "binary").toString("base64"),
  indexedDB: null,
  isSecureContext: false,
  crypto: {},
};
globalThis.setTimeout = () => 0;
"#;
    fs::write(dir.join("harness.mjs"), harness).expect("write harness");

    let path = dir.join("test.mjs");
    fs::write(&path, body).expect("write test");
    let output = Command::new("node")
        .arg(&path)
        .current_dir(&dir)
        .output()
        .expect("launch node");
    let _ = fs::remove_dir_all(&dir);

    if !output.status.success() {
        panic!(
            "live mode client assertions failed ({:?})\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

/// The board points at our own Cell until told otherwise, at the relay while
/// in live mode, and back home again — and the relay path names the Organ.
#[test]
fn the_board_points_at_the_contacts_cell_while_in_live_mode() {
    run(
        "route",
        r#"
import { opened } from "./harness.mjs";
import assert from "node:assert/strict";
const { getSharedTransport } = await import("./transport.js");

const transport = getSharedTransport();
assert.equal(opened.length, 1, "the board opens exactly one socket");
assert.match(
  opened[0].url,
  /\/host\/transport\/ws$/,
  "by default the board drives our OWN Cell",
);
assert.equal(transport.getLiveOrgan(), null);

transport.setLiveOrgan("organ-marcia");
assert.equal(opened.length, 2, "switching reconnects rather than multiplexing");
assert.equal(
  opened[1].url,
  "ws://127.0.0.1:6174/live/organ-marcia/connect",
  "live mode rides our own Cell's relay, which reaches them over iroh",
);
assert.equal(transport.getLiveOrgan(), "organ-marcia");

transport.setLiveOrgan(null);
assert.equal(opened.length, 3);
assert.match(opened[2].url, /\/host\/transport\/ws$/, "and we can come home");
assert.equal(transport.getLiveOrgan(), null);

// A uid is interpolated into a URL path, so it must be escaped.
transport.setLiveOrgan("organ/../../etc");
assert.ok(
  !opened[3].url.includes("/../"),
  `an Organ uid must not be able to walk the path: ${opened[3].url}`,
);
console.log("ok");
"#,
    );
}

/// Frames queued for OUR Cell must never be delivered to theirs.
///
/// This is the failure that would matter: the outbox holds whatever had not
/// been flushed when the switch happened, and the socket it was meant for is
/// gone. Replaying it after the switch would apply someone's half-sent Action
/// to a different Organ's store — a silent cross-Cell write, with no error
/// anywhere.
#[test]
fn work_queued_for_our_cell_is_never_flushed_into_someone_elses() {
    run(
        "outbox",
        r#"
import { opened } from "./harness.mjs";
import assert from "node:assert/strict";
const { getSharedTransport } = await import("./transport.js");

const transport = getSharedTransport();
const ours = opened[0];
// Queue while the socket is not yet reported open, which is what `ready`
// gates on — this is the real path frames take before the open event.
transport.send({ type: "subscribe", id: "q", protein: { source: "record" } });
assert.equal(ours.sent.length, 0, "nothing goes out before the socket opens");

transport.setLiveOrgan("organ-marcia");
const theirs = opened[1];
theirs.fire("open");

assert.equal(
  theirs.sent.length,
  0,
  `a frame addressed to our Cell was flushed into theirs: ${JSON.stringify(theirs.sent)}`,
);
console.log("ok");
"#,
    );
}

/// Switching Cells must not leave a caller waiting on a promise that can never
/// settle: the session those Actions were signed against is gone.
#[test]
fn an_action_in_flight_across_a_switch_is_rejected_not_stranded() {
    run(
        "inflight",
        r#"
import "./harness.mjs";
import assert from "node:assert/strict";
const { getSharedTransport } = await import("./transport.js");

const transport = getSharedTransport();
// No signing session has been established, so this queues awaiting one.
const pending = transport.sendAction("a1", { action: "create-record", head: "x" });
let settled = false;
pending.then(() => { settled = true; }, () => { settled = true; });

transport.setLiveOrgan("organ-marcia");
await new Promise((resolve) => process.nextTick(resolve));
await new Promise((resolve) => process.nextTick(resolve));

assert.ok(settled, "an Action queued for our Cell must settle when we leave it");
console.log("ok");
"#,
    );
}
