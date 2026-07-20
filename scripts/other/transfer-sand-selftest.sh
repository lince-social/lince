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
cp -R "$SAND/app/." "$WORK/app/"
cp "$SAND/styles.css" "$WORK/styles.css"

cat > "$WORK/sand.html" <<'HTML'
<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><link rel="stylesheet" href="styles.css"></head><body>
<main class="transferApp">
  <header class="toolbar"><div class="titleBlock"><div class="eyebrow">Commitments</div><h1>Transfers</h1></div><div class="toolbarTools"><span id="create-blocker" class="creationBlocker" hidden></span><button id="create-transfer" class="primaryButton">+ New transfer</button><select id="workflow-preset"><option value="">Use preset...</option><option value="donation">Donation</option><option value="sale">Sale</option><option value="assignment">Assignment</option><option value="group">Group coordination</option><option value="service">Service</option><option value="information">Information</option><option value="dependency">Dependency plan</option><option value="ride">Ride</option><option value="delivery">Delivery</option></select><label class="search"><span class="visuallyHidden">Search transfers</span><span aria-hidden="true">⌕</span><input id="search" type="search" placeholder="Search"></label><select id="sort"><option value="attention">Workflow priority</option><option value="name">Name</option><option value="status">Status</option></select><span id="live-dot" class="liveDot" data-live="false"></span></div></header>
  <section class="inboxControls"><div id="ownership-filters" class="ownershipFilters"><button type="button" data-ownership="all" aria-pressed="true">All</button><button type="button" data-ownership="mine" aria-pressed="false">Mine</button></div><nav id="filters" class="filters"><button type="button" data-workflow="all" aria-pressed="true">Any status</button><button type="button" data-workflow="awaiting_me" aria-pressed="false">Awaiting me</button><button type="button" data-workflow="awaiting_others" aria-pressed="false">Awaiting others</button><button type="button" data-workflow="active" aria-pressed="false">Active</button><button type="button" data-workflow="completed" aria-pressed="false">Completed</button><button type="button" data-workflow="cancelled_or_broken" aria-pressed="false">Cancelled / broken</button><button type="button" data-workflow="discoverable_open" aria-pressed="false">Discoverable OPEN</button></nav><span id="view-modes" class="viewModes"><button type="button" data-view="list" aria-pressed="true">List</button><button type="button" data-view="tree" aria-pressed="false">Tree</button></span></section>
  <section id="summary" class="summary"></section><section id="inbox-notice" class="inboxNotice" hidden><span id="inbox-notice-message"></span><button id="retry-transfers" type="button" hidden>Retry</button></section><div id="loading" class="stateMessage">Loading transfers</div><div id="empty" class="stateMessage" hidden><p id="empty-message">No transfers match this view.</p><button id="empty-create-transfer" class="primaryButton">Create transfer</button><button id="clear-transfer-filters" type="button" hidden>Clear filters</button></div>
  <section id="workspace" class="workspace" hidden><section class="overview"><div id="transfer-list" class="transferList"></div></section><article id="detail" class="detail" tabindex="-1" hidden></article></section>
  <section id="creator" class="creator" hidden></section>
