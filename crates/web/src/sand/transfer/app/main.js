import { filterAndSort, normalizeTransfer, summarize } from "./model.js";
import { renderDetail, renderList, renderSummary } from "./render.js";
import { createTransferComposer } from "./create.js";

const H = window.LinceWidgetHost;
const elements = {
  detail: byId("detail"),
  empty: byId("empty"),
  emptyMessage: byId("empty-message"),
  emptyCreate: byId("empty-create-transfer"),
  filters: byId("filters"),
  list: byId("transfer-list"),
  liveDot: byId("live-dot"),
  loading: byId("loading"),
  search: byId("search"),
  sort: byId("sort"),
  summary: byId("summary"),
  workspace: byId("workspace"),
  create: byId("create-transfer"),
  createBlocker: byId("create-blocker"),
  creator: byId("creator"),
};

let rows = [];
let records = [];
let context = null;
let pendingUid = "";
let loading = true;
let ui = normalizeUi(null);
let bridgeStateReceived = false;

const composer = createTransferComposer(elements.creator, {
  host: H,
  onCreated: (uid) => {
    if (rows.some((row) => row.uid === uid)) {
      composer.complete();
      selectTransfer(uid);
      return;
    }
    pendingUid = uid;
  },
});

elements.create.addEventListener("click", openCreator);
elements.emptyCreate.addEventListener("click", openCreator);

elements.search.addEventListener("input", () => {
  ui.search = elements.search.value;
  render();
});
elements.sort.addEventListener("change", () => {
  ui.sort = elements.sort.value;
  persistUi();
});
elements.filters.addEventListener("click", (event) => {
  const target = event.target.closest("button[data-filter]");
  if (!target) return;
  ui.filter = target.dataset.filter;
  persistUi();
});
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && ui.selectedUid) selectTransfer("");
});

H?.onLive?.((live) => {
  elements.liveDot.dataset.live = String(Boolean(live));
  elements.liveDot.setAttribute("aria-label", live ? "Connected" : "Reconnecting");
});

H?.onCardState?.((cardState) => {
  bridgeStateReceived = true;
  const incoming = normalizeUi(cardState?.transfers);
  incoming.search = ui.search;
  ui = incoming;
  syncControls();
  render();
});

H?.subscribeProtein?.(
  "transfers",
  { source: "transfer", limit: 500 },
  ({ rows: incoming }) => {
    const projected = Array.isArray(incoming) ? incoming : [];
    context = projected.find((row) => row?.kind === "transfer_context") || null;
    rows = projected.filter((row) => row?.kind !== "transfer_context").map(normalizeTransfer);
    loading = false;
    if (ui.selectedUid && !rows.some((row) => row.uid === ui.selectedUid)) ui.selectedUid = "";
    syncComposerOptions();
    if (pendingUid && rows.some((row) => row.uid === pendingUid)) {
      const createdUid = pendingUid;
      pendingUid = "";
      composer.complete();
      selectTransfer(createdUid);
      return;
    }
    render();
  },
);

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
  const filtered = filterAndSort(rows, ui.filter, ui.search, ui.sort);
  const selected = rows.find((row) => row.uid === ui.selectedUid) || null;
  renderSummary(elements.summary, summarize(rows));
  renderList(elements.list, filtered, ui.selectedUid, selectTransfer);
  renderDetail(elements.detail, selected, {
    onBack: () => selectTransfer(""),
    onOpenRecord: openRecord,
  });

  elements.loading.hidden = !loading;
  elements.workspace.hidden = loading || rows.length === 0;
  elements.empty.hidden = loading || filtered.length > 0;
  elements.emptyMessage.textContent = rows.length ? "No transfers match this view." : "No transfers yet.";
  elements.workspace.dataset.detailOpen = String(Boolean(selected));
  syncCreateCapability();
  for (const button of elements.filters.querySelectorAll("button[data-filter]")) {
    button.setAttribute("aria-pressed", String(button.dataset.filter === ui.filter));
  }
}

function openCreator() {
  if (!canCreate()) return;
  selectTransfer("");
  composer.show();
}

function syncComposerOptions() {
  const people = records.filter((record) => record?.kind === "person");
  const promiseRecords = records.filter((record) => !["transfer", "person", "protein", "sand", "thread", "message"].includes(record?.kind));
  composer.setOptions({ people, records: promiseRecords, transfers: rows });
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
}

function canCreate() {
  return context?.capabilities?.create === true;
}

function creationBlocker() {
  const configured = context?.blocking_reasons?.create;
  const blockers = Array.isArray(configured) ? configured : configured ? [configured] : [];
  if (!context) return "Checking transfer authority";
  if (!blockers.length) return "Transfer creation is unavailable";
  return blockers.map((blocker) => String(blocker).replaceAll("_", " ")).join(" · ");
}

function selectTransfer(uid) {
  ui.selectedUid = uid;
  persistUi();
  if (uid) requestAnimationFrame(() => elements.detail.focus({ preventScroll: true }));
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

function persistedUi() {
  return { filter: ui.filter, sort: ui.sort, selectedUid: ui.selectedUid };
}

function normalizeUi(raw) {
  const state = raw && typeof raw === "object" ? raw : {};
  return {
    filter: ["all", "attention", "open", "settled"].includes(state.filter) ? state.filter : "all",
    sort: ["attention", "name", "status"].includes(state.sort) ? state.sort : "attention",
    selectedUid: typeof state.selectedUid === "string" ? state.selectedUid : "",
    search: "",
  };
}

function byId(id) {
  return document.getElementById(id);
}
