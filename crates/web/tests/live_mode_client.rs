//! The board reaching OTHER Linces (Ontology §11 "live mode"), guest half.
//!
//! Each sand binds a host — our own Cell, or a contact's, reached through our
//! Cell's iroh relay. Sand A may be looking at Organ A while sand B looks at
//! Organ B, so the board holds one connection PER ORGAN rather than one socket
//! switched between them. This runs the SHIPPED `transport.js` in node to prove
//! the routing and, more importantly, the ways it could silently corrupt
//! someone else's Organ or borrow the wrong identity.

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

    let transport =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static/presentation/board/transport.js");
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
// A clock the test drives by hand, so a reconnect delay is a value that can be
// asserted on rather than a wall-clock wait.
export const timers = [];
let nextTimerId = 1;
function fakeSetTimeout(fn, delay) {
  const id = nextTimerId++;
  timers.push({ id, fn, delay: delay || 0, cancelled: false });
  return id;
}
function fakeClearTimeout(id) {
  const timer = timers.find((entry) => entry.id === id);
  if (timer) timer.cancelled = true;
}
// Run every timer that is still pending, oldest first.
export function tick() {
  const due = timers.filter((entry) => !entry.cancelled && !entry.fired);
  for (const entry of due) {
    entry.fired = true;
    entry.fn();
  }
}
globalThis.window = {
  location: { protocol: "http:", host: "127.0.0.1:6174" },
  setTimeout: fakeSetTimeout,
  clearTimeout: fakeClearTimeout,
  btoa: (s) => Buffer.from(s, "binary").toString("base64"),
  indexedDB: null,
  isSecureContext: false,
  crypto: {},
};
globalThis.setTimeout = fakeSetTimeout;
globalThis.clearTimeout = fakeClearTimeout;
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

/// Two sands on two different hosts get two sockets, each pointed at its own
/// Cell — and a third sand on a host already open reuses that one socket
/// rather than opening a second.
#[test]
fn each_bound_host_gets_its_own_connection() {
    run(
        "route",
        r#"
import { opened } from "./harness.mjs";
import assert from "node:assert/strict";
const { getSharedTransport, getTransportFor, listTransports } = await import("./transport.js");

const ours = getSharedTransport();
assert.equal(opened.length, 1, "our own Cell is one socket");
assert.match(opened[0].url, /\/host\/transport\/ws$/, "and it is the local one");
assert.equal(ours.organ, "");

const marcia = getTransportFor("organ-marcia");
assert.equal(opened.length, 2, "a bound host opens its OWN socket");
assert.equal(
  opened[1].url,
  "ws://127.0.0.1:6174/live/organ-marcia/connect",
  "riding our own Cell's relay, which reaches them over iroh",
);

const joao = getTransportFor("organ-joao");
assert.equal(opened.length, 3, "a second host is a third socket, concurrently");
assert.equal(opened[2].url, "ws://127.0.0.1:6174/live/organ-joao/connect");

// The whole point of per-sand binding: both are live AT THE SAME TIME.
assert.notEqual(marcia, joao);
assert.equal(listTransports().length, 3);

// A sand landing on a host someone else already opened shares the socket.
assert.equal(getTransportFor("organ-marcia"), marcia, "same host, same socket");
assert.equal(opened.length, 3, "and no extra connection");

// A uid is interpolated into a URL path, so it must be escaped.
getTransportFor("organ/../../etc");
assert.ok(
  !opened[3].url.includes("/../"),
  `an Organ uid must not be able to walk the path: ${opened[3].url}`,
);
console.log("ok");
"#,
    );
}

/// Frames queued for one Cell must never be delivered to another.
///
/// The outbox holds whatever had not been flushed when a sand's binding
/// changed. Replaying it onto a different host would apply someone's half-sent
/// Action to the wrong Organ's store — a silent cross-Cell write, with no error
/// anywhere. Separate outboxes are what make that structurally impossible.
#[test]
fn work_queued_for_one_cell_is_never_flushed_into_another() {
    run(
        "outbox",
        r#"
import { opened } from "./harness.mjs";
import assert from "node:assert/strict";
const { getSharedTransport, getTransportFor, releaseTransport } = await import("./transport.js");

const ours = getSharedTransport();
// Queue while the socket is not yet reported open, which is what `ready`
// gates on — this is the real path frames take before the open event.
ours.send({ type: "subscribe", id: "q", protein: { source: "record" } });
assert.equal(opened[0].sent.length, 0, "nothing goes out before the socket opens");

const marcia = getTransportFor("organ-marcia");
opened[1].fire("open");
assert.equal(
  opened[1].sent.length,
  0,
  `a frame addressed to our Cell was flushed into theirs: ${JSON.stringify(opened[1].sent)}`,
);

// And ours still has its own frame waiting for its own socket.
opened[0].fire("open");
assert.equal(opened[0].sent.length, 1, "our frame goes to our Cell, late but correct");

// Releasing a host discards its queue rather than redirecting it.
const joao = getTransportFor("organ-joao");
joao.send({ type: "subscribe", id: "j", protein: { source: "record" } });
releaseTransport("organ-joao");
const reopened = getTransportFor("organ-joao");
assert.notEqual(reopened, joao, "a released host is genuinely gone");
const socketCount = opened.length;
opened[socketCount - 1].fire("open");
assert.equal(
  opened[socketCount - 1].sent.length,
  0,
  "a frame from the old binding must not resurface on the new one",
);
console.log("ok");
"#,
    );
}

