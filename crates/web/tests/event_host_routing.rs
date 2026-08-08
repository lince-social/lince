//! An event names a record; the record names a Lince (Ontology §11).
//!
//! A record uid on its own is not enough to fetch anything: the same uid names
//! nothing — or something else entirely — on a different Cell. So an ABI event
//! carries the Cell the emitting sand was reading, and a sand acting on that
//! event fetches from THERE rather than from whatever host it is itself bound
//! to. Without it, clicking a row in a kanban pointed at someone else's Lince
//! opened an empty Record panel: the record was never missing, we were asking
//! our own Cell for a uid it had never heard of.
//!
//! This runs the SHIPPED `widget-bridge.js` and `transport.js` in node against
//! fake frames, so what is asserted is the routing the board actually performs.

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

/// Stage the real modules beside a harness faking only the browser globals they
/// touch. Copying the shipped files (never a transcription) is the point — a
/// divergence between this and what is served makes the test worthless.
fn run(label: &str, body: &str) {
    if !node_available() {
        eprintln!("SKIP event host routing test `{label}`: node is not on PATH");
        return;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir: PathBuf =
        std::env::temp_dir().join(format!("lince-abi-{label}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("create dir");

    let board = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static/presentation/board");
    for name in ["transport.js", "widget-bridge.js"] {
        fs::copy(board.join(name), dir.join(name)).unwrap_or_else(|error| {
            panic!("stage {name}: {error}");
        });
    }

    let harness = r#"
// Sockets record instead of connecting, so the test can read back exactly
// which Cell each frame was sent to.
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
  send(data) { this.sent.push(JSON.parse(data)); }
  close() { this.readyState = 3; for (const fn of this.listeners.close || []) fn(); }
  fire(type, event) { for (const fn of this.listeners[type] || []) fn(event); }
}
FakeWebSocket.OPEN = 1;
globalThis.WebSocket = FakeWebSocket;

// The board listens for `message` on window; the test posts as a sand would.
const windowListeners = {};
export function postFromSand(data) {
  for (const fn of windowListeners.message || []) fn({ data });
}
globalThis.window = {
  location: { protocol: "http:", host: "127.0.0.1:6174" },
  setTimeout: () => 0,
  clearTimeout: () => {},
  addEventListener: (type, fn) => { (windowListeners[type] ||= []).push(fn); },
  removeEventListener: () => {},
  btoa: (s) => Buffer.from(s, "binary").toString("base64"),
  indexedDB: null,
  isSecureContext: false,
  crypto: {},
};
globalThis.setTimeout = () => 0;
globalThis.clearTimeout = () => {};

// A sand frame: an id, a host binding, and an inbox of what the board posted.
export function makeFrame(instanceId) {
  const received = [];
  return {
    dataset: { packageInstanceId: instanceId },
    received,
    contentWindow: { postMessage: (message) => received.push(message) },
  };
}

// Which Cell a socket belongs to, read back off the URL the board built.
export function organOf(socket) {
  const match = /\/live\/([^/]+)\/connect$/.exec(socket.url);
  return match ? decodeURIComponent(match[1]) : "";
}
export function socketFor(organ) {
  return opened.find((socket) => organOf(socket) === organ);
}
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
            "event host routing assertions failed ({:?})\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

/// A sand that names a Cell for one request is served by THAT Cell, whatever
/// its own binding says — and the matching unsubscribe goes to the same place.
#[test]
fn a_request_that_names_a_cell_is_sent_to_that_cell() {
    run(
        "route",
        r#"
import { opened, postFromSand, makeFrame, socketFor } from "./harness.mjs";
import assert from "node:assert/strict";
const { createWidgetBridge } = await import("./widget-bridge.js");

// The Record pin: bound to our own Cell, because it has no host of its own.
const record = makeFrame("shell-record");
createWidgetBridge({
  statusNode: null,
  getFrames: () => [record],
  getCardMeta: () => ({ serverId: "" }),
  getCardAbiListen: () => ["recordClicked"],
  getCardGroupStack: () => [],
});

// It was handed a record living on Marcia's Lince.
postFromSand({
  type: "lince:protein-subscribe",
  instanceId: "shell-record",
  subId: "provenance",
  protein: { source: "record", where: [{ uid_eq: "rec-1" }] },
  organ: "organ-marcia",
});

const theirs = socketFor("organ-marcia");
assert.ok(theirs, "the board opened a connection to the Cell the record lives on");

// The relay confirms the far Cell let us in; only then are queued frames sent.
theirs.fire("open");
theirs.fire("message", {
  data: JSON.stringify({ type: "live_ready", organ: "organ-marcia" }),
});

// At least one: the queued frame is flushed on confirmation and the open
// listener replays it, which is how a subscription survives a reconnect.
assert.ok(
  theirs.sent.some(
    (frame) => frame.type === "subscribe" && frame.id.endsWith(":provenance"),
  ),
  `and asked THEM for it: ${JSON.stringify(theirs.sent)}`,
);

const ours = opened.find((socket) => socket.url.endsWith("/host/transport/ws"));
assert.equal(
  ours.sent.filter((frame) => frame.type === "subscribe").length,
  0,
  "our own Cell was never asked for a record it has never heard of",
);

// The unsubscribe must reach the Cell that holds the subscription. Sent
// anywhere else it would leak a live computation on someone else's Lince.
postFromSand({
  type: "lince:protein-unsubscribe",
  instanceId: "shell-record",
  subId: "provenance",
});
assert.equal(
  theirs.sent.filter((frame) => frame.type === "unsubscribe").length,
  1,
  "the unsubscribe follows the subscription",
);
console.log("ok");
"#,
    );
}

/// Reads and writes must agree on which Cell they mean.
///
/// The failure this rules out is the worst one available here: a sand reading a
/// record from another Lince and writing the edit back to its own binding would
/// create or overwrite whatever local record carries that uid, record the change
/// against the wrong Organ, and raise no error anywhere.
#[test]
fn an_action_goes_to_the_cell_the_record_was_read_from() {
    run(
        "write",
        r#"
import { opened, postFromSand, makeFrame, socketFor } from "./harness.mjs";
import assert from "node:assert/strict";
const { createWidgetBridge } = await import("./widget-bridge.js");

const record = makeFrame("shell-record");
createWidgetBridge({
  statusNode: null,
  getFrames: () => [record],
  getCardMeta: () => ({ serverId: "" }), // bound to our own Cell
  getCardAbiListen: () => [],
  getCardGroupStack: () => [],
});

// The sand read the record from Marcia's Lince, which is what opens the
// connection — then that session comes up, as it does before any editing.
postFromSand({
  type: "lince:protein-subscribe",
  instanceId: "shell-record",
  subId: "provenance",
  protein: { source: "record", where: [{ uid_eq: "rec-1" }] },
  organ: "organ-marcia",
});
const theirs = socketFor("organ-marcia");
assert.ok(theirs, "the read reached the Cell the record lives on");
theirs.fire("open");
theirs.fire("message", {
  data: JSON.stringify({ type: "live_ready", organ: "organ-marcia" }),
});
theirs.fire("message", {
  data: JSON.stringify({
    type: "session_challenge",
    session_id: "s1",
    challenge: "c1",
    algorithm: "ed25519",
    person: null,
    signing_required: false,
  }),
});

// Now the user edits the head of that record.
postFromSand({
  type: "lince:action",
  instanceId: "shell-record",
  reqId: "r1",
  action: { action: "update-record", uid: "rec-1", head: "edited" },
  organ: "organ-marcia",
});
await new Promise((resolve) => process.nextTick(resolve));

assert.ok(
  theirs.sent.some((frame) => frame.type === "act"),
  `the edit is written to THEIR Cell: ${JSON.stringify(theirs.sent)}`,
);

const ours = opened.find((socket) => socket.url.endsWith("/host/transport/ws"));
assert.equal(
  ours.sent.filter((frame) => frame.type === "act").length,
  0,
  `and never to ours: ${JSON.stringify(ours.sent)}`,
);
console.log("ok");
"#,
    );
}

/// A Cell kept open only by an event-driven sand must not be closed for looking
/// unused: its host is in no card's binding, and closing it would blank the
/// sand with nothing to say and nothing to re-dial it.
#[test]
fn a_host_held_only_by_a_subscription_is_still_in_use() {
    run(
        "inuse",
        r#"
import { postFromSand, makeFrame } from "./harness.mjs";
import assert from "node:assert/strict";
const { createWidgetBridge } = await import("./widget-bridge.js");

const record = makeFrame("shell-record");
const bridge = createWidgetBridge({
  statusNode: null,
  getFrames: () => [record],
  getCardMeta: () => ({ serverId: "" }),
  getCardAbiListen: () => [],
  getCardGroupStack: () => [],
});

assert.deepEqual([...bridge.hostsInUse()], [], "nothing open yet");

postFromSand({
  type: "lince:protein-subscribe",
  instanceId: "shell-record",
  subId: "provenance",
  protein: { source: "record", where: [{ uid_eq: "rec-1" }] },
  organ: "organ-marcia",
});

assert.deepEqual(
  [...bridge.hostsInUse()],
  ["organ-marcia"],
  "a host no card names is still being read, and must be kept open",
);

// And once the sand lets go, it stops being in use.
postFromSand({
  type: "lince:protein-unsubscribe",
  instanceId: "shell-record",
  subId: "provenance",
});
assert.deepEqual([...bridge.hostsInUse()], [], "released when the sand is done");
console.log("ok");
"#,
    );
}

/// An event carries the Cell its emitter was reading, so the sand that acts on
/// it knows where to look.
#[test]
fn an_event_carries_the_cell_its_subject_lives_on() {
    run(
        "envelope",
        r#"
import { postFromSand, makeFrame } from "./harness.mjs";
import assert from "node:assert/strict";
const { createWidgetBridge } = await import("./widget-bridge.js");

// A kanban reading Marcia's Lince, and the Record pin reading our own.
const kanban = makeFrame("card-kanban");
const record = makeFrame("shell-record");
const bindings = { "card-kanban": "organ-marcia", "shell-record": "" };

createWidgetBridge({
  statusNode: null,
  getFrames: () => [kanban, record],
  getCardMeta: (id) => ({ serverId: bindings[id] ?? "" }),
  getCardAbiListen: (id) => (id === "shell-record" ? ["recordClicked"] : []),
  getCardGroupStack: () => [],
});

// The Record pin announces itself as a new-way frame, which is how the board
// learns to speak the flat protocol to it.
postFromSand({ type: "lince:ready", instanceId: "shell-record" });
// ...and joins the room, exactly as record.html does with `H.joinRoom`.
postFromSand({
  type: "lince:lane-join",
  instanceId: "shell-record",
  room: "recordClicked",
});

// A row is clicked in the kanban.
postFromSand({
  type: "lince:lane-send",
  instanceId: "card-kanban",
  room: "recordClicked",
  payload: { record: { uid: "rec-1" } },
});

const event = record.received.find((message) => message.type === "lince:lane-event");
assert.ok(event, "the Record sand was told about the click");
assert.equal(
  event.organ,
  "organ-marcia",
  `the event names the Cell the record lives on, not the receiver's: ${JSON.stringify(event)}`,
);
// The emitter must not be able to pass itself off as the receiver's own Cell,
// which is what an absent field would silently mean.
assert.notEqual(event.organ, "", "an empty organ would resolve to our own Cell");
console.log("ok");
"#,
    );
}

/// The same host reaches the user's OTHER devices. It rides as a sibling of
/// `payload` on the lane frame, so a board that predates the field sends one
/// without it and reads one straight past it — the payload every existing sand
/// parses is untouched, and nothing has to be upgraded in step.
#[test]
fn the_cell_an_event_names_survives_the_trip_to_another_device() {
    run(
        "wire-envelope",
        r#"
import { postFromSand, makeFrame, opened } from "./harness.mjs";
import assert from "node:assert/strict";
const { createWidgetBridge } = await import("./widget-bridge.js");

const kanban = makeFrame("card-kanban");
const record = makeFrame("shell-record");
const bindings = { "card-kanban": "organ-marcia", "shell-record": "" };

createWidgetBridge({
  statusNode: null,
  getFrames: () => [kanban, record],
  getCardMeta: (id) => ({ serverId: bindings[id] ?? "" }),
  getCardAbiListen: () => [],
  getCardGroupStack: () => [],
});

postFromSand({ type: "lince:ready", instanceId: "shell-record" });
postFromSand({ type: "lince:lane-join", instanceId: "shell-record", room: "recordClicked" });

// Lane traffic always rides our own Cell, even though the click below happens
// in a card bound to Marcia — a room is one Cell's, and this is how our own
// other sessions hear about it.
const ours = opened.find((socket) => socket.url.endsWith("/host/transport/ws"));
assert.ok(ours, "the lane went out on our own Cell");
ours.fire("open", {});

postFromSand({
  type: "lince:lane-send",
  instanceId: "card-kanban",
  room: "recordClicked",
  payload: { record: { uid: "rec-1" } },
});

const mirror = ours.sent.find((frame) => frame.type === "lane_send");
assert.ok(mirror, `the click was mirrored to the lane: ${JSON.stringify(ours.sent)}`);
assert.equal(mirror.organ, "organ-marcia", "and it says whose record it was");
assert.deepEqual(
  mirror.payload,
  { record: { uid: "rec-1" } },
  "the payload keeps the shape every existing sand already parses",
);

// Coming back the other way: a click on the user's phone, landing here.
record.received.length = 0;
ours.fire("message", {
  data: JSON.stringify({
    type: "lane_event",
    room: "recordClicked",
    from: "conn-phone",
    organ: "organ-marcia",
    payload: { record: { uid: "rec-2" } },
  }),
});
const arrived = record.received.find((message) => message.type === "lince:lane-event");
assert.ok(arrived, "the other device's click reached the Record sand");
assert.equal(
  arrived.organ,
  "organ-marcia",
  `it still names Marcia's Cell, not ours: ${JSON.stringify(arrived)}`,
);

// An older board sends no organ at all. That has to keep meaning "this Cell",
// not become undefined halfway into a sand.
ours.fire("message", {
  data: JSON.stringify({
    type: "lane_event",
    room: "recordClicked",
    from: "conn-old",
    payload: { record: { uid: "rec-3" } },
  }),
});
const legacy = record.received.filter((message) => message.type === "lince:lane-event").at(-1);
assert.equal(legacy.organ, "", "a frame without the field means our own Cell");
console.log("ok");
"#,
    );
}
