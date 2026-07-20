import {
  filterAndSort,
  inboxCounts,
  normalizeTransfer,
  summarize,
} from "./model.js";
import { renderDetail, renderList, renderSummary } from "./render.js";
import { createTransferComposer } from "./create.js";
import { syncSettlementPreviewSubscriptions } from "./occurrence.js";
import { bulkCompletionScope, syncBulkCompletionPreview } from "./bulk.js";
import { renderHierarchyOverview } from "./hierarchy.js";

const H = window.LinceWidgetHost;
const elements = {
  detail: byId("detail"),
  empty: byId("empty"),
  emptyMessage: byId("empty-message"),
  emptyCreate: byId("empty-create-transfer"),
  emptyClear: byId("clear-transfer-filters"),
  filters: byId("filters"),
  ownershipFilters: byId("ownership-filters"),
  list: byId("transfer-list"),
  liveDot: byId("live-dot"),
  loading: byId("loading"),
  notice: byId("inbox-notice"),
  noticeMessage: byId("inbox-notice-message"),
  retry: byId("retry-transfers"),
  search: byId("search"),
  sort: byId("sort"),
  summary: byId("summary"),
  viewModes: byId("view-modes"),
  workspace: byId("workspace"),
  create: byId("create-transfer"),
  preset: byId("workflow-preset"),
  createBlocker: byId("create-blocker"),
  creator: byId("creator"),
};

let rows = [];
let records = [];
let units = [];
let context = null;
let pendingUid = "";
let pendingRevision = null;
let loading = true;
let live = false;
let everLive = false;
let awaitingFreshSnapshot = false;
let transferIssue = null;
let actionWarnings = [];
let unsubscribeTransfers = null;
let ui = normalizeUi(null);
let bridgeStateReceived = false;
const actionStates = new Map();

const composer = createTransferComposer(elements.creator, {
  host: H,
  onCreated: (uid, result) => {
    const warnings = normalizeWarnings(result?.warnings);
    if (warnings.length) actionWarnings = warnings;
    if (rows.some((row) => row.uid === uid)) {
      composer.complete();
      selectTransfer(uid);
      return;
    }
    pendingUid = uid;
  },
  onSubmitted: ({ uid, expectedRevision, requestId, result }) => {
    const warnings = normalizeWarnings(result?.warnings);
    if (warnings.length) actionWarnings = warnings;
    const projected = rows.find((row) => row.uid === uid);
    if (requestId && projected
      && containsEvidenceValue(projected, requestId, new Set(["request_id", "idempotency_key"]))) {
      composer.complete();
      selectTransfer(uid);
      return;
    }
    pendingRevision = { uid, after: Number(expectedRevision), requestId: requestId || null };
  },
});

elements.create.addEventListener("click", () => openCreator());
elements.preset.addEventListener("change", () => {
  const preset = elements.preset.value;
  elements.preset.value = "";
  if (preset) openCreator({ preset });
});
elements.emptyCreate.addEventListener("click", () => openCreator());
elements.emptyClear.addEventListener("click", clearInboxFilters);
elements.retry.addEventListener("click", subscribeTransfers);

elements.search.addEventListener("input", () => {
  ui.search = elements.search.value;
  render();
});
elements.sort.addEventListener("change", () => {
  ui.sort = elements.sort.value;
  persistUi();
});
elements.ownershipFilters.addEventListener("click", (event) => {
  const target = event.target.closest("button[data-ownership]");
  if (!target) return;
  ui.ownership = target.dataset.ownership;
  persistUi();
});
elements.filters.addEventListener("click", (event) => {
  const target = event.target.closest("button[data-workflow]");
  if (!target) return;
  ui.workflow = target.dataset.workflow;
  persistUi();
});
for (const [root, selector] of [
  [elements.ownershipFilters, "button[data-ownership]"],
  [elements.filters, "button[data-workflow]"],
  [elements.viewModes, "button[data-view]"],
]) {
  root.addEventListener("keydown", (event) => moveSegmentFocus(event, root, selector));
}
elements.viewModes.addEventListener("click", (event) => {
  const target = event.target.closest("button[data-view]");
  if (!target) return;
  ui.view = target.dataset.view;
  persistUi();
});
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && ui.selectedUid) selectTransfer("");
});