/// Signing sessions must not be shared between hosts.
///
/// This is the failure that would matter most. Each Cell binds its own Person
/// to its own challenge, and two sands routinely authenticate as DIFFERENT
/// Persons at the same time. A single shared session would sign an Action for
/// one Cell with the identity proved to another — the Ledger would record the
/// wrong author, and no error would be raised anywhere.
#[test]
fn each_host_keeps_its_own_signing_session() {
    run(
        "signing",
        r#"
import { opened } from "./harness.mjs";
import assert from "node:assert/strict";
const { getSharedTransport, getTransportFor } = await import("./transport.js");

const ours = getSharedTransport();
const marcia = getTransportFor("organ-marcia");

// Our Cell announces a trusted-local session; theirs announces a remote one
// bound to a different Person entirely.
opened[0].fire("message", { data: JSON.stringify({
  type: "session_challenge",
  session_id: "s-local",
  challenge: "c-local",
  algorithm: "ed25519",
  person: null,
  signing_required: false,
}) });
opened[1].fire("message", { data: JSON.stringify({
  type: "session_challenge",
  session_id: "s-marcia",
  challenge: "c-marcia",
  algorithm: "ed25519",
  person: "p-eduardo-on-marcia",
  signing_required: true,
}) });

assert.equal(
  ours.getSigningState().status,
  "trusted-local",
  "our own Cell stays trusted-local",
);
assert.notEqual(
  marcia.getSigningState().status,
  "trusted-local",
  "a remote Cell must never inherit our local trust",
);
assert.equal(
  marcia.getSigningState().person,
  "p-eduardo-on-marcia",
  "and carries the Person THAT Cell bound, not ours",
);
assert.equal(ours.getSigningState().person, null, "which never leaks back the other way");
console.log("ok");
"#,
    );
}

/// A websocket to our OWN Cell opening says nothing about whether the far Cell
/// let us in — the relay confirms that separately, and only then is the session
/// live.
///
/// Reporting the upgrade as "live" is what made the status light flicker green
/// on every failed attempt, and worse, flushed the board's subscriptions into a
/// socket that was about to be closed, destroying them once per retry.
#[test]
fn a_relay_that_never_confirms_is_not_reported_live() {
    run(
        "confirm",
        r#"
import { opened } from "./harness.mjs";
import assert from "node:assert/strict";
const { getTransportFor } = await import("./transport.js");

const marcia = getTransportFor("organ-marcia");
const seen = [];
marcia.onLive((live) => seen.push(live));
marcia.send({ type: "subscribe", id: "s1", protein: { source: "record" } });

// Our own Cell accepted the socket; the far Cell has said nothing yet.
opened[0].fire("open");
assert.deepEqual(seen, [], "an unconfirmed relay is not a live session");
assert.equal(marcia.isReady(), false);
assert.equal(
  opened[0].sent.length,
  0,
  "and the subscription is still queued, not spent on a socket that may die",
);

// The relay reports the far Cell let us in.
opened[0].fire("message", { data: JSON.stringify({ type: "live_ready", organ: "organ-marcia" }) });
assert.deepEqual(seen, [true], "now it is live");
assert.equal(opened[0].sent.length, 1, "and the queued subscription goes out");

// A relay frame is between the board and its own Cell; sands never see it.
const frames = [];
marcia.onMessage((message) => frames.push(message?.type));
opened[0].fire("message", { data: JSON.stringify({ type: "live_ready", organ: "organ-marcia" }) });
assert.deepEqual(frames, [], "relay frames are not passed on to sands");
console.log("ok");
"#,
    );
}

/// A host that will not answer is dialled with a growing delay, not once a
/// second forever.
#[test]
fn a_host_that_keeps_failing_is_dialled_with_a_growing_delay() {
    run(
        "backoff",
        r#"
import { opened, timers, tick } from "./harness.mjs";
import assert from "node:assert/strict";
const { getTransportFor } = await import("./transport.js");

getTransportFor("organ-marcia");

const delays = [];
for (let round = 0; round < 6; round += 1) {
  const socket = opened[opened.length - 1];
  socket.fire("open");   // our Cell accepts...
  socket.close();        // ...and the relay gives up on the far Cell
  const scheduled = timers.filter((entry) => !entry.cancelled && !entry.fired);
  assert.equal(scheduled.length, 1, "exactly one reconnect is pending at a time");
  delays.push(scheduled[0].delay);
  tick();
}

// Jitter makes each delay a range, so compare rounds rather than exact values.
assert.ok(delays[0] <= 1000, `first retry is prompt: ${delays[0]}`);
assert.ok(
  delays[5] > delays[0] * 4,
  `a host that keeps failing must be dialled far less often: ${JSON.stringify(delays)}`,
);
assert.ok(delays.every((delay) => delay <= 30000), "and never beyond the cap");
assert.equal(opened.length, 7, "one dial per round, not one per second");
console.log("ok");
"#,
    );
}

