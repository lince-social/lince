//! The vendored loro-wasm, actually executed (Ontology §11 "Collab").
//!
//! The client collab code was written blind and only ever type-checked. This
//! runs the SHIPPED artifact — the same `.wasm` and `.js` served under
//! `/board/vendor/` — in real node, and proves the two things the editor sand
//! depends on:
//!
//!   1. the vendored bundle initializes and produces a working `LoroDoc`;
//!   2. the DELTA protocol converges — export-since-version, not
//!      export-everything, which is what the sand sends on each keystroke.
//!
//! What this does NOT prove: that a sand IFRAME may load it. That depends on
//! the frame's CSP and sandbox flags at runtime, and no header asserted here
//! can stand in for a browser.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const VENDOR: &str = "src/sand/collab/vendor";

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn run(label: &str, body: &str) {
    if !node_available() {
        eprintln!("SKIP collab wasm test `{label}`: node is not on PATH");
        return;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir: PathBuf = std::env::temp_dir().join(format!(
        "lince-collab-{label}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create dir");

    // Copy the vendored bundle verbatim. Rewriting it would mean testing
    // something other than what is served.
    let vendor = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(VENDOR);
    for name in ["loro-index.js", "loro_wasm.js", "loro_wasm_bg.wasm"] {
        fs::copy(vendor.join(name), dir.join(name)).unwrap_or_else(|e| panic!("stage {name}: {e}"));
    }
    // The SHIPPED editor module, not a copy of its logic. If this file and the
    // served one ever diverge, the test is worthless.
    let editor = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("static/presentation/board/collab-editor.js");
    fs::copy(&editor, dir.join("collab-editor.js")).expect("stage collab-editor.js");

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
            "collab wasm assertions failed ({:?})\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

/// The bundle loads and the containers the engine materializes from — `head`
/// and `body` — behave as text.
#[test]
fn the_vendored_bundle_initializes_and_exposes_the_engines_containers() {
    run(
        "init",
        r#"
import init, { LoroDoc } from "./loro-index.js";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

// Node has no `fetch` for a relative file URL, so hand `init` the bytes. A
// browser resolves `loro_wasm_bg.wasm` from `import.meta.url` on its own.
await init({ module_or_path: readFileSync("./loro_wasm_bg.wasm") });

const doc = new LoroDoc();
doc.getText("head").insert(0, "Title");
doc.getText("body").insert(0, "Hello");
doc.commit();

assert.equal(doc.getText("head").toString(), "Title");
assert.equal(doc.getText("body").toString(), "Hello");
"#,
    );
}

/// The protocol the editor sand speaks: export only what changed since the
/// last send, and converge.
///
/// Sending a whole snapshot per keystroke would also converge, which is why
/// this is worth pinning — it works in a two-document test and degrades with
/// document size, the kind of fault that stays invisible until it is
/// expensive.
#[test]
fn a_delta_since_the_last_send_is_enough_to_converge() {
    run(
        "delta",
        r#"
import init, { LoroDoc } from "./loro-index.js";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
await init({ module_or_path: readFileSync("./loro_wasm_bg.wasm") });

const mine = new LoroDoc();
const theirs = new LoroDoc();

mine.getText("body").insert(0, "one");
mine.commit();

// First send: everything so far.
let sent = mine.oplogVersion();
theirs.import(mine.export({ mode: "update" }));
assert.equal(theirs.getText("body").toString(), "one");

// Keep typing, then send ONLY the delta since the last send.
mine.getText("body").insert(3, " two");
mine.commit();
const delta = mine.export({ mode: "update", from: sent });
sent = mine.oplogVersion();
theirs.import(delta);
assert.equal(theirs.getText("body").toString(), "one two");

// The echo: the server pushes the merged doc back to the sender too. Loro
// dedupes by version vector, so re-importing our own work must be a no-op
// rather than duplicating it — otherwise every keystroke would be typed
// twice by the round trip.
const before = mine.getText("body").toString();
mine.import(theirs.export({ mode: "update" }));
assert.equal(mine.getText("body").toString(), before, "the echo must not duplicate");

// Concurrent edits from both sides converge to the same text.
mine.getText("body").insert(0, "A:");
mine.commit();
theirs.getText("body").insert(theirs.getText("body").length, "!");
theirs.commit();
mine.import(theirs.export({ mode: "update" }));
theirs.import(mine.export({ mode: "update" }));
assert.equal(
  mine.getText("body").toString(),
  theirs.getText("body").toString(),
  "concurrent edits must converge",
);
"#,
    );
}

/// The real editor module, driven the way a sand drives it, against the real
/// wasm — two clients typing through a stand-in Cell that behaves like the
/// engine does.
///
/// This is what the structural assertions in the sand's own tests cannot
/// reach: the delta bookkeeping, the guard that stops a remote change being
/// echoed back as if a person typed it, and the caret-preserving write. Those
/// are where the bugs live.
#[test]
fn two_editors_converge_through_a_relay_that_behaves_like_the_cell() {
    run(
        "editor",
        r#"
import init, { LoroDoc } from "./loro-index.js";
import { createCollabEditor } from "./collab-editor.js";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
await init({ module_or_path: readFileSync("./loro_wasm_bg.wasm") });

// btoa/atob exist in modern node, but be explicit rather than depending on it.
globalThis.btoa ??= (s) => Buffer.from(s, "binary").toString("base64");
globalThis.atob ??= (s) => Buffer.from(s, "base64").toString("binary");

// A stand-in for the Cell: it holds the authoritative merged document and,
// like the engine, answers every update by pushing the MERGED doc back to
// everyone — including the sender. That echo is the part most likely to break
// a client, so it is modelled rather than skipped.
const server = new LoroDoc();
const clients = [];
// Delivery can be DEFERRED, because a network has latency and that is exactly
// when concurrency happens: two people type before either update lands.
// Delivering synchronously would only ever test one-at-a-time editing.
let queue = null;
function relay(recordUid, updateBase64) {
  if (queue) { queue.push(updateBase64); return; }
  deliver(updateBase64);
}
function deliver(updateBase64) {
  server.import(Buffer.from(updateBase64, "base64"));
  const merged = server.export({ mode: "snapshot" });
  const b64 = Buffer.from(merged).toString("base64");
  for (const client of clients) client.applyRemote(b64);
}
function holdTheNetwork(fn) {
  queue = [];
  fn();
  const pending = queue;
  queue = null;
  for (const update of pending) deliver(update);
}

function makeClient(name) {
  // A surface with no DOM: exactly what the module is designed against.
  const state = { head: "", body: "" };
  const host = {
    collabJoin: () => () => {},
    collabUpdate: (uid, b64) => relay(uid, b64),
  };
  const editor = createCollabEditor({
    recordUid: "r-1",
    host,
    LoroDoc,
    surface: {
      read: () => ({ head: state.head, body: state.body }),
      write: ({ head, body }) => { state.head = head; state.body = body; },
    },
  });
  return { name, state, editor };
}

const alice = makeClient("alice");
const bob = makeClient("bob");
clients.push(alice.editor, bob.editor);

// Alice types.
alice.state.body = "hello";
alice.editor.localEdit();
assert.equal(bob.state.body, "hello", "bob sees what alice typed");
assert.equal(alice.state.body, "hello", "and the echo does not corrupt alice");

// Bob appends. This is the case that catches a missing `applying` guard: the
// echo reaches alice, whose surface changes, and if that were treated as a
// local edit it would loop or duplicate.
bob.state.body = "hello world";
bob.editor.localEdit();
assert.equal(alice.state.body, "hello world");
assert.equal(bob.state.body, "hello world");

// Concurrent edits, neither having seen the other yet: both commit locally
// while the network is held, then both updates land.
holdTheNetwork(() => {
  alice.state.body = "A hello world";
  alice.editor.localEdit();
  bob.state.body = "hello world B";
  bob.editor.localEdit();
});
assert.equal(
  alice.state.body,
  bob.state.body,
  "concurrent edits must converge to one text on both sides",
);
assert.ok(alice.state.body.includes("A "), "alice's insert survived");
assert.ok(alice.state.body.includes(" B"), "and so did bob's");

// The head container travels too — a card title is edited the same way a body
// is, which is what lets one implementation serve every embed.
alice.state.head = "Shared title";
alice.editor.localEdit();
assert.equal(bob.state.head, "Shared title");

// Nothing was typed, so nothing may be sent: a no-op edit must not produce
// traffic or advance anyone's document.
const before = bob.state.body;
alice.editor.localEdit();
assert.equal(bob.state.body, before, "an edit that changed nothing changes nothing");
"#,
    );
}