H?.onLive?.((isLive) => {
  const nextLive = Boolean(isLive);
  if (nextLive && !live && rows.length) awaitingFreshSnapshot = true;
  if (!nextLive && rows.length) awaitingFreshSnapshot = true;
  live = nextLive;
  if (live) everLive = true;
  elements.liveDot.dataset.live = String(live);
  elements.liveDot.setAttribute("aria-label", live ? "Connected" : everLive ? "Reconnecting" : "Offline");
  elements.liveDot.title = live ? "Connected" : everLive ? "Reconnecting" : "Offline";
  render();
});

H?.onCardState?.((cardState) => {
  bridgeStateReceived = true;
  const incoming = normalizeUi(cardState?.transfers);
  incoming.search = ui.search;
  ui = incoming;
  syncControls();
  render();
});

function subscribeTransfers() {
  unsubscribeTransfers?.();
  unsubscribeTransfers = null;
  transferIssue = null;
  awaitingFreshSnapshot = rows.length > 0;
  if (!rows.length) loading = true;
  if (typeof H?.subscribeProtein !== "function") {
    loading = false;
    transferIssue = {
      kind: "validation",
      message: "This host cannot subscribe to the Transfer Protein source.",
    };
    render();
    return;
  }
  unsubscribeTransfers = H.subscribeProtein(
    "transfers",
    { source: "transfer", limit: 500 },
    (snapshot = {}) => {
      if (snapshot.error) {
        loading = false;
        transferIssue = proteinIssue(snapshot);
        render();
        return;
      }
      const incoming = snapshot.rows;
      const projected = Array.isArray(incoming) ? incoming : [];
      context = projected.find((row) => row?.kind === "transfer_context") || null;
      rows = projected.filter((row) => row?.kind !== "transfer_context").map(normalizeTransfer);
      transferIssue = context ? null : {
        kind: "validation",
        message: "The Transfer snapshot did not include its viewer context.",
      };
      const snapshotWarnings = normalizeWarnings(snapshot.warnings);
      if (snapshotWarnings.length) actionWarnings = snapshotWarnings;
      awaitingFreshSnapshot = false;
      settleActionStates();
      loading = false;
      if (ui.selectedUid && !rows.some((row) => row.uid === ui.selectedUid)) ui.selectedUid = "";
      syncComposerOptions();
      for (const row of rows) composer.updateProjection(row);
      if (pendingUid && rows.some((row) => row.uid === pendingUid)) {
        const createdUid = pendingUid;
        pendingUid = "";
        composer.complete();
        selectTransfer(createdUid);
        return;
      }
      if (pendingRevision) {
        const revised = rows.find((row) => row.uid === pendingRevision.uid
          && (pendingRevision.requestId
            ? containsEvidenceValue(row, pendingRevision.requestId, new Set([
              "request_id",
              "idempotency_key",
            ]))
            : row.revision > pendingRevision.after));
        if (revised) {
          const revisedUid = revised.uid;
          pendingRevision = null;
          composer.complete();
          selectTransfer(revisedUid);
          return;
        }
      }
      render();
    },
  );
  render();
}

subscribeTransfers();

H?.subscribeProtein?.(
  "transfer-units",
  { source: "concept", limit: 2000 },
  ({ rows: incoming }) => {
    units = Array.isArray(incoming) ? incoming : [];
    syncComposerOptions();
  },
);

const handleCreateRequest = (payload) => {
  const value = payload?.data && typeof payload.data === "object" ? payload.data : payload;
  openCreator(value && typeof value === "object" ? value : {});
};
H?.onEvent?.("transferCreate", handleCreateRequest);
if (typeof H?.onLane === "function") {
  H.joinRoom?.("transferCreate");
  H.onLane("transferCreate", handleCreateRequest);
}