</main>
<script>
const transferRows = [
  { kind:"transfer_context", viewer:{local:true,recognized:true}, capabilities:{create:true}, blocking_reasons:{create:[]} },
  { uid:"t_milk", revision:3, slug:"transfer.milk", head:"Weekly milk", status:"broken", active:true, inbox_facets:{mine:true,awaiting_me:true,awaiting_others:false,active:true,completed:false,cancelled_or_broken:true,discoverable_open:false}, agreement_type:"full", agreement:{reviewed:2,committed:1,required:2,total:2,policy_satisfied:false,history:[]}, readiness:{ready:false,blockers:[{code:"party_agreement_required",person:"person_jo"}]}, settlement:"manual", visibility:"proximity", max_proximity:2, require_confirmation:true, parent:"t_house", source:null, balanced:false, balance:{food:-2}, capabilities:{create_message:true,agreement_back:true}, parties:[{uid:"party_ana",actor:"person_ana",actor_head:"Ana",level:2,capabilities:{agreement_back:true}},{uid:"party_jo",actor:"person_jo",actor_head:"Jo",level:1,capabilities:{commit:true,agreement_back:true}}], promises:[{uid:"promise_1",record:"r_milk",record_head:"Milk",record_slug:"food.milk",concept_name:"food",unit_name:"liters",delta:-2,state:"broken",party:"person_ana",window_end:"2026-07-20T12:00:00Z",reserve_from:"active"}], confirmations:[{kind:"delivery",actor:"7",at:"2026-07-18T12:00:00Z",fact:"f_1"}], threads:[{uid:"thread_terms",head:"Terms",capabilities:{create_message:true},messages:[{uid:"message_receipt",body:"Receipt",references:[{uid:"r_receipt",slug:"receipt.july",head:"July receipt",kind:"plain",body:"https://example.invalid/receipt"}]}]}] },
  { uid:"t_bread", slug:"transfer.bread", head:"Bread received", status:"settled", active:true, inbox_facets:{mine:true,awaiting_me:false,awaiting_others:false,active:false,completed:true,cancelled_or_broken:false,discoverable_open:false}, agreement_type:"full", agreement:{reviewed:1,committed:1,required:1,total:1,policy_satisfied:true}, settlement:"manual", visibility:"hidden", require_confirmation:false, balanced:true, balance:{food:0}, parties:[{uid:"party_mia",actor:"person_mia",actor_head:"Mia",level:2}], promises:[{uid:"promise_2",record:"r_bread",record_head:"Bread",unit_name:"loaves",delta:1,state:"kept",party:"person_mia",reserve_from:"active"}], confirmations:[] }
];
const recordRows = [{uid:"person_ana",kind:"person",head:"Ana"},{uid:"person_jo",kind:"person",head:"Jo"},{uid:"r_milk",kind:"plain",head:"Milk"},{uid:"r_receipt",kind:"plain",head:"July receipt"}];
const calls = { subscriptions:[], patches:[], emits:[], actions:[], callbacks:{} };
let liveHandler;
window.__calls = calls;
window.__transferRows = transferRows;
window.__errors = [];
window.addEventListener("error", (event) => window.__errors.push(String(event.error?.stack || event.message)));
window.addEventListener("unhandledrejection", (event) => window.__errors.push(String(event.reason?.stack || event.reason)));
window.LinceWidgetHost = {
  onLive(callback) { liveHandler = callback; setTimeout(() => callback(true), 0); },
  onCardState(callback) { setTimeout(() => callback({ transfers:{ filter:"all", sort:"attention", selectedUid:"" } }), 0); },
  subscribeProtein(id, protein, callback) { calls.subscriptions.push({id,protein}); calls.callbacks[id] = callback; setTimeout(() => callback({rows:id === "transfers" ? transferRows : recordRows}), 0); return () => {}; },
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
  const result = {};
  try {
  await wait(800);
  const frame = document.getElementById("sand");
  const doc = frame.contentDocument;
  const win = frame.contentWindow;
  result.module_errors = win.__errors;
  result.transfer_subscription = win.__calls.subscriptions.some((call) => call.id === "transfers" && call.protein.source === "transfer");
  result.live = doc.getElementById("live-dot").dataset.live === "true";
  result.summary = [...doc.querySelectorAll(".metric strong")].map((node) => node.textContent).join(",") === "2,1,1,1";
  result.overview = doc.querySelectorAll(".transferCard").length === 2 && doc.querySelector(".transferCard").textContent.includes("Weekly milk") && doc.querySelector(".transferCard").textContent.includes("Ana, Jo");

  doc.querySelector(".transferCard").click();
  await wait(80);
  const detail = doc.getElementById("detail");
  result.detail = !detail.hidden && detail.textContent.includes("Weekly milk") && detail.textContent.includes("Promises (1)") && detail.textContent.includes("Transfer tree") && detail.textContent.includes("Delivery");
  result.state_patched = win.__calls.patches.some((patch) => patch.transfers?.selectedUid === "t_milk");
  [...detail.querySelectorAll(".textButton")].find((button) => button.textContent === "Open record")?.click();
  result.record_navigation = win.__calls.emits.some((event) => event.room === "recordClicked" && event.payload.record.uid === "r_milk");
  detail.querySelector(".messageReferences .referenceButton").click();
  result.message_reference_navigation = win.__calls.emits.some((event) => event.room === "recordClicked" && event.payload.record.uid === "r_receipt");
  const messageForm = detail.querySelector(".messageForm");
  messageForm.querySelector("textarea").value = "Attached receipt";
  messageForm.querySelector(".messageReferenceSelect option[value='r_receipt']").selected = true;
  messageForm.dispatchEvent(new Event("submit", {bubbles:true,cancelable:true}));
  await wait(30);
  result.message_reference_action = win.__calls.actions.some((action) => action.action === "create-message" && action.references?.[0] === "r_receipt");

  const agreementButton = [...detail.querySelectorAll(".agreementLevelControl button")].find((button) => button.textContent.includes("Back to"));
  agreementButton?.click();
  await wait(30);
  const agreementAction = win.__calls.actions.find((action) => action.action === "set-transfer-agreement-level");
  result.agreement_action = agreementAction?.transfer === "t_milk" && agreementAction?.level === 1 && agreementAction?.person === "person_ana";
  result.agreement_waits_for_protein = [...detail.querySelectorAll("button")].some((button) => button.textContent === "Waiting for live agreement");
  const milk = win.__transferRows.find((row) => row.uid === "t_milk");
  milk.agreement.history.push({uid:"agreement_event",person:"person_ana",from_level:2,to_level:1,request_id:agreementAction.request_id,revision:3});
  milk.parties[0].level = 1;
  milk.parties[0].capabilities = {commit:true,agreement_back:true};
  win.__calls.callbacks.transfers({rows:win.__transferRows});
  await wait(40);
  result.agreement_settles_on_protein = ![...detail.querySelectorAll("button")].some((button) => button.textContent === "Waiting for live agreement");

  doc.querySelector('[data-workflow="completed"]').click();
  await wait(30);
  result.filter = doc.querySelectorAll(".transferCard").length === 1 && doc.querySelector(".transferCard").textContent.includes("Bread received");
  result.creation_ready = !doc.getElementById("create-transfer").disabled && win.__calls.subscriptions.some((call) => call.id === "transfer-records" && call.protein.source === "record");

  const presetsModule = await import(new URL("app/presets.js", win.location.href));
  const modelModule = await import(new URL("app/create-model.js", win.location.href));
  const expectedPresets = ["donation","sale","assignment","group","service","information","dependency","ride","delivery"];
  const presetActions = [];
  let requiredPeople = 0;
  for (const presetId of expectedPresets) {
    const draft = modelModule.emptyDraft({local:true});
    draft.creator = "person_ana";
    const preset = presetsModule.applyWorkflowPreset(draft, presetId, {
      makePromise: modelModule.emptyPromise,
      makeDependency: modelModule.emptyDependency,
      recordUids: ["r_milk", "r_receipt"],
      records: [{uid:"r_milk",kind:"plain",head:"Milk"},{uid:"r_receipt",kind:"plain",head:"July receipt"}],
    });
    if (presetsModule.validatePresetPeople(draft, preset)) requiredPeople += 1;
    draft.invitees = ["person_jo"];
    presetsModule.bindPresetParties(draft, preset);
    for (const dependency of draft.dependencies) dependency.upstream = "t_milk";
    const errors = [0,1,2,3].map((step) => modelModule.validateDraftStep(draft, step)).filter(Boolean);
    const action = modelModule.toCreateTransferAction(draft);
    presetActions.push(Boolean(preset) && errors.length === 0 && !JSON.stringify(action).includes("preset"));
  }
  result.preset_catalog_actions = presetsModule.WORKFLOW_PRESETS.map((preset) => preset.id).join(",") === expectedPresets.join(",") && requiredPeople === 6 && presetActions.every(Boolean);

  doc.getElementById("workflow-preset").value = "donation";
  doc.getElementById("workflow-preset").dispatchEvent(new Event("change", {bubbles:true}));
  await wait(30);
  const creator = doc.getElementById("creator");
  result.preset_compact_flow = !creator.hidden && creator.querySelectorAll(".stepItem").length === 2 && creator.textContent.includes("Donation");
  creator.querySelector('[aria-label="Close transfer creator"]')?.click();

  frame.style.width = "500px";
  doc.querySelector(".transferCard").click();
  await wait(40);
  result.mobile_detail = getComputedStyle(doc.querySelector(".overview")).display === "none" && !doc.getElementById("detail").hidden;
  document.title = "RESULT=" + JSON.stringify(result);
  } catch (error) {
    result.harness_error = String(error?.stack || error);
    document.title = "RESULT=" + JSON.stringify(result);
  }
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
check message_reference_navigation "message Record reference did not open through recordClicked"
check message_reference_action "message Record references were not sent through the typed Action"
check agreement_action "agreement control did not send the typed Person-scoped Action"
check agreement_waits_for_protein "agreement control claimed success before pushed Protein evidence"
check agreement_settles_on_protein "agreement control did not settle after pushed request evidence"
check filter                "settled filter did not narrow the overview"
check creation_ready        "creation capability or Record/Person source was not wired"
check preset_catalog_actions "a workflow preset did not produce a valid ordinary draft Action"
check preset_compact_flow   "workflow preset did not open the compact two-step composer"
check mobile_detail         "mobile selection did not replace the overview"

[ "$fail" -eq 0 ] && echo "PASS: transfer overview + selection + high-detail control view" || exit 1
