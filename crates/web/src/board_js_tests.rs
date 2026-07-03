//! Frontend tests for the board grouping logic (marquee selection, group
//! move/resize, group pin, lock persistence). The board JS is written as ES
//! modules, so each test stages the real files as `.mjs` in a temp dir and
//! runs node against them - the same node-driven pattern as the trail sand
//! tests, but with real module imports instead of source concatenation.

use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const GRID_JS: &str = include_str!("../static/presentation/board/grid.js");
const INTERACTIONS_JS: &str = include_str!("../static/presentation/board/interactions.js");
const STORE_JS: &str = include_str!("../static/presentation/board/store.js");
const GROUP_LOGIC_JS: &str = include_str!("../static/presentation/board/group-logic.js");

const PRELUDE: &str = r#"
import assert from "node:assert/strict";
import { MIN_CARD_SIZE, createGridConfig } from "./grid.mjs";
import {
  buildGroupMoveCandidates,
  buildGroupResizeCandidates,
  groupBounds,
} from "./interactions.mjs";
import { createBoardStore } from "./store.mjs";
import {
  buildGroupPinUpdates,
  newGroupId,
  resolveMarqueeGroup,
  selectMarqueeMembers,
} from "./group-logic.mjs";

globalThis.window = globalThis;
globalThis.document = globalThis.document || {
  documentElement: { clientWidth: 1600, clientHeight: 1000 },
};

const config = createGridConfig({
  density: 4,
  world: { width: 10000, height: 10000, snap: 40 },
});

function card(id, x, y, width, height, extra = {}) {
  return {
    id,
    kind: "text",
    title: id,
    description: "d",
    text: "t",
    html: "",
    author: "",
    permissions: [],
    packageName: "",
    requiresServer: false,
    serverId: "",
    viewId: null,
    streamsEnabled: true,
    widgetState: {},
    x,
    y,
    width,
    height,
    pinned: false,
    system: false,
    zIndex: 1,
    groupId: null,
    abiListen: [],
    ...extra,
  };
}

function makeStore(cards) {
  return createBoardStore({
    seedCards: [],
    initialBoardState: {
      schemaVersion: 2,
      density: 4,
      globalStreamsEnabled: true,
      world: { width: 10000, height: 10000, snap: 40 },
      activeWorkspaceId: "space-1",
      workspaces: [
        {
          id: "space-1",
          name: "Area 1",
          camera: { x: 0, y: 0, scale: 1 },
          cards,
        },
      ],
    },
    config,
    persistState: () => {},
  });
}

function byId(cards, id) {
  return cards.find((entry) => entry.id === id);
}
"#;