H?.subscribeProtein?.(
  "transfer-records",
  { source: "record", limit: 2000 },
  ({ rows: incoming }) => {
    records = Array.isArray(incoming) ? incoming : [];
    syncComposerOptions();
  },
);

syncControls();
render();

function render() {
  const viewer = context?.viewer || null;
  const mutationsEnabled = live && !awaitingFreshSnapshot && !transferIssue;
  const filtered = filterAndSort(rows, {
    ownership: ui.ownership,
    workflow: ui.workflow,
  }, ui.search, ui.sort, viewer);
  const selected = rows.find((row) => row.uid === ui.selectedUid) || null;
  const bulkScope = mutationsEnabled
    ? bulkCompletionScope(selected, rows, { viewer: context?.viewer })
    : null;
  syncBulkCompletionPreview(bulkScope, render);
  syncSettlementPreviewSubscriptions(
    mutationsEnabled ? selected?.occurrences
      .filter((occurrence) => occurrence.settlement_preview)
      .map((occurrence) => occurrence.uid) || [] : [],
  );
  const ownershipRows = filterAndSort(rows, {
    ownership: ui.ownership,
    workflow: "all",
  }, "", "name", viewer);
  renderSummary(elements.summary, summarize(ownershipRows, viewer));
  if (ui.view === "tree") {
    renderHierarchyOverview(elements.list, rows, filtered, {
      branchUid: ui.branchUid || ui.selectedUid,
      rootUid: ui.treeRootUid,
    }, {
      onRefresh: render,
      onRoot: (uid) => {
        ui.treeRootUid = uid;
        if (uid) selectBranch(uid);
        else persistUi();
      },
      onSelect: selectBranch,
      viewer,
    });
  } else {
    elements.list.classList.remove("hierarchyOverview");
    renderList(elements.list, filtered, ui.selectedUid, selectTransfer, viewer);
  }
  renderDetail(elements.detail, selected, {
    onBack: () => selectTransfer(""),
    onOpenRecord: openRecord,
    onEdit: () => composer.showEdit(selected),
    onCounteroffer: (row) => composer.showCounteroffer(row),
    onClaim: (row, promise) => composer.showClaim(row, promise),
    onAction: (key, action) => performLiveAction(key, selected, action),
    onBulkAction: (key, action, scopeRows) => performLiveAction(key, selected, action, scopeRows),
    onBulkSelectionChange: render,
    onSelectBranch: selectBranch,
    actionState: (key) => actionStates.get(key) || null,
    onSettlementPreviewChange: render,
    people: personOptions(),
    records: referenceRecordOptions(),
    viewer: context?.viewer,
    mutationsEnabled,
    hierarchyRows: rows,
  });
  composer.setMutationsEnabled(mutationsEnabled);

  const blockingIssue = transferIssue && rows.length === 0;
  elements.loading.hidden = !loading && !blockingIssue;
  elements.loading.textContent = blockingIssue
    ? transferIssue.message
    : live ? "Loading transfers" : everLive ? "Reconnecting to the Cell" : "Offline. Waiting to connect to the Cell";
  elements.loading.dataset.state = blockingIssue ? transferIssue.kind : live ? "loading" : "offline";
  elements.workspace.hidden = loading || rows.length === 0 || (filtered.length === 0 && !selected);
  elements.workspace.setAttribute("aria-busy", String(loading || awaitingFreshSnapshot));
  elements.empty.hidden = loading || blockingIssue || filtered.length > 0 || Boolean(selected);
  elements.emptyMessage.textContent = rows.length
    ? "No transfers match the current ownership, workflow, and search filters."
    : "No transfers yet.";
  elements.emptyCreate.hidden = rows.length > 0 || !canCreate();
  elements.emptyClear.hidden = rows.length === 0 || !hasInboxFilters();
  elements.workspace.dataset.detailOpen = String(Boolean(selected));
  syncCreateCapability();
  syncInboxCounts(inboxCounts(rows, ui.ownership, viewer));
  syncInboxNotice();
  for (const button of elements.ownershipFilters.querySelectorAll("button[data-ownership]")) {
    button.setAttribute("aria-pressed", String(button.dataset.ownership === ui.ownership));
  }
  for (const button of elements.filters.querySelectorAll("button[data-workflow]")) {
    button.setAttribute("aria-pressed", String(button.dataset.workflow === ui.workflow));
  }
  for (const button of elements.viewModes.querySelectorAll("button[data-view]")) {
    button.setAttribute("aria-pressed", String(button.dataset.view === ui.view));
  }
}

