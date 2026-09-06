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
const VIEWPORT_JS: &str = include_str!("../static/presentation/board/viewport.js");
const VAULT_JS: &str = include_str!("../static/presentation/board/vault.js");

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
  disbandGroup,
  groupStackOf,
  innermostGroupId,
  newGroupId,
  resolveMarqueeGroup,
  selectMarqueeMembers,
  sharesGroup,
  wrapInGroup,
} from "./group-logic.mjs";
import { createBoardViewport } from "./viewport.mjs";
import { LOCKED_LABEL, VAULT_MARKER, isLocked } from "./vault.mjs";

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

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn stage_and_run(label: &str, body: &str) {
    if !node_available() {
        eprintln!("SKIP board_js test `{label}`: node is not available on PATH");
        return;
    }
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
        ("viewport", VIEWPORT_JS),
        ("vault", VAULT_JS),
    ];
    for (name, source) in modules {
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
fn nested_groups_disband_outer_preserves_inner() {
    stage_and_run(
        "nested-groups",
        r#"
// A kanban sand ships as its own inner group: board + Record.
let cards = [
  card("kanban", 100, 100, 400, 400),
  card("recinfo", 100, 100, 400, 400),
  card("todo", 600, 100, 300, 300),
];
cards = wrapInGroup(cards, ["kanban", "recinfo"], "g-kanban");
assert.deepStrictEqual(groupStackOf(byId(cards, "kanban")), ["g-kanban"]);
assert.strictEqual(innermostGroupId(byId(cards, "kanban")), "g-kanban");

// The user marquees the kanban group together with the todo into an OUTER group.
cards = wrapInGroup(cards, ["kanban", "recinfo", "todo"], "g-outer");
assert.deepStrictEqual(groupStackOf(byId(cards, "kanban")), ["g-outer", "g-kanban"]);
assert.deepStrictEqual(groupStackOf(byId(cards, "todo")), ["g-outer"]);
// groupId mirrors the innermost for flat-group back-compat.
assert.strictEqual(byId(cards, "kanban").groupId, "g-kanban");
assert.strictEqual(byId(cards, "todo").groupId, "g-outer");

// Groupception: disbanding the OUTER group releases todo, but the kanban's
// inner group (board + Record) survives.
cards = disbandGroup(cards, "g-outer");
assert.deepStrictEqual(groupStackOf(byId(cards, "kanban")), ["g-kanban"]);
assert.deepStrictEqual(groupStackOf(byId(cards, "recinfo")), ["g-kanban"]);
assert.deepStrictEqual(groupStackOf(byId(cards, "todo")), []);
assert.strictEqual(byId(cards, "todo").groupId, null);

// Event scoping: kanban and Record still share a group; todo does not.
assert.ok(sharesGroup(byId(cards, "kanban"), byId(cards, "recinfo")));
assert.ok(!sharesGroup(byId(cards, "kanban"), byId(cards, "todo")));

// Disbanding the inner group finally ungroups the kanban pair.
cards = disbandGroup(cards, "g-kanban");
assert.deepStrictEqual(groupStackOf(byId(cards, "kanban")), []);
assert.ok(!sharesGroup(byId(cards, "kanban"), byId(cards, "recinfo")));
"#,
    );
}

#[test]
fn marquee_group_forms_without_crypto_random_uuid() {
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
// proportional layout never collapses. Members land exactly ON the floor —
// 240x180 is MIN_CARD_SIZE — and that is also the point: 480x360 scaled by
// 0.5 is still 4:3, so the group keeps its shape all the way down.
//
// This previously expected a height of 200, explained as the per-member clamp
// "snapping to the 40px grid". `clampCard` does no snapping — it only clamps
// to MIN_CARD_SIZE and the world — and 240x200 would have BROKEN the
// proportionality this test is named for. The maths was right; the
// expectation was not.
const shrunk = buildGroupResizeCandidates(origins, "se", { x: -900, y: -340 }, config);
assert.deepStrictEqual(
  shrunk.map((entry) => [entry.x, entry.y, entry.width, entry.height]),
  [
    [400, 400, 240, 180],
    [640, 400, 240, 180],
  ],
);
// The shape survives the clamp: every member keeps the 4:3 it started with.
for (const entry of shrunk) {
  assert.strictEqual(entry.width / entry.height, 480 / 360);
}
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
fn ctrl_drag_reaches_marquee_instead_of_camera_pan() {
    stage_and_run(
        "viewport-ctrl-drag",
        r#"
const windowListeners = [];
globalThis.addEventListener = (type, handler, options) => {
  windowListeners.push({ type, handler, options });
};
globalThis.removeEventListener = () => {};
globalThis.requestAnimationFrame = (fn) => { fn(); return 1; };

function fakeViewportElement() {
  return {
    addEventListener() {},
    removeEventListener() {},
    getBoundingClientRect() {
      return { left: 0, top: 0, right: 1600, bottom: 1000, width: 1600, height: 1000 };
    },
    contains: () => true,
  };
}

function fakeWorldElement() {
  return {
    style: {},
    addEventListener() {},
    removeEventListener() {},
    dispatchEvent() {},
  };
}

createBoardViewport({
  viewportElement: fakeViewportElement(),
  worldElement: fakeWorldElement(),
  onCameraChanged: () => {},
});

const pointerdownEntry = windowListeners.find((entry) => entry.type === "pointerdown");
assert.ok(pointerdownEntry, "camera pan must register a window pointerdown listener");
const pointerdown = pointerdownEntry.handler;

function pointerEvent(overrides = {}) {
  return {
    button: 0,
    pointerId: 7,
    clientX: 400,
    clientY: 300,
    ctrlKey: false,
    metaKey: false,
    defaultPrevented: false,
    stopped: false,
    target: { closest: () => null },
    preventDefault() { this.defaultPrevented = true; },
    stopPropagation() { this.stopped = true; },
    ...overrides,
  };
}

// Ctrl+drag on empty canvas must fall through to the marquee listener.
const ctrlDown = pointerEvent({ ctrlKey: true });
pointerdown(ctrlDown);
assert.strictEqual(ctrlDown.defaultPrevented, false, "ctrl+pointerdown must not start a camera pan");
assert.strictEqual(ctrlDown.stopped, false, "ctrl+pointerdown must keep propagating to the board");

// Meta behaves the same (mac).
const metaDown = pointerEvent({ metaKey: true, pointerId: 8 });
pointerdown(metaDown);
assert.strictEqual(metaDown.defaultPrevented, false);
assert.strictEqual(metaDown.stopped, false);

// A plain drag still starts the camera pan.
const panDown = pointerEvent({ pointerId: 9 });
pointerdown(panDown);
assert.strictEqual(panDown.defaultPrevented, true, "plain pointerdown still pans the camera");
assert.strictEqual(panDown.stopped, true);
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

#[test]
fn a_locked_description_is_recognised_before_it_is_rendered() {
    stage_and_run(
        "vault-locked-description",
        r#"
const envelope =
  VAULT_MARKER + " m=19456,t=2,p=1 c2FsdHNhbHRzYWx0c2E= " +
  "bm9uY2Vub25jZW5vbmNlbm9uY2Vub24= Y2lwaGVydGV4dA==";

assert.equal(isLocked(envelope), true);
assert.equal(isLocked("an ordinary description"), false);
assert.equal(isLocked(VAULT_MARKER + " not an envelope"), false);
assert.equal(isLocked(null), false);
assert.ok(LOCKED_LABEL.length > 0);
"#,
    );
}
