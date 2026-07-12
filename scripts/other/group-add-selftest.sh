#!/usr/bin/env bash
# Behavioral verification of the client-side add-as-group flow (Stage 8b, base
# task 2 / kanban Track B): the board store drops a whole sand GROUP (kanban =
# board + record_info) onto the canvas at once, preserving each member's
# relative layout + z-order + ABI listen topics and re-homing the archive's
# inner group id to a FRESH id so repeated adds are independent groups.
#
# Drives board/store.js (+ grid.js + group-logic.js) in headless chromium as ES
# modules — no server boot — feeding it the exact card shape the group endpoint
# returns (camelCase BoardCard).
#
# Requires: chromium on PATH. Usage: scripts/other/group-add-selftest.sh
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

BOARD="$ROOT/crates/web/static/presentation/board"
for f in store.js grid.js group-logic.js; do cp "$BOARD/$f" "$WORK/$f"; done

cat > "$WORK/harness.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"></head><body>
<script type="module">
  import { createBoardStore } from "./store.js";
  import { createGridConfig } from "./grid.js";
  import { innermostGroupId } from "./group-logic.js";

  const config = createGridConfig({});
  const store = createBoardStore({
    seedCards: [], initialBoardState: null, config, persistState: () => {},
  });

  // Exactly the shape the /host/packages/local/group/{filename} endpoint returns
  // (camelCase BoardCard): kanban board (z=1) + record_info (z=2, same rect,
  // listens recordClicked), both sharing one inner group id.
  const groupCards = [
    { id: "card-kanban", kind: "package", title: "Kanban", html: "<!doctype html><p>kanban</p>",
      packageName: "kanban.html", x: 49000, y: 49000, width: 720, height: 520, zIndex: 1,
      groupId: "group-kanban-kanban", groupIds: ["group-kanban-kanban"], abiListen: [] },
    { id: "card-kanban-record-info", kind: "package", title: "Record Info",
      html: "<!doctype html><p>recinfo</p>", packageName: "record-info.html",
      x: 49000, y: 49000, width: 720, height: 520, zIndex: 2,
      groupId: "group-kanban-kanban", groupIds: ["group-kanban-kanban"],
      abiListen: ["recordClicked"] },
  ];

  const results = {};
  const a = store.addImportedGroup(groupCards, { center: { x: 5000, y: 5000 } });
  results.added_two = Array.isArray(a) && a.length === 2;

  const byTitle = (cards, t) => cards.find((c) => c.title === t);
  const board = byTitle(a, "Kanban");
  const info = byTitle(a, "Record Info");

  // Fresh unique card ids (not the archive ids).
  results.fresh_ids = !!board && !!info && board.id !== "card-kanban" &&
    info.id !== "card-kanban-record-info" && board.id !== info.id;
  // Both share ONE fresh inner group id (not the archive's).
  results.shared_fresh_group = !!board && !!info &&
    innermostGroupId(board) === innermostGroupId(info) &&
    !!innermostGroupId(board) && innermostGroupId(board) !== "group-kanban-kanban";
  // Same rect (record_info stacked on the board) and z-order preserved.
  results.same_rect = !!board && !!info && board.x === info.x && board.y === info.y;
  results.z_order = !!board && !!info && info.zIndex > board.zIndex;
  // ABI listen preserved on record_info, absent on the board.
  results.abi_listen = !!info && Array.isArray(info.abiListen) &&
    info.abiListen.includes("recordClicked") &&
    !!board && (board.abiListen || []).length === 0;
  // Placed near the requested center, not at the archive's 49000 coords.
  results.repositioned = !!board && board.x < 49000 && board.y < 49000;
  // HTML carried through so the board can render the sand.
  results.html_carried = !!board && board.html.includes("kanban");

  // A SECOND add is an independent group (fresh id, no collision).
  const b = store.addImportedGroup(groupCards, { center: { x: 6000, y: 6000 } });
  results.second_independent = Array.isArray(b) && b.length === 2 &&
    innermostGroupId(b[0]) !== innermostGroupId(a[0]) &&
    b[0].id !== a[0].id;

  document.title = "RESULT=" + JSON.stringify(results);
</script>
</body></html>
HTML

TITLE="$(cd "$WORK" && timeout 60 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --allow-file-access-from-files --virtual-time-budget=3000 --dump-dom harness.html 2>/dev/null \
  | grep -oE '<title>[^<]*</title>' | sed 's/<[^>]*>//g')"

echo "result: $TITLE"
JSON="${TITLE#RESULT=}"
[ "$JSON" != "$TITLE" ] || { echo "FAIL: harness did not produce a result (module load?)"; exit 1; }

fail=0
check() { grep -q "\"$1\":true" <<<"$JSON" || { echo "FAIL: $2"; fail=1; }; }
check added_two          "adding the group did not create both member cards"
check fresh_ids          "group cards kept the archive ids instead of fresh unique ones"
check shared_fresh_group "group members do not share one FRESH inner group id"
check same_rect          "record_info was not stacked on the board at the same rect"
check z_order            "record_info is not above the board in z-order"
check abi_listen         "recordClicked ABI listen was not preserved on record_info"
check repositioned       "group was not repositioned from the archive coords to the board"
check html_carried       "member sand HTML was not carried through"
check second_independent "a second add was not an independent group (id collision)"

[ "$fail" -eq 0 ] && echo "PASS: add-as-group drops board+record_info as one fresh, scoped, repositioned group" || exit 1