function openCreator(prefill = {}) {
  if (!canCreate()) return;
  selectTransfer("");
  composer.show(prefill);
}

function syncComposerOptions() {
  const people = personOptions();
  const promiseRecords = records.filter((record) => !["transfer", "person", "protein", "sand", "thread", "message"].includes(record?.kind));
  composer.setOptions({ people, records: promiseRecords, units, transfers: rows, viewer: context?.viewer });
}

function personOptions() {
  return records.filter((record) => record?.kind === "person");
}

function referenceRecordOptions() {
  return records.filter((record) => record?.kind === "plain");
}

async function performLiveAction(key, row, action, scopeRows = null) {
  if (!live || awaitingFreshSnapshot || transferIssue
    || !row?.uid || actionStates.get(key)?.busy || actionStates.get(key)?.waiting) return;
  const state = {
    busy: true,
    waiting: false,
    error: "",
    transferUid: row.uid,
    action,
    requestId: action.request_id || null,
    baseline: actionTargetFingerprint(row, action),
    scopeUids: scopeRows?.map((item) => item.uid) || null,
    observed: false,
    createdUid: null,
  };
  actionStates.set(key, state);
  render();
  try {
    const result = await H.act(action);
    state.busy = false;
    state.waiting = true;
    state.createdUid = result?.created || null;
    state.warnings = normalizeWarnings(result?.warnings);
    if (state.warnings.length) actionWarnings = state.warnings;
    if (state.observed || actionProjectionObserved(state)) actionStates.delete(key);
  } catch (cause) {
    state.busy = false;
    state.waiting = false;
    state.error = cause instanceof Error ? cause.message : "The transfer action failed.";
  }
  render();
}

function settleActionStates() {
  for (const [key, state] of actionStates) {
    if (!actionProjectionObserved(state)) continue;
    if (state.busy) state.observed = true;
    else if (state.waiting) actionStates.delete(key);
  }
}

function actionProjectionObserved(state) {
  const scope = state.scopeUids?.length
    ? state.scopeUids.map((uid) => rows.find((row) => row.uid === uid)).filter(Boolean)
    : rows.filter((row) => row.uid === state.transferUid);
  if (state.requestId) {
    return scope.some((row) => containsEvidenceValue(row, state.requestId, new Set([
      "request_id",
      "idempotency_key",
    ])));
  }
  if (state.createdUid) {
    return scope.some((row) => containsEvidenceValue(row, state.createdUid, new Set(["uid"])));
  }
  const current = scope.find((row) => row.uid === state.transferUid);
  return state.baseline != null
    && current != null
    && actionTargetFingerprint(current, state.action) !== state.baseline;
}

function actionTargetFingerprint(row, action) {
  if (action?.action === "create-message" || action?.action === "create-transfer-message") {
    const thread = row?.threads?.find((candidate) => candidate.uid === action.thread);
    return JSON.stringify((thread?.messages || []).map((message) => message.uid));
  }
  if (action?.action === "create-thread" || action?.action === "create-transfer-thread") {
    return JSON.stringify((row?.threads || []).map((thread) => thread.uid));
  }
  return null;
}

