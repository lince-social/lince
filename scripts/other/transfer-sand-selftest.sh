#!/usr/bin/env bash
# Drives the Transfer sand modules in Chromium against a stubbed current bridge.
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SAND="$ROOT/crates/web/src/sand/transfer"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

mkdir -p "$WORK/app"
cp "$SAND/app/"*.js "$WORK/app/"
cp "$SAND/styles.css" "$WORK/styles.css"

cat > "$WORK/sand.html" <<'HTML'
<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><link rel="stylesheet" href="styles.css"></head><body>
<main class="transferApp">
  <header class="toolbar"><div class="titleBlock"><div class="eyebrow">Commitments</div><h1>Transfers</h1></div><div class="toolbarTools"><span id="create-blocker" class="creationBlocker" hidden></span><button id="create-transfer" class="primaryButton">+ New transfer</button><label class="search"><span class="visuallyHidden">Search transfers</span><span aria-hidden="true">⌕</span><input id="search" type="search" placeholder="Search"></label><select id="sort"><option value="attention">Needs attention</option><option value="name">Name</option><option value="status">Status</option></select><span id="live-dot" class="liveDot" data-live="false"></span></div></header>
  <nav id="filters" class="filters"><button type="button" data-filter="all" aria-pressed="true">All</button><button type="button" data-filter="attention" aria-pressed="false">Attention</button><button type="button" data-filter="open" aria-pressed="false">Open</button><button type="button" data-filter="settled" aria-pressed="false">Settled</button></nav>
  <section id="summary" class="summary"></section><div id="loading" class="stateMessage">Loading transfers</div><div id="empty" class="stateMessage" hidden><p id="empty-message">No transfers match this view.</p><button id="empty-create-transfer" class="primaryButton">Create transfer</button></div>
  <section id="workspace" class="workspace" hidden><section class="overview"><div id="transfer-list" class="transferList"></div></section><article id="detail" class="detail" tabindex="-1" hidden></article></section>
  <section id="creator" class="creator" hidden></section>
</main>
<script>
const transferRows = [
  { kind:"transfer_context", viewer:{local:true,recognized:true}, capabilities:{create:true}, blocking_reasons:{create:[]} },
  { uid:"t_milk", slug:"transfer.milk", head:"Weekly milk", status:"broken", active:true, agreement_type:"full", agreement:{reviewed:2,committed:1,required:2,total:2,policy_satisfied:false}, settlement:"manual", visibility:"proximity", max_proximity:2, require_confirmation:true, parent:"t_house", source:null, balanced:false, balance:{food:-2}, parties:[{uid:"party_ana",actor:"person_ana",actor_head:"Ana",level:2},{uid:"party_jo",actor:"person_jo",actor_head:"Jo",level:1}], promises:[{uid:"promise_1",record:"r_milk",record_head:"Milk",record_slug:"food.milk",concept_name:"food",unit_name:"liters",delta:-2,state:"broken",party:"person_ana",window_end:"2026-07-20T12:00:00Z",reserve_from:"active"}], confirmations:[{kind:"delivery",actor:"7",at:"2026-07-18T12:00:00Z",fact:"f_1"}] },
  { uid:"t_bread", slug:"transfer.bread", head:"Bread received", status:"settled", active:true, agreement_type:"full", agreement:{reviewed:1,committed:1,required:1,total:1,policy_satisfied:true}, settlement:"manual", visibility:"hidden", require_confirmation:false, balanced:true, balance:{food:0}, parties:[{uid:"party_mia",actor:"person_mia",actor_head:"Mia",level:2}], promises:[{uid:"promise_2",record:"r_bread",record_head:"Bread",unit_name:"loaves",delta:1,state:"kept",party:"person_mia",reserve_from:"active"}], confirmations:[] }
];
const recordRows = [{uid:"person_ana",kind:"person",head:"Ana"},{uid:"person_jo",kind:"person",head:"Jo"},{uid:"r_milk",kind:"plain",head:"Milk"}];
const calls = { subscriptions:[], patches:[], emits:[], actions:[] };
let liveHandler;
window.__calls = calls;
window.LinceWidgetHost = {
  onLive(callback) { liveHandler = callback; setTimeout(() => callback(true), 0); },
  onCardState(callback) { setTimeout(() => callback({ transfers:{ filter:"all", sort:"attention", selectedUid:"" } }), 0); },
  subscribeProtein(id, protein, callback) { calls.subscriptions.push({id,protein}); setTimeout(() => callback({rows:id === "transfers" ? transferRows : recordRows}), 0); return () => {}; },
  patchCardState(patch) { calls.patches.push(patch); },
  emit(room, payload) { calls.emits.push({room,payload}); },
  act(action) { calls.actions.push(action); return Promise.resolve({created:"t_new"}); }
};
</script><script type="module" src="app/main.js"></script>
</body></html>
HTML