fn stage_and_run(label: &str, body: &str) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir: PathBuf = std::env::temp_dir().join(format!(
        "lince-board-js-{label}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create board js test dir");

    let modules = [
        ("grid", GRID_JS),
        ("interactions", INTERACTIONS_JS),
        ("store", STORE_JS),
        ("group-logic", GROUP_LOGIC_JS),
    ];
    for (name, source) in modules {
        // The staged copies import each other with .mjs specifiers so node
        // treats them as ES modules without a package.json.
        let rewritten = source.replace(".js\"", ".mjs\"");
        fs::write(dir.join(format!("{name}.mjs")), rewritten).expect("stage board module");
    }

    let script = format!("{PRELUDE}\n{body}\n");
    let test_path = dir.join("test.mjs");
    fs::write(&test_path, script).expect("write board js test");

    let output = Command::new("node")
        .arg(&test_path)
        .current_dir(&dir)
        .output()
        .expect("failed to launch node");

    let _ = fs::remove_dir_all(&dir);

    if output.status.success() {
        return;
    }

    panic!(
        "board js assertions failed (status: {:?})\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn marquee_groups_fully_contained_cards() {
    stage_and_run(
        "marquee-contained",
        r#"
const cards = [
  card("a", 120, 120, 240, 180),
  card("b", 400, 120, 240, 180),
  card("far", 2000, 2000, 240, 180),
];

const group = resolveMarqueeGroup(
  cards,
  { x: 0, y: 0, width: 1000, height: 1000 },
  () => "group-test",
);
assert.ok(group, "marquee over two cards must form a group");
assert.deepStrictEqual(group.cardIds, ["a", "b"]);
assert.strictEqual(group.locked, false);
assert.strictEqual(group.id, "group-test");

// Partially covered cards stay out (fully-contained rule).
const partial = selectMarqueeMembers(cards, { x: 200, y: 0, width: 1000, height: 1000 });
assert.deepStrictEqual(partial.map((entry) => entry.id), ["b"]);

// Pinned and system cards are never group members.
const mixed = [
  card("pinned", 120, 120, 240, 180, { pinned: true }),
  card("system", 120, 400, 240, 180, { system: true }),
  card("normal", 120, 680, 240, 180),
];
const members = selectMarqueeMembers(mixed, { x: 0, y: 0, width: 5000, height: 5000 });
assert.deepStrictEqual(members.map((entry) => entry.id), ["normal"]);

// An empty marquee forms no group.
assert.strictEqual(resolveMarqueeGroup(cards, { x: 9000, y: 9000, width: 100, height: 100 }), null);
"#,
    );
}

#[test]
fn marquee_group_forms_without_crypto_random_uuid() {
    // Regression: crypto.randomUUID only exists in secure contexts, so the
    // marquee must still form a group when it is unavailable (e.g. the app
    // served over plain http on a LAN address).
    stage_and_run(
        "marquee-no-crypto",
        r#"
const originalCrypto = Object.getOwnPropertyDescriptor(globalThis, "crypto");
Object.defineProperty(globalThis, "crypto", { value: undefined, configurable: true });
try {
  const id = newGroupId();
  assert.ok(id.startsWith("group-"), "fallback id must still be generated");

  const group = resolveMarqueeGroup(
    [card("a", 120, 120, 240, 180)],
    { x: 0, y: 0, width: 1000, height: 1000 },
  );
  assert.ok(group, "group must form without crypto.randomUUID");
  assert.ok(group.id.startsWith("group-"));
  assert.deepStrictEqual(group.cardIds, ["a"]);
} finally {
  if (originalCrypto) {
    Object.defineProperty(globalThis, "crypto", originalCrypto);
  }
}
"#,
    );
}

#[test]
fn marquee_reactivates_locked_group_exactly() {
    stage_and_run(
        "marquee-locked",
        r#"
const locked = [
  card("a", 120, 120, 240, 180, { groupId: "group-locked" }),
  card("b", 400, 120, 240, 180, { groupId: "group-locked" }),
];

const reactivated = resolveMarqueeGroup(locked, { x: 0, y: 0, width: 1000, height: 1000 });
assert.strictEqual(reactivated.locked, true);
assert.strictEqual(reactivated.id, "group-locked");

// Selecting the locked pair plus an outsider makes a fresh unlocked group.
const withExtra = [...locked, card("c", 680, 120, 240, 180)];
const fresh = resolveMarqueeGroup(
  withExtra,
  { x: 0, y: 0, width: 2000, height: 1000 },
  () => "group-fresh",
);
assert.strictEqual(fresh.locked, false);
assert.strictEqual(fresh.id, "group-fresh");
assert.deepStrictEqual(fresh.cardIds, ["a", "b", "c"]);
"#,
    );
}

#[test]
fn group_move_applies_delta_to_every_member() {
    stage_and_run(
        "group-move",
        r#"
const origins = [card("a", 400, 400, 240, 180), card("b", 800, 600, 240, 180)];

const moved = buildGroupMoveCandidates(origins, { x: 80, y: 40 }, config);
assert.deepStrictEqual(
  moved.map((entry) => [entry.id, entry.x, entry.y]),
  [["a", 480, 440], ["b", 880, 640]],
);

// Hitting the world edge clamps the delta for the whole group, keeping the
// members' relative offsets intact.
const clamped = buildGroupMoveCandidates(origins, { x: -1000, y: 0 }, config);
const [a, b] = clamped;
assert.strictEqual(a.x, 0);
assert.strictEqual(b.x - a.x, 400);
assert.strictEqual(b.y - a.y, 200);
"#,
    );
}

#[test]
fn group_resize_scales_proportionally_with_min_floor() {
    stage_and_run(
        "group-resize",
        r#"
const origins = [card("a", 400, 400, 480, 360), card("b", 880, 400, 480, 360)];
// Bounding box: 400..1360 x 400..760 (960 x 360).

const grown = buildGroupResizeCandidates(origins, "se", { x: 960, y: 360 }, config);
assert.deepStrictEqual(
  grown.map((entry) => [entry.x, entry.y, entry.width, entry.height]),
  [
    [400, 400, 960, 720],
    [1360, 400, 960, 720],
  ],
);

// Shrinking stops once a member reaches the single-card minimum size, so the
// proportional layout never collapses. Heights land on 200 because the
// per-member clamp snaps to the 40px grid after applying the minimum floor.
const shrunk = buildGroupResizeCandidates(origins, "se", { x: -900, y: -340 }, config);
assert.deepStrictEqual(
  shrunk.map((entry) => [entry.x, entry.y, entry.width, entry.height]),
  [
    [400, 400, 240, 200],
    [640, 400, 240, 200],
  ],
);
for (const entry of shrunk) {
  assert.ok(entry.width >= MIN_CARD_SIZE.width);
  assert.ok(entry.height >= MIN_CARD_SIZE.height);
}
"#,
    );
}

#[test]
fn group_pin_moves_members_to_pinned_band() {
    stage_and_run(
        "group-pin",
        r#"
const cards = [
  card("a", 400, 400, 240, 180, { groupId: "group-l" }),
  card("b", 800, 400, 240, 180, { groupId: "group-l" }),
  card("z", 5000, 5000, 240, 180),
];
const rects = new Map([
  ["a", { left: 120, top: 90 }],
  ["b", { left: 520, top: 90 }],
]);
const canvasRect = { left: 20, top: 10 };

const next = buildGroupPinUpdates(cards, ["a", "b"], rects, canvasRect);

for (const id of ["a", "b"]) {
  const entry = byId(next, id);
  assert.strictEqual(entry.pinned, true, id + " must be pinned");
  assert.strictEqual(entry.groupId, null, "pinning clears the lock");
  assert.strictEqual(entry.zIndex, 89, "pinned members land on top of the pinned band");
}
assert.strictEqual(byId(next, "a").x, 100);
assert.strictEqual(byId(next, "a").y, 80);
assert.strictEqual(byId(next, "b").x, 500);

const untouched = byId(next, "z");
assert.strictEqual(untouched.pinned, false);
assert.strictEqual(untouched.x, 5000);
"#,
    );
}

#[test]
fn store_locks_moves_and_unlocks_groups() {
    stage_and_run(
        "store-group-roundtrip",
        r#"
const store = makeStore([
  card("a", 400, 400, 240, 180),
  card("b", 800, 400, 240, 180),
  card("z", 2000, 2000, 240, 180),
]);

// Lock: groupId lands on every member and survives export (persistence payload).
store.setCardsGroup(["a", "b"], "group-x", { persist: false });
let cards = store.getCards();
assert.strictEqual(byId(cards, "a").groupId, "group-x");
assert.strictEqual(byId(cards, "b").groupId, "group-x");
assert.strictEqual(byId(cards, "z").groupId, null);

const exported = store
  .getSnapshot()
  .boardState.workspaces.find((workspace) => workspace.id === "space-1").cards;
assert.strictEqual(byId(exported, "a").groupId, "group-x");
assert.strictEqual(byId(exported, "b").groupId, "group-x");

// Group move committed through the store keeps membership and offsets.
const members = store.getCards().filter((entry) => ["a", "b"].includes(entry.id));
const moved = buildGroupMoveCandidates(members, { x: 80, y: 40 }, config);
const movedById = new Map(moved.map((entry) => [entry.id, entry]));
store.replaceCards(
  store.getCards().map((entry) => movedById.get(entry.id) || entry),
  { persist: false },
);
cards = store.getCards();
assert.strictEqual(byId(cards, "a").x, 480);
assert.strictEqual(byId(cards, "a").y, 440);
assert.strictEqual(byId(cards, "b").x, 880);
assert.strictEqual(byId(cards, "a").groupId, "group-x", "move must not drop the lock");

// Unlock clears the persisted groupId.
store.setCardsGroup(["a", "b"], null, { persist: false });
cards = store.getCards();
assert.strictEqual(byId(cards, "a").groupId, null);
assert.strictEqual(byId(cards, "b").groupId, null);
"#,
    );
}