function containsEvidenceValue(value, expected, keys) {
  if (!value || typeof value !== "object") return false;
  if (Array.isArray(value)) return value.some((item) => containsEvidenceValue(item, expected, keys));
  for (const [key, child] of Object.entries(value)) {
    if (keys.has(key) && String(child) === String(expected)) return true;
    if (child && typeof child === "object" && containsEvidenceValue(child, expected, keys)) return true;
  }
  return false;
}

function syncCreateCapability() {
  const enabled = canCreate();
  const blocker = creationBlocker();
  elements.createBlocker.hidden = enabled || !context;
  elements.createBlocker.textContent = enabled ? "" : blocker;
  for (const button of [elements.create, elements.emptyCreate]) {
    button.disabled = !enabled;
    button.title = enabled ? "Create transfer" : blocker;
  }
  elements.preset.disabled = !enabled;
  elements.preset.title = enabled ? "Start from workflow preset" : blocker;
}

function canCreate() {
  return live && !awaitingFreshSnapshot && !transferIssue && context?.capabilities?.create === true;
}

function creationBlocker() {
  const configured = context?.blocking_reasons?.create;
  const blockers = Array.isArray(configured) ? configured : configured ? [configured] : [];
  if (!live) return "Reconnect before creating a transfer";
  if (awaitingFreshSnapshot) return "Wait for a fresh transfer snapshot";
  if (transferIssue) return transferIssue.message;
  if (!context) return "Checking transfer authority";
  if (!blockers.length) return "Transfer creation is unavailable";
  return blockers.map((blocker) => String(blocker).replaceAll("_", " ")).join(" · ");
}

function selectTransfer(uid) {
  ui.selectedUid = uid;
  if (uid) ui.branchUid = uid;
  persistUi();
  if (uid) requestAnimationFrame(() => elements.detail.focus({ preventScroll: true }));
}

function selectBranch(uid) {
  ui.view = "tree";
  ui.branchUid = uid;
  selectTransfer(uid);
}

function openRecord(uid) {
  if (!uid) return;
  H?.emit?.("recordClicked", { record: { uid } });
}

function persistUi() {
  syncControls();
  render();
  if (bridgeStateReceived) H?.patchCardState?.({ transfers: persistedUi() });
}

function syncControls() {
  if (elements.search.value !== ui.search) elements.search.value = ui.search;
  elements.sort.value = ui.sort;
}

function syncInboxCounts(counts) {
  for (const counter of elements.ownershipFilters.querySelectorAll("[data-count-ownership]")) {
    const count = counts.ownership[counter.dataset.countOwnership] || 0;
    counter.textContent = String(count);
    const button = counter.closest("button");
    const label = button?.querySelector("span:first-child")?.textContent || "Ownership filter";
    button?.setAttribute("aria-label", `${label}, ${count} transfers`);
  }
  for (const counter of elements.filters.querySelectorAll("[data-count-workflow]")) {
    const count = counts.workflow[counter.dataset.countWorkflow] || 0;
    counter.textContent = String(count);
    const button = counter.closest("button");
    const label = button?.querySelector("span:first-child")?.textContent || "Workflow filter";
    button?.setAttribute("aria-label", `${label}, ${count} transfers`);
  }
}