/// A host that lets us in and then immediately drops us backs off too.
///
/// The dangerous half of the backoff. Clearing the failure count the moment a
/// session opens would leave this case dialling once a second and flashing the
/// status light green/red on every round — the original symptom exactly, just
/// one handshake further along.
#[test]
fn a_session_that_dies_on_arrival_does_not_earn_a_prompt_retry() {
    run(
        "unstable",
        r#"
import { opened, timers, tick } from "./harness.mjs";
import assert from "node:assert/strict";
const { getTransportFor } = await import("./transport.js");

const marcia = getTransportFor("organ-marcia");
const delays = [];
for (let round = 0; round < 5; round += 1) {
  const socket = opened[opened.length - 1];
  socket.fire("open");
  // The far Cell admits us...
  socket.fire("message", { data: JSON.stringify({ type: "live_ready", organ: "organ-marcia" }) });
  assert.equal(marcia.isReady(), true, "the session did open");
  // ...and drops us before it was ever worth anything.
  socket.close();
  const pending = timers.filter((entry) => !entry.cancelled && !entry.fired);
  assert.equal(pending.length, 1, "one reconnect pending, and no stray stability timer");
  delays.push(pending[0].delay);
  tick();
}

assert.ok(
  delays[4] > delays[0] * 3,
  `a session that never lasts must not be retried once a second: ${JSON.stringify(delays)}`,
);
console.log("ok");
"#,
    );
}

/// A session that DOES last resets the backoff, so a single blip does not
/// leave a healthy host being dialled every thirty seconds.
#[test]
fn a_session_that_lasts_earns_a_prompt_retry_again() {
    run(
        "stable",
        r#"
import { opened, timers, tick } from "./harness.mjs";
import assert from "node:assert/strict";
const { getTransportFor } = await import("./transport.js");

getTransportFor("organ-marcia");

// Four dials that go nowhere: the delay climbs.
for (let round = 0; round < 4; round += 1) {
  const socket = opened[opened.length - 1];
  socket.fire("open");
  socket.close();
  tick();
}
const climbed = timers.filter((entry) => entry.fired).pop().delay;

// Then a session that opens and stays up past the stability mark.
const good = opened[opened.length - 1];
good.fire("open");
good.fire("message", { data: JSON.stringify({ type: "live_ready", organ: "organ-marcia" }) });
tick(); // the stability verdict falls due
good.close();

const next = timers.filter((entry) => !entry.cancelled && !entry.fired);
assert.equal(next.length, 1);
assert.ok(
  next[0].delay <= 1000,
  `a host that was working is dialled promptly again: ${next[0].delay} after ${climbed}`,
);
console.log("ok");
"#,
    );
}

/// A host that wants a login we do not hold stops being dialled entirely —
/// and starts again the moment the user logs in.
#[test]
fn a_host_that_wants_a_login_we_do_not_have_stops_being_dialled() {
    run(
        "stall",
        r#"
import { opened, timers } from "./harness.mjs";
import assert from "node:assert/strict";
const { getTransportFor } = await import("./transport.js");

const marcia = getTransportFor("organ-marcia");
opened[0].fire("open");
opened[0].fire("message", { data: JSON.stringify({
  type: "live_unavailable",
  code: "live_login_required",
  fatal: true,
  message: "That Lince wants a login. Connect to it and try again.",
}) });
opened[0].close();

assert.equal(
  timers.filter((entry) => !entry.cancelled && !entry.fired).length,
  0,
  "a password we do not have will not arrive by retrying",
);
assert.equal(opened.length, 1, "so the host is not dialled again");
assert.equal(marcia.getStall()?.code, "live_login_required");
assert.match(
  marcia.getSigningState().message,
  /login/i,
  "and the sand can say why it is empty",
);

// The user logs in: trying again must not need a page reload.
marcia.retry();
assert.equal(opened.length, 2, "logging in dials the host again");
assert.equal(marcia.getStall(), null);
console.log("ok");
"#,
    );
}

/// Dropping a host binding must not leave a caller waiting on a promise that
/// can never settle: the session those Actions were signed against is gone.
#[test]
fn an_action_in_flight_when_a_host_is_released_is_rejected_not_stranded() {
    run(
        "inflight",
        r#"
import "./harness.mjs";
import assert from "node:assert/strict";
const { getTransportFor, releaseTransport } = await import("./transport.js");

const marcia = getTransportFor("organ-marcia");
// No signing session has been established, so this queues awaiting one.
const pending = marcia.sendAction("a1", { action: "create-record", head: "x" });
let settled = false;
pending.then(() => { settled = true; }, () => { settled = true; });

releaseTransport("organ-marcia");
await new Promise((resolve) => process.nextTick(resolve));
await new Promise((resolve) => process.nextTick(resolve));

assert.ok(settled, "an Action queued for a released host must settle, not hang");
console.log("ok");
"#,
    );
}