cat > "$WORK/harness.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"><title>pending</title></head><body><iframe id="sand" src="sand.html" style="width:1100px;height:760px;border:0"></iframe><script>
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
(async () => {
  await wait(800);
  const frame = document.getElementById("sand");
  const doc = frame.contentDocument;
  const win = frame.contentWindow;
  const result = {};
  result.transfer_subscription = win.__calls.subscriptions.length === 1 && win.__calls.subscriptions[0].protein.source === "transfer";
  result.live = doc.getElementById("live-dot").dataset.live === "true";
  result.summary = [...doc.querySelectorAll(".metric strong")].map((node) => node.textContent).join(",") === "2,1,1,1";
  result.overview = doc.querySelectorAll(".transferCard").length === 2 && doc.querySelector(".transferCard").textContent.includes("Weekly milk") && doc.querySelector(".transferCard").textContent.includes("Ana, Jo");

  doc.querySelector(".transferCard").click();
  await wait(80);
  const detail = doc.getElementById("detail");
  result.detail = !detail.hidden && detail.textContent.includes("Weekly milk") && detail.textContent.includes("Promises (1)") && detail.textContent.includes("Transfer tree") && detail.textContent.includes("Delivery");
  result.state_patched = win.__calls.patches.some((patch) => patch.transfers?.selectedUid === "t_milk");
  detail.querySelector(".textButton").click();
  result.record_navigation = win.__calls.emits.some((event) => event.room === "recordClicked" && event.payload.record.uid === "r_milk");

  doc.querySelector('[data-filter="settled"]').click();
  await wait(30);
  result.filter = doc.querySelectorAll(".transferCard").length === 1 && doc.querySelector(".transferCard").textContent.includes("Bread received");
  result.creation_ready = !doc.getElementById("create-transfer").disabled && win.__calls.subscriptions.some((call) => call.id === "transfer-records" && call.protein.source === "record");

  frame.style.width = "500px";
  doc.querySelector(".transferCard").click();
  await wait(40);
  result.mobile_detail = getComputedStyle(doc.querySelector(".overview")).display === "none" && !doc.getElementById("detail").hidden;
  document.title = "RESULT=" + JSON.stringify(result);
})();
</script></body></html>
HTML

TITLE="$(cd "$WORK" && timeout 60 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --disable-breakpad --disable-crash-reporter --user-data-dir="$WORK/profile" \
  --allow-file-access-from-files --virtual-time-budget=5000 --dump-dom harness.html \
  | grep -oE '<title>[^<]*</title>' | sed 's/<[^>]*>//g' || true)"

echo "result: $TITLE"
JSON="${TITLE#RESULT=}"
[ "$JSON" != "$TITLE" ] || { echo "FAIL: harness produced no result"; exit 1; }

fail=0
check() { grep -q "\"$1\":true" <<<"$JSON" || { echo "FAIL: $2"; fail=1; }; }
check transfer_subscription "sand did not subscribe once to source=transfer"
check live                  "live connection state was not rendered"
check summary               "portfolio summary counts were incorrect"
check overview              "detailed overview rows were not rendered"
check detail                "selection did not open the high-detail surface"
check state_patched         "selected transfer was not persisted in card state"
check record_navigation     "linked record did not emit recordClicked"
check filter                "settled filter did not narrow the overview"
check creation_ready        "creation capability or Record/Person source was not wired"
check mobile_detail         "mobile selection did not replace the overview"

[ "$fail" -eq 0 ] && echo "PASS: transfer overview + selection + high-detail control view" || exit 1