function syncInboxNotice() {
  const partialCount = rows.filter((row) => !row.inbox_projection_complete).length;
  const waitingCount = [...actionStates.values()].filter((state) => state.busy || state.waiting).length;
  let notice = null;
  if (transferIssue) {
    notice = { kind: transferIssue.kind, message: transferIssue.message, retry: true };
  } else if (!live) {
    notice = {
      kind: rows.length ? "stale" : everLive ? "reconnecting" : "offline",
      message: rows.length
        ? everLive
          ? "Reconnecting. Showing the last received transfer snapshot."
          : "Offline. Showing the last available transfer snapshot."
        : everLive ? "Reconnecting to the Cell." : "Offline. Waiting to connect to the Cell.",
      retry: true,
    };
  } else if (awaitingFreshSnapshot && rows.length) {
    notice = {
      kind: "stale",
      message: "Connected. Waiting for a fresh transfer snapshot before clearing stale state.",
      retry: true,
    };
  } else if (waitingCount) {
    notice = {
      kind: "waiting",
      message: `${waitingCount} accepted transfer action${waitingCount === 1 ? " is" : "s are"} waiting for pushed Protein evidence.`,
      retry: false,
    };
  } else if (actionWarnings.length) {
    notice = { kind: "warning", message: actionWarnings.join(" · "), retry: false };
  } else if (partialCount) {
    notice = {
      kind: "warning",
      message: `${partialCount} transfer${partialCount === 1 ? " has" : "s have"} no authoritative inbox facet projection. It remains in All and is excluded from specific facets.`,
      retry: false,
    };
  }
  elements.notice.hidden = !notice;
  elements.notice.dataset.kind = notice?.kind || "";
  elements.noticeMessage.textContent = notice?.message || "";
  elements.retry.hidden = !notice?.retry;
}

function clearInboxFilters() {
  ui.ownership = "all";
  ui.workflow = "all";
  ui.search = "";
  persistUi();
}

function hasInboxFilters() {
  return ui.ownership !== "all" || ui.workflow !== "all" || Boolean(ui.search.trim());
}

function proteinIssue(snapshot) {
  const code = String(snapshot.code || snapshot.error?.code || "");
  const message = String(snapshot.message || snapshot.error?.message || snapshot.error || "Transfer data is unavailable.");
  const forbidden = /forbidden|permission|unauthorized|denied/.test(code.toLocaleLowerCase());
  return {
    kind: forbidden ? "forbidden" : "validation",
    message,
  };
}

function normalizeWarnings(value) {
  return (Array.isArray(value) ? value : value ? [value] : [])
    .map((warning) => String(warning?.message || warning?.code || warning || ""))
    .filter(Boolean);
}

function moveSegmentFocus(event, root, selector) {
  if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
  const buttons = [...root.querySelectorAll(selector)].filter((button) => !button.disabled);
  if (!buttons.length) return;
  const current = Math.max(0, buttons.indexOf(document.activeElement));
  const next = event.key === "Home" ? 0
    : event.key === "End" ? buttons.length - 1
      : event.key === "ArrowRight" ? (current + 1) % buttons.length
        : (current - 1 + buttons.length) % buttons.length;
  event.preventDefault();
  buttons[next].focus();
}

function persistedUi() {
  return {
    ownership: ui.ownership,
    workflow: ui.workflow,
    sort: ui.sort,
    selectedUid: ui.selectedUid,
    view: ui.view,
    treeRootUid: ui.treeRootUid,
    branchUid: ui.branchUid,
  };
}

function normalizeUi(raw) {
  const state = raw && typeof raw === "object" ? raw : {};
  const legacyWorkflow = {
    attention: "awaiting_me",
    open: "all",
    settled: "completed",
  }[state.filter];
  return {
    ownership: state.ownership === "mine" ? "mine" : "all",
    workflow: [
      "all",
      "awaiting_me",
      "awaiting_others",
      "active",
      "completed",
      "cancelled_or_broken",
      "discoverable_open",
    ].includes(state.workflow) ? state.workflow : legacyWorkflow || "all",
    sort: ["attention", "name", "status"].includes(state.sort) ? state.sort : "attention",
    selectedUid: typeof state.selectedUid === "string" ? state.selectedUid : "",
    view: ["list", "tree"].includes(state.view) ? state.view : "list",
    treeRootUid: typeof state.treeRootUid === "string" ? state.treeRootUid : "",
    branchUid: typeof state.branchUid === "string" ? state.branchUid : "",
    search: "",
  };
}

function byId(id) {
  return document.getElementById(id);
}
