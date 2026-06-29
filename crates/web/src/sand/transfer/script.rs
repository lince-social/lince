pub(super) fn script() -> String {
    r##"
(() => {
  const app = document.getElementById("transfer-app");
  const status = document.getElementById("transfer-status");
  const identitySummary = document.getElementById("identity-summary");
  const identityForm = document.getElementById("identity-form");
  const identityLabel = document.getElementById("identity-label");
  const identitySave = document.getElementById("identity-save");
  const identityReset = document.getElementById("identity-reset");
  const settingsPanel = document.getElementById("settings-panel");
  const resetKeyModal = document.getElementById("reset-key-modal");
  const ingressSummary = document.getElementById("ingress-summary");
  const ingressForm = document.getElementById("ingress-form");
  const publicProposalsEnabled = document.getElementById("public-proposals-enabled");
  const networkSummary = document.getElementById("network-summary");
  const networkPolicyForm = document.getElementById("network-policy-form");
  const knownPeerPollingEnabled = document.getElementById("known-peer-polling-enabled");
  const shareQuantityProjections = document.getElementById("share-quantity-projections");
  const globalSatiationForm = document.getElementById("global-satiation-form");
  const globalSatiationPolicy = document.getElementById("global-satiation-policy");
  const receiptPolicyForm = document.getElementById("receipt-policy-form");
  const sendReceivedReceipts = document.getElementById("send-received-receipts");
  const sendSeenReceipts = document.getElementById("send-seen-receipts");
  const anonymousPackageViewing = document.getElementById("anonymous-package-viewing");
  const contactDiscoveryForm = document.getElementById("contact-discovery-form");
  const contactDiscoveryBaseUrl = document.getElementById("contact-discovery-base-url");
  const contactDiscoverySearch = document.getElementById("contact-discovery-search");
  const contactDiscoveryList = document.getElementById("contact-discovery-list");
  const peerList = document.getElementById("peer-list");
  const organLoginForm = document.getElementById("organ-login-form");
  const loginOrgan = document.getElementById("login-organ");
  const recordForm = document.getElementById("record-form");
  const recordList = document.getElementById("record-list");
  const proposalSubmit = document.getElementById("proposal-create");
  const transferSearch = document.getElementById("transfer-search");
  const recordOptions = document.getElementById("record-options");
  const organOptions = document.getElementById("organ-options");
  const transferList = document.getElementById("transfer-list");
  const transferDetail = document.getElementById("transfer-detail");
  const transferCount = document.getElementById("transfer-count");
  const transferTabs = document.getElementById("transfer-tabs");
  const visualizationToggle = document.getElementById("visualization-toggle");
  const transferGraphView = document.getElementById("transfer-graph-view");
  const transferGraph = document.getElementById("transfer-graph");
  const transferGraphSummary = document.getElementById("transfer-graph-summary");

  const frame = window.frameElement;
  const bridge = window.LinceWidgetHost || null;
  const instanceId = String(frame?.dataset?.packageInstanceId || "").trim();
  const savedUiState = normalizeUiState(bridge?.getCardState?.()?.transfer || bridge?.getCardState?.() || {});

  let snapshot = {
    localIdentity: null,
    ingressPolicy: { publicProposalsEnabled: false },
    networkPolicy: { knownPeerPollingEnabled: true, shareQuantityProjections: false },
    receiptPolicy: { sendReceivedReceipts: true, sendSeenReceipts: true, anonymousPackageViewing: false },
    records: [],
    organs: [],
    transfers: [],
    gossipTransfers: [],
  };
  let discoveredContacts = [];
  let selectedTransferId = savedUiState.selectedTransferId;
  let selectedGossipTransferUid = savedUiState.selectedGossipTransferUid;
  let activeTransferTab = savedUiState.activeTransferTab;
  let visualizationMode = savedUiState.visualizationMode;
  let detailMode = savedUiState.detailMode;
  let pendingDeleteTransferId = null;
  let busy = false;
  let contractLoading = false;
  let reloadQueued = false;
  let confirmingProgressAction = null;
  const seenMarkingTransfers = new Set();
  const collapsedTransferUids = new Set(savedUiState.collapsedTransferUids);
  const graphState = {
    simulation: null,
    zoom: null,
    viewport: null,
  };

  function normalizeUiState(rawState) {
    const value = rawState && typeof rawState === "object" ? rawState : {};
    const tab = value.activeTransferTab === "observed" ? "observed" : "mine";
    const visualization = value.visualizationMode === "graph" ? "graph" : "structured";
    const detail = value.detailMode === "create" ? "create" : "selected";
    const transferId = Number(value.selectedTransferId);
    const collapsed = Array.isArray(value.collapsedTransferUids)
      ? value.collapsedTransferUids.map((uid) => String(uid || "").trim()).filter(Boolean)
      : [];

    return {
      activeTransferTab: visualization === "graph" ? "mine" : tab,
      visualizationMode: visualization,
      detailMode: visualization === "graph" ? "selected" : detail,
      selectedTransferId: Number.isFinite(transferId) && transferId > 0 ? transferId : null,
      selectedGossipTransferUid: String(value.selectedGossipTransferUid || "").trim() || null,
      collapsedTransferUids: collapsed,
    };
  }

  function persistUiState() {
    (window.LinceWidgetHost || bridge)?.patchCardState?.({
      transfer: {
        activeTransferTab,
        visualizationMode,
        detailMode,
        selectedTransferId,
        selectedGossipTransferUid,
        collapsedTransferUids: Array.from(collapsedTransferUids),
      },
    });
  }

  function applyUiState(rawState) {
    const next = normalizeUiState(rawState?.transfer || rawState || {});
    selectedTransferId = next.selectedTransferId;
    selectedGossipTransferUid = next.selectedGossipTransferUid;
    activeTransferTab = next.activeTransferTab;
    visualizationMode = next.visualizationMode;
    detailMode = next.detailMode;
    collapsedTransferUids.clear();
    for (const uid of next.collapsedTransferUids) {
      collapsedTransferUids.add(uid);
    }
  }

  function bindHostState() {
    const host = window.LinceWidgetHost || bridge;
    if (!host || typeof host.subscribe !== "function") return;
    host.subscribe((detail) => {
      const cardState = detail?.meta?.cardState || host.getCardState?.() || null;
      if (
        !cardState ||
        (!cardState.transfer &&
          !Object.prototype.hasOwnProperty.call(cardState, "visualizationMode"))
      ) {
        return;
      }
      applyUiState(cardState);
      setVisualizationMode(visualizationMode);
      render();
    });
    host.requestState?.();
  }

  function contractUrl() {
    return "/host/widgets/" + encodeURIComponent(instanceId) + "/contract";
  }

  function actionUrl(action) {
    return "/host/widgets/" + encodeURIComponent(instanceId) + "/actions/" + encodeURIComponent(action);
  }

  function streamUrl() {
    return "/host/widgets/" + encodeURIComponent(instanceId) + "/stream";
  }

  function setBusy(nextBusy) {
    busy = nextBusy;
    app.dataset.busy = nextBusy ? "true" : "false";
    for (const element of app.querySelectorAll("button, input, select, textarea")) {
      if (element.dataset.keepEnabled === "true") continue;
      element.disabled = nextBusy;
    }
    updateStaticDisabledState();
  }

  function setStatus(text, tone = "idle") {
    status.textContent = text;
    status.dataset.tone = tone;
  }

  function setSettingsOpen(open) {
    settingsPanel.dataset.open = open ? "true" : "false";
    settingsPanel.setAttribute("aria-hidden", open ? "false" : "true");
  }

  function setResetModalOpen(open) {
    resetKeyModal.dataset.open = open ? "true" : "false";
    resetKeyModal.setAttribute("aria-hidden", open ? "false" : "true");
  }

  function escapeHtml(value) {
    return String(value ?? "")
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;")
      .replaceAll('"', "&quot;")
      .replaceAll("'", "&#039;");
  }

  function parseNumber(value, fallback = 0) {
    const number = Number(String(value ?? "").trim().replace(",", "."));
    return Number.isFinite(number) ? number : fallback;
  }

  function formatQuantity(value) {
    const number = Number(value);
    if (!Number.isFinite(number)) return "0";
    return number.toLocaleString(undefined, { maximumFractionDigits: 3 });
  }

  function currentQuantityText(record) {
    return record ? formatQuantity(record.quantity) : "unknown";
  }

  function shortKey(value) {
    const text = String(value || "");
    if (text.length <= 16) return text || "none";
    return text.slice(0, 8) + "..." + text.slice(-6);
  }

  function recordLabel(record) {
    return "#" + record.id + " " + (record.head || "Record");
  }

  function selectedRecordId(raw) {
    const value = String(raw || "").trim();
    const hashMatch = value.match(/^#?(\d+)/);
    if (hashMatch) return Number(hashMatch[1]);
    const lowered = value.toLowerCase();
    const record = snapshot.records.find((item) =>
      String(item.head || "").toLowerCase() === lowered
    );
    return record ? Number(record.id) : 0;
  }

  function selectedOrganId(raw) {
    const value = String(raw || "").trim();
    if (!value) return 0;
    const numeric = Number(value);
    if (Number.isInteger(numeric) && numeric > 0) return numeric;
    const organ = snapshot.organs.find((item) =>
      item.name === value || String(item.id) === value || item.baseUrl === value
    );
    return organ ? Number(organ.id) : 0;
  }

  function transferById(id) {
    return snapshot.transfers.find((transfer) => Number(transfer.id) === Number(id)) || null;
  }

  function gossipByUid(uid) {
    return snapshot.gossipTransfers.find((transfer) => String(transfer.transferUid) === String(uid)) || null;
  }

  function visibleTransfers() {
    const localLabel = snapshot.localIdentity?.label || "";
    const query = String(transferSearch?.value || "").trim().toLowerCase();
    const transfers = activeTransferTab === "mine"
      ? snapshot.transfers.filter((transfer) => transfer.proposerLabel === localLabel)
      : snapshot.transfers.filter((transfer) =>
      transfer.proposerLabel !== localLabel && (transfer.localRole || transfer.controls?.canDuplicate)
    );
    if (!query) return transfers;
    return transfers.filter((transfer) => [
      transfer.title,
      transfer.transferUid,
      transfer.proposerLabel,
      transfer.counterpartyLabel,
      transfer.contribution?.head,
      transfer.need?.head,
      transfer.status,
      transfer.state,
    ].some((value) => String(value || "").toLowerCase().includes(query)));
  }

  function buildTransferTree(transfers) {
    const byUid = new Map();
    const childrenByParentUid = new Map();
    for (const transfer of transfers) {
      const uid = String(transfer.transferUid || "");
      if (uid) byUid.set(uid, transfer);
    }
    for (const transfer of transfers) {
      const parentUid = String(transfer.tree?.parentUid || "");
      if (!parentUid || !byUid.has(parentUid)) continue;
      if (!childrenByParentUid.has(parentUid)) childrenByParentUid.set(parentUid, []);
      childrenByParentUid.get(parentUid).push(transfer);
    }
    const sorted = (items) => items.slice().sort((left, right) => {
      const leftTime = String(left.updatedAt || "");
      const rightTime = String(right.updatedAt || "");
      if (leftTime !== rightTime) return rightTime.localeCompare(leftTime);
      return Number(right.id || 0) - Number(left.id || 0);
    });
    const roots = sorted(transfers.filter((transfer) => {
      const parentUid = String(transfer.tree?.parentUid || "");
      return !parentUid || !byUid.has(parentUid);
    }));
    const rows = [];
    const visit = (transfer, depth, ancestors) => {
      const uid = String(transfer.transferUid || "");
      if (uid && ancestors.has(uid)) return;
      const children = sorted(childrenByParentUid.get(uid) || []);
      rows.push({ transfer, depth, childCount: children.length });
      if (!uid || collapsedTransferUids.has(uid)) return;
      const nextAncestors = new Set(ancestors);
      nextAncestors.add(uid);
      for (const child of children) visit(child, depth + 1, nextAncestors);
    };
    for (const root of roots) visit(root, 0, new Set());
    return rows;
  }

  function toggleTransferBranch(transferUid) {
    if (!transferUid) return;
    if (collapsedTransferUids.has(transferUid)) {
      collapsedTransferUids.delete(transferUid);
    } else {
      collapsedTransferUids.add(transferUid);
    }
    persistUiState();
    renderTransferList();
  }

  function setVisualizationMode(mode) {
    visualizationMode = mode === "graph" ? "graph" : "structured";
    app.dataset.visualization = visualizationMode;
    if (transferGraphView) {
      transferGraphView.setAttribute("aria-hidden", visualizationMode === "graph" ? "false" : "true");
    }
    if (visualizationMode === "graph" && activeTransferTab === "observed") {
      activeTransferTab = "mine";
      selectedGossipTransferUid = null;
    }
    if (visualizationMode === "graph") {
      detailMode = "selected";
    }
    if (visualizationToggle) {
      const graphActive = visualizationMode === "graph";
      visualizationToggle.textContent = graphActive ? "Structured" : "Graph";
      visualizationToggle.setAttribute("aria-label", graphActive ? "Switch to structured view" : "Switch to graph view");
      visualizationToggle.title = graphActive ? "Switch to structured view" : "Switch to graph view";
    }
    persistUiState();
    renderTransferList();
    renderDetail();
    renderTransferGraph();
  }

  function selectedTransfer() {
    const transfers = visibleTransfers();
    if (!selectedTransferId && transfers.length) {
      selectedTransferId = transfers[0].id;
    }
    return transferById(selectedTransferId);
  }

  function reconcileSelectedTransfer() {
    if (detailMode === "create") return;
    if (selectedTransferId && !visibleTransfers().some((transfer) => Number(transfer.id) === Number(selectedTransferId))) {
      selectedTransferId = null;
    }
    if (pendingDeleteTransferId && !transferById(pendingDeleteTransferId)) {
      pendingDeleteTransferId = null;
    }
    if (selectedGossipTransferUid && !gossipByUid(selectedGossipTransferUid)) {
      selectedGossipTransferUid = null;
    }
  }

  function chip(label, tone = "idle") {
    return `<span class="chip" data-tone="${escapeHtml(tone)}">${escapeHtml(label)}</span>`;
  }

  function renderIdentity() {
    const identity = snapshot.localIdentity;
    if (!identity) {
      identitySummary.innerHTML = `
        <div class="emptyBlock">
          Creating local signing identity.
        </div>
      `;
      return;
    }
    identityLabel.value = identity.label || "";
    identitySummary.innerHTML = `
      <div class="identityBox">
        <div class="strong">${escapeHtml(identity.label)}</div>
        <div class="meta">public key ${escapeHtml(shortKey(identity.publicKey))}</div>
        <div class="meta">stored locally in this node database</div>
        <div class="meta">created ${escapeHtml(identity.createdAt || "")}</div>
        <div class="meta">updated ${escapeHtml(identity.updatedAt || "")}</div>
      </div>
    `;
  }

  function renderIngress() {
    const enabled = Boolean(snapshot.ingressPolicy?.publicProposalsEnabled);
    publicProposalsEnabled.checked = enabled;
    ingressSummary.innerHTML = `
      <div class="identityBox">
        <div class="strong">${escapeHtml(enabled ? "Public proposals enabled" : "Public proposals disabled")}</div>
        <div class="meta">${escapeHtml(snapshot.ingressPolicy?.copy || "Initial public proposal ingress is off by default.")}</div>
      </div>
    `;
  }

  function formatDateTime(value) {
    const text = String(value || "").trim();
    if (!text) return "never";
    const date = new Date(text.endsWith("Z") ? text : text.replace(" ", "T") + "Z");
    if (Number.isNaN(date.getTime())) return text;
    return date.toLocaleString();
  }

  function trustTone(trustState) {
    if (trustState === "known") return "ok";
    if (trustState === "blocked") return "danger";
    return "warn";
  }

  function renderNetwork() {
    const polling = Boolean(snapshot.networkPolicy?.knownPeerPollingEnabled);
    const sharingProjections = Boolean(snapshot.networkPolicy?.shareQuantityProjections);
    const receipt = snapshot.receiptPolicy || {};
    knownPeerPollingEnabled.checked = polling;
    shareQuantityProjections.checked = sharingProjections;
    sendReceivedReceipts.checked = receipt.sendReceivedReceipts !== false;
    sendSeenReceipts.checked = receipt.sendSeenReceipts !== false;
    anonymousPackageViewing.checked = Boolean(receipt.anonymousPackageViewing);
    if (globalSatiationPolicy) globalSatiationPolicy.value = snapshot.globalSatiationPolicy || "";
    const knownCount = snapshot.organs.filter((organ) => organ.trustState === "known").length;
    const unknownCount = snapshot.organs.filter((organ) => organ.trustState === "unknown").length;
    const blockedCount = snapshot.organs.filter((organ) => organ.trustState === "blocked").length;
    networkSummary.innerHTML = `
      <div class="identityBox">
        <div class="strong">${escapeHtml(polling ? "Known-peer polling enabled" : "Known-peer polling disabled")}</div>
        <div class="meta">${escapeHtml(knownCount)} known / ${escapeHtml(unknownCount)} unknown / ${escapeHtml(blockedCount)} blocked / projections ${escapeHtml(sharingProjections ? "shared" : "local")}</div>
      </div>
    `;
    renderPeerList();
    renderDiscoveredContacts();
  }

  function peerActionButton(organ, action, label, tone = "") {
    return `<button type="button" class="${escapeHtml(tone)}" data-peer-action="${escapeHtml(action)}" data-peer-id="${escapeHtml(organ.id)}" data-keep-enabled="true">${escapeHtml(label)}</button>`;
  }

  function renderPeerList() {
    if (!snapshot.organs.length) {
      peerList.innerHTML = `<div class="emptyBlock">No Organ contacts yet.</div>`;
      return;
    }
    peerList.innerHTML = snapshot.organs.map((organ) => {
      const trust = organ.trustState || "known";
      const discoverable = Boolean(organ.contactDiscoveryEnabled);
      const promote = trust === "known" ? "" : peerActionButton(organ, "trust-known", "Known", "primary");
      const unblock = trust === "blocked" ? peerActionButton(organ, "trust-unknown", "Unblock") : peerActionButton(organ, "trust-blocked", "Block", "danger");
      const discovery = peerActionButton(organ, discoverable ? "hide-contact" : "show-contact", discoverable ? "Hide" : "Expose");
      const poll = trust === "blocked" ? "" : peerActionButton(organ, "poll", "Poll");
      return `
        <div class="networkRow" data-peer-id="${escapeHtml(organ.id)}">
          <div class="networkRowMain">
            <strong>${escapeHtml(organ.name || "Organ")}</strong>
            <span class="meta mono">${escapeHtml(organ.baseUrl || "")}</span>
            <span class="chips">
              ${chip(trust, trustTone(trust))}
              ${chip(discoverable ? "discoverable" : "private", discoverable ? "ok" : "idle")}
              ${chip(organ.authenticated ? "connected" : "no session", organ.authenticated ? "ok" : "idle")}
            </span>
            <span class="meta">seen ${escapeHtml(formatDateTime(organ.lastSeenAt))} / polled ${escapeHtml(formatDateTime(organ.lastTransferPolledAt))}</span>
          </div>
          <div class="networkRowActions">
            <label class="compactField"><span>Proximity</span><input data-peer-proximity="${escapeHtml(organ.id)}" value="${escapeHtml(organ.proximity ?? 100)}" inputmode="numeric" data-keep-enabled="true"></label>
            <label class="checkRow compactCheck"><input type="checkbox" data-peer-received-receipts="${escapeHtml(organ.id)}" ${organ.transferSendReceivedReceipts === false ? "" : "checked"} data-keep-enabled="true"><span>Received</span></label>
            <label class="checkRow compactCheck"><input type="checkbox" data-peer-seen-receipts="${escapeHtml(organ.id)}" ${organ.transferSendSeenReceipts === false ? "" : "checked"} data-keep-enabled="true"><span>Seen</span></label>
            ${peerActionButton(organ, "save-proximity", "Save proximity")}
            ${peerActionButton(organ, "save-receipts", "Save receipts")}
            ${promote}
            ${poll}
            ${discovery}
            ${unblock}
          </div>
        </div>
      `;
    }).join("");
  }

  function renderDiscoveredContacts() {
    if (!discoveredContacts.length) {
      contactDiscoveryList.innerHTML = `<div class="emptyBlock">No discovered contacts loaded.</div>`;
      return;
    }
    contactDiscoveryList.innerHTML = discoveredContacts.map((contact, index) => {
      const exists = snapshot.organs.some((organ) => String(organ.baseUrl || "").replace(/\/+$/, "") === String(contact.baseUrl || "").replace(/\/+$/, ""));
      return `
        <div class="networkRow">
          <div class="networkRowMain">
            <strong>${escapeHtml(contact.name || "Discovered node")}</strong>
            <span class="meta mono">${escapeHtml(contact.baseUrl || "")}</span>
            <span class="meta">seen ${escapeHtml(formatDateTime(contact.lastSeenAt))}</span>
          </div>
          <div class="networkRowActions">
            <button type="button" data-contact-add="${escapeHtml(index)}" data-keep-enabled="true" ${exists ? "disabled" : ""}>${escapeHtml(exists ? "Added" : "Add unknown")}</button>
          </div>
        </div>
      `;
    }).join("");
  }

  function renderRecords() {
    recordOptions.innerHTML = snapshot.records
      .map((record) => `<option value="${escapeHtml(recordLabel(record))}"></option>`)
      .join("");
    recordList.innerHTML = snapshot.records.length
      ? snapshot.records.slice(0, 8).map((record) => `
          <button type="button" class="listRow" data-fill-record="${escapeHtml(recordLabel(record))}" data-keep-enabled="true">
            <span>${escapeHtml(record.head || "Record")}</span>
            <span class="mono">#${escapeHtml(record.id)} / ${escapeHtml(formatQuantity(record.quantity))}</span>
          </button>
        `).join("")
      : `<div class="emptyBlock">No Records yet.</div>`;
  }

  function renderOrgans() {
    organOptions.innerHTML = snapshot.organs
      .map((organ) => `<option value="${escapeHtml(organ.name)}"></option>`)
      .join("");
    loginOrgan.innerHTML = [`<option value="">Select Organ</option>`].concat(
      snapshot.organs.map((organ) => {
        const state = organ.authenticated ? "connected" : "login needed";
        return `<option value="${escapeHtml(organ.id)}">${escapeHtml(organ.name + " / " + state)}</option>`;
      })
    ).join("");
  }

  function sideHtml(label, side, transfer = null) {
    const record = snapshot.records.find((item) => Number(item.id) === Number(side?.recordId || 0));
    const organUrl = transfer?.targetBaseUrl || transfer?.sourceBaseUrl || "local";
    return `
      <div class="partyCard">
        <div class="organMeta">
          <div><span>Organ ID</span><strong>${escapeHtml(side?.recordId || 0)}</strong></div>
          <div><span>Organ Name</span><strong>${escapeHtml(side?.actorLabel || "unbound")}</strong></div>
          <div><span>Organ URL</span><strong>${escapeHtml(side?.publicKey ? organUrl : "unbound")}</strong></div>
        </div>
        <div class="partyItemLine">
          <span>
            <span class="fieldLabel">Transfer Item ${escapeHtml(label)} Title</span>
            <strong>${escapeHtml(side?.head || label + " Head")}</strong>
          </span>
          <span class="qtyPair">
            <span>${escapeHtml(label)} Qty ${escapeHtml(formatQuantity(side?.quantity || 0))}</span>
            <span class="partyQtyInline">Current Qty ${escapeHtml(currentQuantityText(record))}</span>
          </span>
        </div>
      </div>
    `;
  }

  function editableSideHtml(label, side, transfer) {
    const role = label.toLowerCase();
    const record = snapshot.records.find((item) => Number(item.id) === Number(side?.recordId || 0));
    return `
      <div class="partyCard">
        <div class="organMeta">
          <div><span>Organ ID</span><strong>${escapeHtml(side?.recordId || 0)}</strong></div>
          <div><span>Organ Name</span><strong>${escapeHtml(side?.actorLabel || "unbound")}</strong></div>
          <div><span>Organ URL</span><strong>${escapeHtml(transfer.targetBaseUrl || transfer.sourceBaseUrl || "local")}</strong></div>
        </div>
        <div class="editableTerms" data-local-terms="${escapeHtml(role)}">
          <label>
            <span>Transfer Item ${escapeHtml(label)} Title</span>
            <input id="local-item-title" value="${escapeHtml(side?.head || "")}" autocomplete="off">
          </label>
          <label>
            <span>Local Record</span>
            <input id="local-record-input" list="record-options" value="${escapeHtml(recordLabel({ id: side?.recordId || "", head: side?.head || "" }))}" autocomplete="off">
          </label>
          <label>
            <span>${escapeHtml(label)} Qty / Current Qty ${escapeHtml(currentQuantityText(record))}</span>
            <input id="local-quantity-input" inputmode="decimal" value="${escapeHtml(formatQuantity(side?.quantity || 1))}">
          </label>
          <div class="formActions">
            <button type="button" class="primary" data-transfer-action="update-transfer-local-item">Save Terms</button>
          </div>
        </div>
      </div>
    `;
  }

  function duplicateSideHtml(label) {
    const role = label.toLowerCase();
    return `
      <div class="partyCard duplicateCard">
        <div class="partyMeta">
          <span>unclaimed ${escapeHtml(label)}</span>
          <span>Select your local Record</span>
          <span>then duplicate</span>
        </div>
        <div class="editableTerms">
          <input id="duplicate-role" type="hidden" value="${escapeHtml(role)}">
          ${recordSelectHtml("duplicate-record")}
          ${actionButton("duplicate-proposal", "Duplicate as " + label, false, 'class="primary"')}
        </div>
      </div>
    `;
  }

  function partySection(transfer, sideName, side) {
    const local = transfer.localRole === sideName;
    const label = sideName === "need" ? "Need" : "Contribution";
    const unclaimed = !side?.publicKey;
    const body = transfer.controls?.canDuplicate && unclaimed
      ? duplicateSideHtml(label)
      : local
        ? editableSideHtml(label, side || {}, transfer)
        : sideHtml(label, side || {}, transfer);
    const roleParties = (transfer.partySettlements || []).filter(p => p.role === sideName);
    const multiPartyHtml = roleParties.length > 1
      ? `<div class="partySettlementList">${roleParties.map(p => `
          <span class="partySettlementRow ${p.settled ? "settled" : "unsettled"}">
            ${escapeHtml(p.actorLabel)}
            <span class="settleMark">${p.settled ? "✓" : "○"}</span>
          </span>`).join("")}</div>`
      : "";
    return `
      <section class="transferParty" data-local="${local ? "true" : "false"}">
        <div class="partyLabel">${escapeHtml(label)}</div>
        ${processButtons(transfer, sideName)}
        ${body}
        ${multiPartyHtml}
      </section>
    `;
  }

  function transferChips(transfer) {
    const controls = transfer.controls || {};
    const agreement = transfer.agreement || {};
    const confirmations = transfer.confirmations || {};
    const localRole = transfer.localRole || "observer";
    return [
      chip(transfer.status || transfer.state || "proposal", transfer.status === "ready_to_settle" ? "warn" : transfer.status === "local_settled" ? "ok" : "idle"),
      chip("local " + localRole, transfer.localRole ? "ok" : "idle"),
      chip("contribution agree " + Number(agreement.contribution || 0), Number(agreement.contribution || 0) >= 2 ? "ok" : "warn"),
      chip("need agree " + Number(agreement.need || 0), Number(agreement.need || 0) >= 2 ? "ok" : "warn"),
      chip("delivery " + (confirmations.delivery ? "yes" : "no"), confirmations.delivery ? "ok" : "warn"),
      chip("receipt " + (confirmations.receipt ? "yes" : "no"), confirmations.receipt ? "ok" : "warn"),
      controls.canDuplicate ? chip("can duplicate", "ok") : "",
    ].join("");
  }

  function renderTransferList() {
    renderTransferTabs();
    if (activeTransferTab === "observed") {
      renderGossipList();
      return;
    }
    const transfers = visibleTransfers();
    transferCount.textContent = transfers.length + " total";
    if (!transfers.length) {
      transferList.innerHTML = `<div class="emptyBlock">No Transfers in this tab.</div>`;
      return;
    }
    const rows = buildTransferTree(transfers);
    transferList.innerHTML = rows.map(({ transfer, depth, childCount }) => {
      const active = Number(transfer.id) === Number(selectedTransferId);
      const safeDepth = Math.min(depth, 8);
      const indentPx = 10 + safeDepth * 18;
      const guidePx = 17 + Math.max(safeDepth - 1, 0) * 18;
      const transferUid = String(transfer.transferUid || "");
      const collapsed = transferUid && collapsedTransferUids.has(transferUid);
      const toggle = childCount
        ? `<span class="treeToggle" role="button" tabindex="0" data-transfer-toggle="${escapeHtml(transferUid)}" aria-label="${escapeHtml(collapsed ? "Expand Transfer" : "Collapse Transfer")}" aria-expanded="${collapsed ? "false" : "true"}">${collapsed ? "▸" : "▾"}</span>`
        : `<span class="treeToggleSpacer"></span>`;
      return `
        <button type="button" class="transferRow" data-transfer-id="${escapeHtml(transfer.id)}" data-active="${active ? "true" : "false"}" data-depth="${escapeHtml(safeDepth)}" data-keep-enabled="true" style="--tree-indent: ${indentPx}px; --tree-guide: ${guidePx}px">
          <span class="treeCell">
            ${toggle}
            <span class="transferTitle">${escapeHtml(transfer.title || "Transfer")}</span>
          </span>
          <span class="meta">#${escapeHtml(transfer.id)} ${escapeHtml(shortKey(transfer.transferUid))}</span>
          <span class="meta">${escapeHtml(childCount ? childCount + " children" : transfer.tree?.parentUid ? "child" : "root")}</span>
          <span class="meta">updated ${escapeHtml(transfer.updatedAt || "")}</span>
          <span class="chips">${transferChips(transfer)}</span>
        </button>
      `;
    }).join("");
  }

  function renderTransferTabs() {
    const localLabel = snapshot.localIdentity?.label || "";
    const mineCount = snapshot.transfers.filter((transfer) => transfer.proposerLabel === localLabel).length;
    const participatingCount = snapshot.transfers.filter((transfer) =>
      transfer.proposerLabel !== localLabel && (transfer.localRole || transfer.controls?.canDuplicate)
    ).length;
    const observedCount = snapshot.gossipTransfers.length;
    transferTabs.innerHTML = [
      tabButton("mine", "Mine", mineCount),
      tabButton("participating", "Participating", participatingCount),
      tabButton("observed", "Observed", observedCount),
    ].join("");
  }

  function tabButton(tab, label, count) {
    return `<button type="button" class="tabButton" data-transfer-tab="${escapeHtml(tab)}" data-active="${activeTransferTab === tab ? "true" : "false"}">${escapeHtml(label)} ${escapeHtml(count)}</button>`;
  }

  function graphTransfers() {
    const query = String(transferSearch?.value || "").trim().toLowerCase();
    const transfers = snapshot.transfers.slice();
    if (!query) return transfers;
    return transfers.filter((transfer) => [
      transfer.title,
      transfer.transferUid,
      transfer.proposerLabel,
      transfer.counterpartyLabel,
      transfer.contribution?.head,
      transfer.need?.head,
      transfer.status,
      transfer.state,
    ].some((value) => String(value || "").toLowerCase().includes(query)));
  }

  function graphColor(transfer) {
    if (Number(transfer.id) === Number(selectedTransferId)) return "#86c7ff";
    if (transfer.status === "inactive") return "#647183";
    if (transfer.localRole === "contribution") return "#8fe3aa";
    if (transfer.localRole === "need") return "#b99cff";
    return "#f0c979";
  }

  function renderTransferGraph() {
    if (!transferGraph || visualizationMode !== "graph") return;
    if (!window.d3) {
      transferGraphSummary.textContent = "D3 is not loaded";
      return;
    }
    const transfers = graphTransfers();
    const byUid = new Map(transfers.map((transfer) => [String(transfer.transferUid || ""), transfer]));
    const nodes = transfers.map((transfer) => ({
      id: String(transfer.id),
      uid: String(transfer.transferUid || transfer.id),
      transfer,
      radius: Number(transfer.id) === Number(selectedTransferId) ? 22 : 18,
    }));
    const links = transfers
      .map((transfer) => {
        const parentUid = String(transfer.tree?.parentUid || "");
        const parent = parentUid ? byUid.get(parentUid) : null;
        return parent ? { source: String(parent.id), target: String(transfer.id) } : null;
      })
      .filter(Boolean);
    transferGraphSummary.textContent = `${transfers.length} transfers / ${links.length} links`;
    const svg = window.d3.select(transferGraph);
    const rect = transferGraph.getBoundingClientRect();
    const width = Math.max(360, Math.round(rect.width || transferGraph.clientWidth || 900));
    const height = Math.max(320, Math.round(rect.height || transferGraph.clientHeight || 600));
    svg.attr("viewBox", [0, 0, width, height].join(" "));
    svg.selectAll("*").remove();
    const root = svg.append("g");
    graphState.viewport = root;
    graphState.zoom = window.d3.zoom()
      .scaleExtent([0.35, 3])
      .on("zoom", (event) => root.attr("transform", event.transform));
    svg.call(graphState.zoom);
    const link = root.append("g")
      .attr("class", "transferGraphLinks")
      .selectAll("line")
      .data(links)
      .join("line");
    const node = root.append("g")
      .attr("class", "transferGraphNodes")
      .selectAll("g")
      .data(nodes, (node) => node.id)
      .join("g")
      .attr("class", "transferGraphNode")
      .attr("data-active", (node) => Number(node.transfer.id) === Number(selectedTransferId) ? "true" : "false")
      .call(window.d3.drag()
        .on("start", (event, node) => {
          if (!event.active) graphState.simulation?.alphaTarget(0.18).restart();
          node.fx = node.x;
          node.fy = node.y;
        })
        .on("drag", (event, node) => {
          node.fx = event.x;
          node.fy = event.y;
        })
        .on("end", (event, node) => {
          if (!event.active) graphState.simulation?.alphaTarget(0);
          node.fx = null;
          node.fy = null;
        }));
    node.append("circle")
      .attr("r", (node) => node.radius)
      .attr("fill", (node) => graphColor(node.transfer));
    node.append("text")
      .attr("class", "transferGraphNodeTitle")
      .attr("x", 26)
      .attr("y", -2)
      .text((node) => node.transfer.title || "Transfer");
    node.append("text")
      .attr("class", "transferGraphNodeMeta")
      .attr("x", 26)
      .attr("y", 14)
      .text((node) => `#${node.transfer.id} ${node.transfer.status || node.transfer.state || ""}`);
    node.on("click", (event, node) => {
      event.stopPropagation();
      selectedTransferId = node.transfer.id;
      detailMode = "selected";
      persistUiState();
      renderTransferList();
      renderDetail();
      renderTransferGraph();
    });
    svg.on("click", () => {
      selectedTransferId = null;
      persistUiState();
      renderDetail();
      renderTransferGraph();
    });
    graphState.simulation?.stop();
    graphState.simulation = window.d3.forceSimulation(nodes)
      .force("link", window.d3.forceLink(links).id((node) => node.id).distance(120).strength(0.45))
      .force("charge", window.d3.forceManyBody().strength(-260))
      .force("collision", window.d3.forceCollide().radius((node) => node.radius + 26))
      .force("center", window.d3.forceCenter(width / 2, height / 2))
      .on("tick", () => {
        link
          .attr("x1", (link) => link.source.x)
          .attr("y1", (link) => link.source.y)
          .attr("x2", (link) => link.target.x)
          .attr("y2", (link) => link.target.y);
        node.attr("transform", (node) => `translate(${node.x},${node.y})`);
      });
  }

  function fitTransferGraph() {
    if (!window.d3 || !transferGraph || !graphState.zoom || !graphState.viewport) return;
    const bounds = graphState.viewport.node()?.getBBox();
    if (!bounds || !Number.isFinite(bounds.width) || !bounds.width || !bounds.height) return;
    const rect = transferGraph.getBoundingClientRect();
    const width = Math.max(360, rect.width || 900);
    const height = Math.max(320, rect.height || 600);
    const scale = Math.min(2.2, Math.max(0.35, 0.86 / Math.max(bounds.width / width, bounds.height / height)));
    const transform = window.d3.zoomIdentity
      .translate(width / 2 - scale * (bounds.x + bounds.width / 2), height / 2 - scale * (bounds.y + bounds.height / 2))
      .scale(scale);
    window.d3.select(transferGraph).transition().duration(180).call(graphState.zoom.transform, transform);
  }

  function renderGossipList() {
    const query = String(transferSearch?.value || "").trim().toLowerCase();
    const transfers = query
      ? snapshot.gossipTransfers.filter((transfer) => [
        transfer.title,
        transfer.transferUid,
        transfer.proposerLabel,
        transfer.counterpartyLabel,
        transfer.contribution?.head,
        transfer.need?.head,
        transfer.state,
      ].some((value) => String(value || "").toLowerCase().includes(query)))
      : snapshot.gossipTransfers;
    transferCount.textContent = transfers.length + " observed";
    if (!transfers.length) {
      transferList.innerHTML = `<div class="emptyBlock">No observed gossip packages.</div>`;
      return;
    }
    transferList.innerHTML = transfers.map((transfer) => {
      const active = String(transfer.transferUid) === String(selectedGossipTransferUid);
      return `
        <button type="button" class="transferRow" data-gossip-transfer-uid="${escapeHtml(transfer.transferUid)}" data-active="${active ? "true" : "false"}" data-keep-enabled="true">
          <span class="transferTitle">${escapeHtml(transfer.title || "Observed Transfer")}</span>
          <span class="meta">${escapeHtml(shortKey(transfer.transferUid))}</span>
          <span class="meta">updated ${escapeHtml(transfer.updatedAt || "")}</span>
          <span class="chips">${chip("observed", "idle")}${chip(String(transfer.eventCount || 0) + " events", "ok")}</span>
        </button>
      `;
    }).join("");
  }

  function actionButton(action, label, disabled, extra = "") {
    return `<button type="button" data-transfer-action="${escapeHtml(action)}" ${disabled ? "disabled" : ""} ${extra}>${escapeHtml(label)}</button>`;
  }

  function recordSelectHtml(id, selectedId = "") {
    const options = [`<option value="">Select Record</option>`].concat(
      snapshot.records.map((record) => `
        <option value="${escapeHtml(record.id)}" ${Number(selectedId) === Number(record.id) ? "selected" : ""}>
          ${escapeHtml(recordLabel(record))} / ${escapeHtml(formatQuantity(record.quantity))}
        </option>
      `)
    );
    return `<select id="${escapeHtml(id)}">${options.join("")}</select>`;
  }

  function postTargetSelectHtml(id, transfer) {
    const replyOptions = [];
    if (transfer?.sourceBaseUrl) {
      replyOptions.push(`<option value="url:${escapeHtml(transfer.sourceBaseUrl)}">Reply to source (${escapeHtml(transfer.sourceBaseUrl)})</option>`);
    }
    if (transfer?.targetBaseUrl && transfer.targetBaseUrl !== transfer.sourceBaseUrl) {
      replyOptions.push(`<option value="url:${escapeHtml(transfer.targetBaseUrl)}">Transfer target (${escapeHtml(transfer.targetBaseUrl)})</option>`);
    }
    const options = [`<option value="">Select Organ</option>`].concat(
      replyOptions,
      snapshot.organs.map((organ) => `
        <option value="organ:${escapeHtml(organ.id)}">
          ${escapeHtml(organ.name)}${organ.authenticated ? "" : " (login for replies)"}
        </option>
      `)
    );
    return `<select id="${escapeHtml(id)}">${options.join("")}</select>`;
  }

  function workOptions() {
    return snapshot.workAssigneeOptions || { localUsers: [], organs: [] };
  }

  function dateTimeInputValue(value) {
    const text = String(value || "");
    if (!text) return "";
    const date = new Date(text);
    if (!Number.isFinite(date.getTime())) return text.slice(0, 16);
    const offset = date.getTimezoneOffset() * 60000;
    return new Date(date.getTime() - offset).toISOString().slice(0, 16);
  }

  function dateTimePayloadValue(value) {
    const text = String(value || "").trim();
    if (!text) return null;
    const date = new Date(text);
    if (!Number.isFinite(date.getTime())) return text;
    return date.toISOString().slice(0, 19) + "Z";
  }

  function localUserLabel(user) {
    return user?.name || user?.username || ("user " + user?.id);
  }

  function assignmentLabel(assignment) {
    if (assignment.subjectKind === "app_user") {
      const user = workOptions().localUsers.find((item) => Number(item.id) === Number(assignment.appUserId));
      return user ? localUserLabel(user) : (assignment.displayName || "Local user");
    }
    const organ = assignment.organName ? " / " + assignment.organName : "";
    return (assignment.displayName || "External actor") + organ;
  }

  function externalAssignmentAttrs(assignment) {
    return [
      ["displayName", assignment.displayName || ""],
      ["organName", assignment.organName || ""],
      ["remoteBaseUrl", assignment.remoteBaseUrl || ""],
      ["remotePublicKey", assignment.remotePublicKey || ""],
      ["remoteSubjectUid", assignment.remoteSubjectUid || ""],
    ].map(([key, value]) => `data-${key.replace(/[A-Z]/g, (match) => "-" + match.toLowerCase())}="${escapeHtml(value)}"`).join(" ");
  }

  function renderAssigneeControls(prefix, work) {
    const assignments = work?.assignments || [];
    const localIds = new Set(assignments
      .filter((assignment) => assignment.subjectKind === "app_user")
      .map((assignment) => String(assignment.appUserId)));
    const externalAssignments = assignments.filter((assignment) => assignment.subjectKind !== "app_user");
    const localUsers = workOptions().localUsers || [];
    const organs = workOptions().organs || [];
    const localHtml = localUsers.length
      ? localUsers.map((user) => `
          <label class="checkRow assigneeChoice">
            <input type="checkbox" data-work-local-assignee="${escapeHtml(prefix)}" value="${escapeHtml(user.id)}" ${localIds.has(String(user.id)) ? "checked" : ""} data-keep-enabled="true">
            <span>${escapeHtml(localUserLabel(user))}</span>
          </label>
        `).join("")
      : `<div class="emptyBlock">No local users available.</div>`;
    const existingExternal = externalAssignments.length
      ? externalAssignments.map((assignment) => `
          <label class="checkRow assigneeChoice">
            <input type="checkbox" data-work-external-assignee="${escapeHtml(prefix)}" ${externalAssignmentAttrs(assignment)} checked data-keep-enabled="true">
            <span>${escapeHtml(assignmentLabel(assignment))}</span>
          </label>
        `).join("")
      : `<div class="meta">No external assignees.</div>`;
    const organOptions = [`<option value="">No Organ</option>`].concat(
      organs.map((organ) => `<option value="${escapeHtml(organ.id)}">${escapeHtml(organ.name)}</option>`)
    ).join("");
    return `
      <div class="assigneeGrid">
        <div>
          <div class="fieldLabel">Local assignees</div>
          <div class="assigneeStack">${localHtml}</div>
        </div>
        <div>
          <div class="fieldLabel">External assignees</div>
          <div class="assigneeStack">${existingExternal}</div>
          <div class="externalAssigneeNew">
            <input id="${escapeHtml(prefix)}-external-name" placeholder="External name" autocomplete="off" data-keep-enabled="true">
            <select id="${escapeHtml(prefix)}-external-organ" data-keep-enabled="true">${organOptions}</select>
          </div>
        </div>
      </div>
    `;
  }

  function renderWorkPanel(prefix, title, work, action, buttonLabel, extraAttrs = "") {
    return `
      <div class="actionBox workBox">
        <div class="actionTitle">${escapeHtml(title)}</div>
        <div class="workGrid">
          <label><span>Start</span><input id="${escapeHtml(prefix)}-start" type="datetime-local" value="${escapeHtml(dateTimeInputValue(work?.startAt))}" data-keep-enabled="true"></label>
          <label><span>End</span><input id="${escapeHtml(prefix)}-end" type="datetime-local" value="${escapeHtml(dateTimeInputValue(work?.endAt))}" data-keep-enabled="true"></label>
          <label><span>Estimate minutes</span><input id="${escapeHtml(prefix)}-estimate" type="number" min="0" step="1" value="${escapeHtml(work?.estimateSeconds ? Math.round(Number(work.estimateSeconds) / 60) : "")}" data-keep-enabled="true"></label>
          <label class="workNotes"><span>Completion notes</span><textarea id="${escapeHtml(prefix)}-notes" rows="3" data-keep-enabled="true">${escapeHtml(work?.completionNotes || "")}</textarea></label>
        </div>
        ${renderAssigneeControls(prefix, work)}
        <div class="formActions">${actionButton(action, buttonLabel, false, `class="primary" data-work-prefix="${escapeHtml(prefix)}" ${extraAttrs}`)}</div>
      </div>
    `;
  }

  function renderTransferWorkSection(transfer) {
    const items = transfer.items || [];
    const interactions = transfer.interactions || [];
    return `
      <div class="workSection">
        ${renderWorkPanel("transfer-work", "Transfer work", transfer.work || {}, "update-transfer-work", "Save Transfer work")}
        ${items.length ? `
          <div class="workItems">
            ${items.map((item) => `
              <details class="workItem">
                <summary>
                  <span>${escapeHtml(item.title || "Transfer item")}</span>
                  <span class="meta">${escapeHtml(item.role || "")}${item.sourceRecordId ? " / record #" + escapeHtml(item.sourceRecordId) : ""}</span>
                </summary>
                ${renderWorkPanel("item-work-" + item.id, "Item work", item.work || {}, "update-transfer-item-work", "Save item work", `data-structured-item-id="${escapeHtml(item.id)}"`)}
              </details>
            `).join("")}
          </div>
        ` : ""}
        ${interactions.length ? `
          <div class="workItems">
            ${interactions.map((interaction) => `
              <details class="workItem">
                <summary>
                  <span>${escapeHtml((interaction.interactionKind || "interaction").replaceAll("_", " "))}</span>
                  <span class="meta">${escapeHtml(interaction.direction || "")}${interaction.dependencyKind ? " / " + escapeHtml(interaction.dependencyKind) : ""}${interaction.quantity ? " / qty " + escapeHtml(formatQuantity(interaction.quantity)) : ""}</span>
                </summary>
                ${renderWorkPanel("interaction-work-" + interaction.id, "Interaction work", interaction.work || {}, "update-transfer-interaction-work", "Save interaction work", `data-interaction-id="${escapeHtml(interaction.id)}"`)}
              </details>
            `).join("")}
          </div>
        ` : ""}
      </div>
    `;
  }

  function collectWorkPayload(prefix) {
    const estimateMinutes = parseNumber(document.getElementById(prefix + "-estimate")?.value, 0);
    const assignees = [];
    for (const input of app.querySelectorAll(`[data-work-local-assignee="${CSS.escape(prefix)}"]:checked`)) {
      assignees.push({ kind: "appUser", appUserId: Number(input.value) });
    }
    for (const input of app.querySelectorAll(`[data-work-external-assignee="${CSS.escape(prefix)}"]:checked`)) {
      assignees.push({
        kind: "externalActor",
        displayName: input.dataset.displayName || "",
        organName: input.dataset.organName || null,
        remoteBaseUrl: input.dataset.remoteBaseUrl || null,
        remotePublicKey: input.dataset.remotePublicKey || null,
        remoteSubjectUid: input.dataset.remoteSubjectUid || null,
      });
    }
    const externalName = String(document.getElementById(prefix + "-external-name")?.value || "").trim();
    if (externalName) {
      const organId = Number(document.getElementById(prefix + "-external-organ")?.value || 0);
      const organ = workOptions().organs.find((item) => Number(item.id) === organId);
      assignees.push({
        kind: "externalActor",
        displayName: externalName,
        organName: organ?.name || null,
        remoteBaseUrl: organ?.baseUrl || null,
        remotePublicKey: null,
        remoteSubjectUid: null,
      });
    }
    return {
      startAt: dateTimePayloadValue(document.getElementById(prefix + "-start")?.value || ""),
      endAt: dateTimePayloadValue(document.getElementById(prefix + "-end")?.value || ""),
      estimateSeconds: estimateMinutes > 0 ? Math.round(estimateMinutes * 60) : null,
      completionNotes: document.getElementById(prefix + "-notes")?.value || null,
      assignees,
    };
  }

  function sideAgreementLevel(transfer, side) {
    return Number(side === "contribution" ? transfer.agreement?.contribution || 0 : transfer.agreement?.need || 0);
  }

  function sideConclusionDone(transfer, side) {
    return side === "contribution"
      ? Boolean(transfer.confirmations?.delivery)
      : Boolean(transfer.confirmations?.receipt);
  }

  function sideButtonClass(active, done) {
    if (done) return active ? "processDone" : "processDone remoteDone";
    return active ? "processWaiting" : "processIdle";
  }

  function processButtons(transfer, side) {
    const local = transfer.localRole === side;
    const agreementLevel = sideAgreementLevel(transfer, side);
    const otherSide = side === "contribution" ? "need" : "contribution";
    const otherLocked = sideAgreementLevel(transfer, otherSide) >= 1;
    const termsLocked = agreementLevel >= 1;
    const termsAccepted = agreementLevel >= 2;
    const conclusionDone = sideConclusionDone(transfer, side);
    const canLock = local && !termsLocked && transfer.controls?.canSignAgreement;
    const canAccept = local && termsLocked && !termsAccepted && otherLocked && transfer.controls?.canSignAgreement;
    const canConclude = local && !conclusionDone && (
      (side === "contribution" && transfer.controls?.canConfirmDelivery)
      || (side === "need" && transfer.controls?.canConfirmReceipt)
    );
    const canSettle = local && conclusionDone && transfer.controls?.canSettleLocal;
    const concludeAction = canSettle ? "settle-local" : side === "contribution" ? "confirm-delivery" : "confirm-receipt";
    const buttons = [
      {
        step: "lock-terms",
        label: "Lock My Terms",
        action: "sign-agreement",
        enabled: canLock,
        done: termsLocked,
        title: "Lock My Terms signs your current side of the Transfer as ready. It does not mutate Records.",
      },
      {
        step: "accept-terms",
        label: "Accept Terms I Need To Accept",
        action: "sign-agreement",
        enabled: canAccept,
        done: termsAccepted,
        title: "Accept Terms I Need To Accept becomes available after all required parties have locked their terms.",
      },
      {
        step: "confirm-conclusion",
        label: "Confirm Conclusion",
        action: concludeAction,
        enabled: canConclude || canSettle,
        done: conclusionDone || canSettle,
        title: "Confirm Conclusion records that this side's contribution or need has been completed. When both sides are confirmed, this applies your local Record quantity change.",
      },
    ];
    return `
      <div class="processButtons" data-side="${escapeHtml(side)}">
        ${buttons.map((button) => {
          const key = progressActionKey(transfer, side, button.step);
          const confirming = confirmingProgressAction === key;
          const clickable = local && button.enabled && !button.done;
          const active = clickable || confirming;
          const buttonClass = active && local && !button.done ? "processReady" : sideButtonClass(local, button.done);
          const actionAttrs = local && active
            ? `data-progress-action="${escapeHtml(button.action)}" data-progress-side="${escapeHtml(side)}" data-progress-step="${escapeHtml(button.step)}" data-progress-label="${escapeHtml(button.label)}"`
            : "";
          return `<button
            type="button"
            class="${buttonClass}"
            ${actionAttrs}
            ${active ? "" : "disabled"}
            title="${escapeHtml(button.title)}"
          >${escapeHtml(confirming ? "Confirm " + button.label : button.label)}</button>`;
        }).join("")}
      </div>
    `;
  }

  function progressActionKey(transfer, side, step) {
    return [transfer?.id || "", side || "", step || ""].join(":");
  }

  function proposalOrganOptions() {
    return [`<option value="">Local only</option>`].concat(
      snapshot.organs.map((organ) => {
        const auth = organ.authenticated ? "" : " (login for replies)";
        return `<option value="${escapeHtml(organ.id)}">${escapeHtml(organ.name + auth)}</option>`;
      })
    ).join("");
  }

  function renderItemsSection(transfer) {
    const items = transfer.items || [];
    const roleOptions = ["contribution", "need", "support", "task", "information", "reservation"]
      .map((r) => `<option value="${r}">${r}</option>`)
      .join("");
    const itemRows = items.map((item) => `
      <details class="crudItem">
        <summary>
          <span class="label">${escapeHtml(item.title || "item")}</span>
          <span class="meta">${escapeHtml(item.role || "")}${item.quantity != null ? " / " + escapeHtml(formatQuantity(item.quantity)) : ""}${item.unit ? " " + escapeHtml(item.unit) : ""}</span>
        </summary>
        <div class="crudForm formGrid">
          <label><span>Role</span><select id="edit-item-role-${item.id}" data-keep-enabled="true">${roleOptions.replace(`value="${item.role}"`, `value="${item.role}" selected`)}</select></label>
          <label><span>Title</span><input id="edit-item-title-${item.id}" value="${escapeHtml(item.title || "")}" autocomplete="off" data-keep-enabled="true"></label>
          <label><span>Description</span><textarea id="edit-item-desc-${item.id}" rows="2" data-keep-enabled="true">${escapeHtml(item.description || "")}</textarea></label>
          <label><span>Record</span>${recordSelectHtml("edit-item-record-" + item.id, item.sourceRecordId)}</label>
          <label><span>Quantity</span><input id="edit-item-qty-${item.id}" inputmode="decimal" value="${item.quantity != null ? escapeHtml(String(item.quantity)) : ""}" data-keep-enabled="true"></label>
          <label><span>Unit</span><input id="edit-item-unit-${item.id}" value="${escapeHtml(item.unit || "")}" data-keep-enabled="true"></label>
          <div class="formActions">
            ${actionButton("edit-transfer-item", "Save", false, `data-item-id="${item.id}" data-transfer-id="${transfer.id}" class="primary"`)}
            ${actionButton("delete-transfer-item", "Delete", false, `data-item-id="${item.id}" class="danger"`)}
          </div>
        </div>
      </details>
    `).join("");
    return `
      <section class="panel crudSection" aria-labelledby="items-section-title">
        <div class="panelHead">
          <h2 id="items-section-title">Items</h2>
          ${actionButton("settle-all-local", "Settle my parts", false, `data-transfer-id="${transfer.id}" class="primary"`)}
        </div>
        <div class="panelBody">
          ${itemRows || '<p class="muted">No items yet.</p>'}
          <details class="crudItem">
            <summary>Add item</summary>
            <div class="crudForm formGrid">
              <label><span>Role</span><select id="new-item-role" data-keep-enabled="true">${roleOptions}</select></label>
              <label><span>Title</span><input id="new-item-title" autocomplete="off" data-keep-enabled="true"></label>
              <label><span>Description</span><textarea id="new-item-desc" rows="2" data-keep-enabled="true"></textarea></label>
              <label><span>Record</span>${recordSelectHtml("new-item-record")}</label>
              <label><span>Quantity</span><input id="new-item-qty" inputmode="decimal" data-keep-enabled="true"></label>
              <label><span>Unit</span><input id="new-item-unit" data-keep-enabled="true"></label>
              <div class="formActions">
                ${actionButton("create-transfer-item", "Add item", false, `data-transfer-id="${transfer.id}" class="primary"`)}
              </div>
            </div>
          </details>
        </div>
      </section>
    `;
  }

  function renderInteractionsSection(transfer) {
    const items = transfer.items || [];
    const interactions = transfer.interactions || [];
    const itemOptions = [`<option value="">None</option>`]
      .concat(items.map((item) => `<option value="${item.id}">${escapeHtml(item.title || "item " + item.id)}</option>`))
      .join("");
    const kindOptions = ["contributes_to", "depends_on", "unblocks", "replaces", "informs"]
      .map((k) => `<option value="${k}">${k.replaceAll("_", " ")}</option>`)
      .join("");
    const dirOptions = ["outgoing", "incoming", "mutual", "informational"]
      .map((d) => `<option value="${d}">${d}</option>`)
      .join("");
    const depOptions = ["", "must_agree", "must_activate", "must_deliver", "must_receive", "must_settle"]
      .map((d) => `<option value="${d}">${d ? d.replaceAll("_", " ") : "none"}</option>`)
      .join("");
    const terminalStates = new Set(["settled", "satisfied", "complete", "completed", "inactive", "cancelled"]);
    const interactionRows = interactions.map((ia) => {
      const isBlocking = ia.dependencyKind && !terminalStates.has(ia.state || "");
      return `
      <details class="crudItem ${isBlocking ? "blockingDep" : ""}">
        <summary>
          <span class="label">${escapeHtml((ia.interactionKind || "interaction").replaceAll("_", " "))}</span>
          <span class="meta">${escapeHtml(ia.direction || "")}${ia.dependencyKind ? " / " + escapeHtml(ia.dependencyKind) : ""}${ia.quantity ? " / qty " + escapeHtml(formatQuantity(ia.quantity)) : ""}${isBlocking ? ' <span class="blockingBadge">blocking</span>' : ""}</span>
        </summary>
        <div class="crudForm formGrid">
          <label><span>Kind</span><select id="edit-ia-kind-${ia.id}" data-keep-enabled="true">${kindOptions.replace(`value="${ia.interactionKind}"`, `value="${ia.interactionKind}" selected`)}</select></label>
          <label><span>Direction</span><select id="edit-ia-dir-${ia.id}" data-keep-enabled="true">${dirOptions.replace(`value="${ia.direction}"`, `value="${ia.direction}" selected`)}</select></label>
          <label><span>Dependency kind</span><select id="edit-ia-dep-${ia.id}" data-keep-enabled="true">${depOptions.replace(`value="${ia.dependencyKind || ""}"`, `value="${ia.dependencyKind || ""}" selected`)}</select></label>
          <label><span>Quantity</span><input id="edit-ia-qty-${ia.id}" inputmode="decimal" value="${ia.quantity != null ? escapeHtml(String(ia.quantity)) : ""}" data-keep-enabled="true"></label>
          <div class="formActions">
            ${actionButton("edit-transfer-interaction", "Save", false, `data-interaction-id="${ia.id}" class="primary"`)}
            ${actionButton("delete-transfer-interaction", "Delete", false, `data-interaction-id="${ia.id}" class="danger"`)}
          </div>
        </div>
      </details>
    `}).join("");
    return `
      <section class="panel crudSection" aria-labelledby="interactions-section-title">
        <div class="panelHead">
          <h2 id="interactions-section-title">Interactions</h2>
        </div>
        <div class="panelBody">
          ${interactionRows || '<p class="muted">No interactions yet.</p>'}
          <details class="crudItem">
            <summary>Add interaction</summary>
            <div class="crudForm formGrid">
              <label><span>From item</span><select id="new-ia-from" data-keep-enabled="true">${itemOptions}</select></label>
              <label><span>To item</span><select id="new-ia-to" data-keep-enabled="true">${itemOptions}</select></label>
              <label><span>Kind</span><select id="new-ia-kind" data-keep-enabled="true">${kindOptions}</select></label>
              <label><span>Direction</span><select id="new-ia-dir" data-keep-enabled="true">${dirOptions}</select></label>
              <label><span>Dependency kind</span><select id="new-ia-dep" data-keep-enabled="true">${depOptions}</select></label>
              <label><span>Quantity</span><input id="new-ia-qty" inputmode="decimal" data-keep-enabled="true"></label>
              <div class="formActions">
                ${actionButton("create-transfer-interaction", "Add interaction", false, `data-transfer-id="${transfer.id}" class="primary"`)}
              </div>
            </div>
          </details>
        </div>
      </section>
    `;
  }

  function renderMessagesSection(transfer) {
    const messages = transfer.messages || [];
    const topLevel = messages.filter((m) => !m.parentMessageId);
    const byParent = new Map();
    for (const m of messages) {
      if (m.parentMessageId) {
        if (!byParent.has(m.parentMessageId)) byParent.set(m.parentMessageId, []);
        byParent.get(m.parentMessageId).push(m);
      }
    }
    function renderMessage(m) {
      const replies = byParent.get(m.id) || [];
      return `
        <div class="messageItem" data-message-id="${m.id}">
          <div class="messageHead">
            <span class="messageAuthor">${escapeHtml(m.authorLabel || "anonymous")}</span>
            <span class="messageMeta">${escapeHtml(m.createdAt || "")}</span>
            <button type="button" class="link" data-reply-to="${m.id}" data-keep-enabled="true">Reply</button>
            ${actionButton("delete-transfer-message", "Delete", false, `data-message-id="${m.id}" class="link"`)}
          </div>
          <div class="messageBody">${escapeHtml(m.body)}</div>
          ${replies.length ? `<div class="messageReplies">${replies.map(renderMessage).join("")}</div>` : ""}
        </div>
      `;
    }
    const agreementInfo = transfer.agreement || {};
    const agType = agreementInfo.agreementType || "individual";
    let agProgress = "";
    if (agType === "full" || agType === "percentage") {
      const pct = agType === "percentage" ? ` (${agreementInfo.agreementPercentage || 100}%)` : "";
      agProgress = `<span class="meta">${escapeHtml(agType)}${pct}: ${agreementInfo.agreedPartyCount || 0} / ${agreementInfo.partyCount || 0} parties agreed</span>`;
    } else if (agType === "individual") {
      agProgress = `<span class="meta">individual: contribution ${agreementInfo.contribution || 0}/2, need ${agreementInfo.need || 0}/2</span>`;
    }
    return `
      <section class="panel crudSection" aria-labelledby="messages-section-title">
        <div class="panelHead">
          <h2 id="messages-section-title">Messages</h2>
          ${agProgress}
        </div>
        <div class="panelBody">
          <div id="messages-thread">${topLevel.map(renderMessage).join("") || '<p class="muted">No messages yet.</p>'}</div>
          <div id="message-compose" class="messageCompose formGrid">
            <input id="message-reply-to" type="hidden" value="">
            <p id="message-reply-label" class="muted" style="display:none"></p>
            <textarea id="message-body" rows="3" placeholder="Write a message…" data-keep-enabled="true"></textarea>
            <div class="formActions">
              ${actionButton("send-transfer-message", "Send", false, `data-transfer-id="${transfer.id}" class="primary"`)}
              <button type="button" id="message-cancel-reply" class="link" data-keep-enabled="true" style="display:none">Cancel reply</button>
            </div>
          </div>
        </div>
      </section>
    `;
  }

  function renderInfluenceFactsSection(transfer) {
    const facts = transfer.influenceFacts || [];
    if (!facts.length) return "";
    return `
      <section class="transferSection influenceFacts">
        <div class="sectionTitle">Quantity influence</div>
        <div class="influenceFactList">
          ${facts.map(f => {
            const delta = (f.proposedIncoming || 0) - (f.proposedOutgoing || 0);
            const sign = delta > 0 ? "+" : delta < 0 ? "" : "±";
            const surplus = f.surplusQuantity ?? (f.actualQuantity - (f.reservedQuantity || 0) - (f.proposedOutgoing || 0));
            return `
            <div class="influenceFactRow">
              <span class="influenceRecord">${escapeHtml(f.recordHead || "Record #" + f.recordId)}</span>
              <span class="influenceNow" title="actual quantity">${escapeHtml(formatQuantity(f.actualQuantity))}</span>
              <span class="influenceArrow">→</span>
              <span class="influencePlanned" title="planned quantity">${escapeHtml(formatQuantity(f.plannedQuantity))} <span class="influenceDelta">(${sign}${escapeHtml(formatQuantity(Math.abs(delta)))})</span></span>
              ${(f.reservedQuantity || 0) > 0 ? `<span class="influenceReserved" title="hard reserved outgoing">${escapeHtml(formatQuantity(f.reservedQuantity))} reserved</span>` : ""}
              <span class="influenceSurplus ${surplus < 0 ? "negative" : ""}" title="surplus = actual − reserved − proposed outgoing">${escapeHtml(formatQuantity(surplus))} surplus</span>
            </div>`;
          }).join("")}
        </div>
      </section>
    `;
  }

  function renderChainLinksSection(transfer) {
    const links = transfer.chainLinks || [];
    const upstream = links.filter(l => l.downstreamTransferId === transfer.id);
    const downstream = links.filter(l => l.upstreamTransferId === transfer.id);
    const fmt = l => {
      const dir = l.upstreamTransferId === transfer.id ? "→ T#" + l.downstreamTransferId : "← T#" + l.upstreamTransferId;
      const amt = l.amountKind === "percentage" ? l.amountValue + "%" : "constant " + l.amountValue;
      return `<div class="crudItem" data-link-id="${l.id}">
        <span>${escapeHtml(dir)} &mdash; ${escapeHtml(amt)} &mdash; <em>${escapeHtml(l.state)}</em></span>
        ${actionButton("remove-chain-link", "Remove", false, `data-link-id="${l.id}" class="link"`)}
      </div>`;
    };
    return `
      <section class="panel crudSection" aria-labelledby="chainlinks-section-title">
        <div class="panelHead"><h2 id="chainlinks-section-title">Chain Links</h2>
          <span class="meta muted">Private &mdash; not shared in package</span>
        </div>
        <div class="panelBody">
          ${upstream.length ? `<div class="meta">Upstream (feeds this Transfer):</div>${upstream.map(fmt).join("")}` : ""}
          ${downstream.length ? `<div class="meta">Downstream (this Transfer feeds):</div>${downstream.map(fmt).join("")}` : ""}
          ${!links.length ? '<p class="muted">No chain links.</p>' : ""}
          <details class="crudItem">
            <summary>Add chain link</summary>
            <div class="crudForm formGrid">
              <label>Direction
                <select id="chain-direction" data-keep-enabled="true">
                  <option value="upstream">This Transfer feeds another (downstream)</option>
                  <option value="downstream">Another Transfer feeds this one (upstream)</option>
                </select>
              </label>
              <label>Other Transfer ID <input id="chain-other-transfer" type="number" min="1" data-keep-enabled="true"></label>
              <label>Amount kind
                <select id="chain-amount-kind" data-keep-enabled="true">
                  <option value="constant">Constant</option>
                  <option value="percentage">Percentage</option>
                </select>
              </label>
              <label>Amount <input id="chain-amount-value" type="number" step="0.01" data-keep-enabled="true"></label>
              ${actionButton("add-chain-link", "Add link", false, `data-transfer-id="${transfer.id}" class="primary"`)}
            </div>
          </details>
        </div>
      </section>
    `;
  }

  function renderSpectatorsSection(transfer) {
    const spectators = transfer.spectators || [];
    return `
      <section class="panel crudSection" aria-labelledby="spectators-section-title">
        <div class="panelHead"><h2 id="spectators-section-title">Watching</h2>
          <span class="meta muted">Trigger a local Record delta when a source Transfer settles</span>
        </div>
        <div class="panelBody">
          ${spectators.length ? spectators.map(s => `
            <div class="crudItem">
              <span>${escapeHtml(s.watchedRole)} of <code>${escapeHtml(s.watchedSourceTransferUid)}</code>
                &rarr; record #${s.watcherRecordId}
                &mdash; ${escapeHtml(s.amountKind === "percentage" ? s.amountValue + "%" : "constant " + s.amountValue)}
                &mdash; <em>${escapeHtml(s.state)}</em>
              </span>
              ${actionButton("remove-spectator", "Remove", false, `data-spectator-id="${s.id}" class="link"`)}
            </div>
          `).join("") : '<p class="muted">Not watching any source Transfers.</p>'}
          <details class="crudItem">
            <summary>Add watch</summary>
            <div class="crudForm formGrid">
              <label>Source Transfer UID <input id="spectator-source-uid" type="text" placeholder="transfer-uid of the template" data-keep-enabled="true"></label>
              <label>Role to watch
                <select id="spectator-role" data-keep-enabled="true">
                  <option value="need">Need (someone's need being met)</option>
                  <option value="contribution">Contribution (someone contributing)</option>
                </select>
              </label>
              <label>Apply to Record ID <input id="spectator-record-id" type="number" min="1" data-keep-enabled="true"></label>
              <label>Amount kind
                <select id="spectator-amount-kind" data-keep-enabled="true">
                  <option value="constant">Constant</option>
                  <option value="percentage">Percentage</option>
                </select>
              </label>
              <label>Amount <input id="spectator-amount-value" type="number" step="0.01" data-keep-enabled="true"></label>
              ${actionButton("add-spectator", "Add watch", false, `data-transfer-id="${transfer.id}" class="primary"`)}
            </div>
          </details>
        </div>
      </section>
    `;
  }

  function renderSatiationSection(transfer) {
    const current = transfer.satiationPolicy;
    return `
      <section class="panel crudSection" aria-labelledby="satiation-section-title">
        <div class="panelHead"><h2 id="satiation-section-title">Satiation Policy</h2></div>
        <div class="panelBody formGrid">
          <label>Policy for duplicates from same source
            <select id="satiation-policy" data-keep-enabled="true">
              <option value="" ${!current ? "selected" : ""}>Inherit (global default)</option>
              <option value="none" ${current === "none" ? "selected" : ""}>None &mdash; all duplicates proceed independently</option>
              <option value="first_completes" ${current === "first_completes" ? "selected" : ""}>First completes &mdash; cancel siblings when any settles</option>
            </select>
          </label>
          ${actionButton("set-satiation-policy", "Save", false, `data-transfer-id="${transfer.id}"`)}
        </div>
      </section>
    `;
  }

  function renderDetail() {
    if (detailMode === "create") {
      transferDetail.dataset.inactive = "false";
      renderCreateDetail();
      return;
    }
    if (activeTransferTab === "observed") {
      transferDetail.dataset.inactive = "false";
      renderGossipDetail();
      return;
    }
    if (visualizationMode === "graph" && !selectedTransferId) {
      transferDetail.dataset.inactive = "false";
      transferDetail.innerHTML = `<div class="emptyBlock">Click a Transfer node to inspect and edit it.</div>`;
      return;
    }
    const transfer = selectedTransfer();
    if (!transfer) {
      transferDetail.dataset.inactive = "false";
      transferDetail.innerHTML = `<div class="emptyBlock">Select or create a Transfer.</div>`;
      return;
    }
    const controls = transfer.controls || {};
    const packageText = JSON.stringify(transfer.package || {}, null, 2);
    const treeConfig = transfer.tree?.config || {};
    const branchMode = treeConfig.branchMode || "inherit";
    const syncMode = treeConfig.recordSyncMode || "none";
    const reservationPolicy = treeConfig.reservationPolicy || "";
    transferDetail.dataset.inactive = transfer.status === "inactive" ? "true" : "false";
    transferDetail.innerHTML = `
      <div class="transferHero">
        <div class="transferHeroInfo">
          <h2>${escapeHtml(transfer.title || "Transfer")}</h2>
          <div class="meta">last updated ${escapeHtml(transfer.updatedAt || "")} / ${escapeHtml(transfer.status || transfer.state || "")}</div>
        </div>
        <div class="sendControls">
          ${postTargetSelectHtml("post-organ", transfer)}
          ${actionButton("post-transfer", "Send to Organ", false)}
        </div>
      </div>

      <div class="transferParties">
        ${partySection(transfer, "need", transfer.need || {})}
        ${partySection(transfer, "contribution", transfer.contribution || {})}
      </div>

      ${renderTransferWorkSection(transfer)}

      ${renderItemsSection(transfer)}

      ${renderInfluenceFactsSection(transfer)}

      ${renderInteractionsSection(transfer)}

      ${renderMessagesSection(transfer)}

      ${renderChainLinksSection(transfer)}

      ${renderSpectatorsSection(transfer)}

      ${renderSatiationSection(transfer)}

      ${renderTransferVisibilitySection(transfer)}

      <div class="actionGrid">
        <div class="actionBox">
          <div class="actionTitle">Transfer tree</div>
          <div class="meta">parent ${escapeHtml(transfer.tree?.parentId ? "#" + transfer.tree.parentId : "none")} / children ${escapeHtml(String(transfer.tree?.childIds?.length || 0))} / effective ${escapeHtml(transfer.tree?.effectiveBranchMode || "duplicated")}</div>
          <div class="inlineControls">
            <label>
              <span>Branch mode</span>
              <select id="tree-branch-mode" data-keep-enabled="true">
                <option value="inherit" ${branchMode === "inherit" ? "selected" : ""}>Inherit</option>
                <option value="duplicated" ${branchMode === "duplicated" ? "selected" : ""} title="Each child duplicates this transfer's items independently">Duplicated</option>
                <option value="greedy" ${branchMode === "greedy" ? "selected" : ""} title="Children consume from this transfer's quantity pool — first to settle wins the available quantity">Greedy</option>
              </select>
            </label>
            ${actionButton("set-transfer-branch-mode", "Save mode", false)}
            <label>
              <span>Sync</span>
              <select id="tree-sync-mode" data-keep-enabled="true">
                <option value="none" ${syncMode === "none" ? "selected" : ""}>None</option>
                <option value="copy_once" ${syncMode === "copy_once" ? "selected" : ""}>Copy once</option>
                <option value="live" ${syncMode === "live" ? "selected" : ""}>Live</option>
              </select>
            </label>
            ${actionButton("set-transfer-tree-sync-mode", "Save sync", false)}
            ${actionButton("sync-transfer-tree", "Sync now", syncMode !== "live")}
            <label>
              <span>Reservation policy</span>
              <select id="tree-reservation-policy" data-keep-enabled="true">
                <option value="" ${!reservationPolicy ? "selected" : ""}>None (default)</option>
                <option value="soft" ${reservationPolicy === "soft" ? "selected" : ""}>Soft &mdash; planned state</option>
                <option value="hard_on_proposal" ${reservationPolicy === "hard_on_proposal" ? "selected" : ""}>Hard on proposal &mdash; locks on active</option>
                <option value="hard_on_consume" ${reservationPolicy === "hard_on_consume" ? "selected" : ""}>Hard on consume &mdash; locks on settle</option>
                <option value="hard_on_lock" ${reservationPolicy === "hard_on_lock" ? "selected" : ""}>Hard on lock &mdash; locks on lock</option>
              </select>
            </label>
            ${actionButton("set-transfer-reservation-policy", "Save reservation", false)}
          </div>
        </div>
      </div>

      <div class="actionGrid">
        <div class="actionBox">
          <div class="actionTitle">Add child</div>
          <div class="proposalGrid">
            <label><span>Title</span><input id="child-title" value="${escapeHtml((transfer.title || "Transfer") + " child")}" autocomplete="off" data-keep-enabled="true"></label>
            <label><span>Local side</span><select id="child-role" data-keep-enabled="true"><option value="need">Need</option><option value="contribution">Contribution</option></select></label>
            <label><span>Local Record</span><input id="child-record" list="record-options" autocomplete="off" data-keep-enabled="true"></label>
            <label><span>Quantity</span><input id="child-quantity" inputmode="decimal" value="1" data-keep-enabled="true"></label>
            <label><span>Counterparty</span><input id="child-counterparty" value="${escapeHtml(transfer.counterpartyLabel || "")}" list="organ-options" autocomplete="off" data-keep-enabled="true"></label>
            ${actionButton("create-child-transfer", "Create child", false, 'class="primary"')}
          </div>
        </div>
        <div class="actionBox">
          <div class="actionTitle">Import Record tree</div>
          <div class="proposalGrid">
            <label><span>Root Record</span><input id="tree-record-root" list="record-options" autocomplete="off" data-keep-enabled="true"></label>
            <label><span>Local side</span><select id="tree-role" data-keep-enabled="true"><option value="need">Need</option><option value="contribution">Contribution</option></select></label>
            <label><span>Quantity</span><input id="tree-quantity" inputmode="decimal" value="1" data-keep-enabled="true"></label>
            <label><span>Sync mode</span><span><label><input name="tree-sync-create" type="radio" value="live" checked data-keep-enabled="true"> Live</label> <label><input name="tree-sync-create" type="radio" value="copy_once" data-keep-enabled="true"> Copy once</label></span></label>
            ${actionButton("create-transfer-tree-from-record", "Create tree", false, 'class="primary"')}
          </div>
        </div>
      </div>

      <div class="actionGrid">
        <div class="actionBox">
          <div class="actionTitle">Danger zone</div>
          <div class="inlineControls">
            ${actionButton("inactivate-transfer", transfer.status === "inactive" ? "Inactive" : "Inactivate transfer", !controls.canInactivate, 'class="danger"')}
            ${actionButton(
              "delete-transfer",
              Number(pendingDeleteTransferId) === Number(transfer.id) ? "Confirm delete" : "Delete transfer",
              false,
              'class="danger"'
            )}
          </div>
        </div>
      </div>

      <details class="transferPackageArea">
        <summary>Package and events</summary>
        <div class="packageGrid">
          <label>
            <span>Package</span>
            <textarea id="package-output" readonly rows="8" data-keep-enabled="true">${escapeHtml(packageText)}</textarea>
          </label>
          <label>
            <span>Import package</span>
            <textarea id="package-input" rows="8" data-keep-enabled="true"></textarea>
          </label>
          <div class="formActions">
            <button type="button" data-copy-package data-keep-enabled="true">Copy package</button>
            <button type="button" data-import-package data-keep-enabled="true">Import package</button>
          </div>
        </div>
        <section class="eventSection">
          <h2>Events</h2>
          <ol class="events">
            ${(transfer.events || []).length ? transfer.events.map(eventHtml).join("") : `<li class="emptyBlock">No signed events.</li>`}
          </ol>
        </section>
      </details>
    `;
    if (visualizationMode === "graph") {
      transferDetail.insertAdjacentHTML("afterbegin", `
        <button type="button" class="graphDetailClose" data-action="close-graph-detail" data-keep-enabled="true" aria-label="Close Transfer detail">
          <span aria-hidden="true">↑</span>
          <span>Close</span>
        </button>
      `);
    }
    markSelectedTransferSeen(transfer);
  }

  function renderTransferVisibilitySection(transfer) {
    const visibility = transfer.visibility || {};
    const mode = visibility.visibilityMode || "hidden";
    const allowedOrgans = new Set((visibility.organIds || []).map((id) => String(id)));
    const receiptEvents = (transfer.receipt?.events || []).map((event) => `
      <div class="listRow">
        <span>${escapeHtml(event.eventKind === "package_seen" ? "seen" : "received")} by ${escapeHtml(event.actorLabel || "remote")}</span>
        <span class="meta">${escapeHtml(formatDateTime(event.createdAt))}</span>
      </div>
    `).join("");
    const organChecks = snapshot.organs.map((organ) => `
      <label class="checkRow">
        <input type="checkbox" name="visibility-organ" value="${escapeHtml(organ.id)}" ${allowedOrgans.has(String(organ.id)) ? "checked" : ""} data-keep-enabled="true">
        <span>${escapeHtml(organ.name || organ.baseUrl || "Organ")} <span class="meta">proximity ${escapeHtml(organ.proximity ?? 100)}</span></span>
      </label>
    `).join("");
    return `
      <div class="actionGrid">
        <div class="actionBox">
          <div class="actionTitle">Visibility</div>
          <div class="meta">received ${escapeHtml(formatDateTime(transfer.receipt?.receivedAt))} / seen ${escapeHtml(formatDateTime(transfer.receipt?.seenAt))}</div>
          <div class="proposalGrid">
            <label>
              <span>Mode</span>
              <select id="visibility-mode" data-keep-enabled="true">
                <option value="hidden" ${mode === "hidden" ? "selected" : ""}>Hidden</option>
                <option value="public" ${mode === "public" ? "selected" : ""}>Public</option>
                <option value="restricted" ${mode === "restricted" ? "selected" : ""}>Restricted</option>
              </select>
            </label>
            <label>
              <span>Max proximity</span>
              <input id="visibility-proximity" value="${escapeHtml(visibility.maxVisibleProximity ?? "")}" inputmode="numeric" placeholder="restricted only" data-keep-enabled="true">
            </label>
            ${actionButton("set-transfer-visibility", "Save visibility", false)}
            <label>
              <span>Next wave proximity</span>
              <input id="visibility-wave-proximity" value="" inputmode="numeric" placeholder="widen restricted" data-keep-enabled="true">
            </label>
            <label>
              <span>Wave reason</span>
              <input id="visibility-wave-reason" value="karma_wave" data-keep-enabled="true">
            </label>
            ${actionButton("apply-visibility-wave", "Apply wave", mode !== "restricted")}
          </div>
          <div class="compactList">${organChecks || `<div class="emptyBlock">No Organs saved.</div>`}</div>
          <div class="compactList">${receiptEvents || `<div class="emptyBlock">No package receipt events.</div>`}</div>
        </div>
      </div>
    `;
  }

  function markSelectedTransferSeen(transfer) {
    if (!transfer?.id || transfer.receipt?.seenAt || seenMarkingTransfers.has(Number(transfer.id))) return;
    if (snapshot.receiptPolicy?.anonymousPackageViewing) return;
    seenMarkingTransfers.add(Number(transfer.id));
    fetch(actionUrl("mark-transfer-seen"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ transferId: transfer.id }),
    })
      .then((response) => response.json().catch(() => null).then((body) => ({ response, body })))
      .then(({ response, body }) => {
        if (!response.ok) throw new Error(body?.error || "Mark seen failed.");
        snapshot = body?.snapshot || snapshot;
        reconcileSelectedTransfer();
        renderTransferList();
      })
      .catch((error) => {
        console.warn(error);
      })
      .finally(() => {
        seenMarkingTransfers.delete(Number(transfer.id));
      });
  }

  function renderCreateDetail() {
    transferDetail.innerHTML = `
      <div class="detailHeader">
        <div>
          <h2>New Transfer</h2>
          <div class="meta">Create a proposal from one local Record and optionally post it to an Organ.</div>
        </div>
      </div>

      <form id="proposal-form" class="proposalGrid createProposalGrid">
        <label>
          <span>Title</span>
          <input id="proposal-title-input" name="title" value="Record transfer" autocomplete="off">
        </label>
        <label>
          <span>Local side</span>
          <select id="proposal-role" name="role">
            <option value="need">Need</option>
            <option value="contribution">Contribution</option>
          </select>
        </label>
        <label>
          <span>Local Record</span>
          <input id="proposal-record" name="record" list="record-options" autocomplete="off" placeholder="Select Record">
        </label>
        <label>
          <span>Quantity</span>
          <input id="proposal-quantity" name="quantity" inputmode="decimal" value="1">
        </label>
        <label>
          <span>Counterparty</span>
          <input id="proposal-counterparty" name="counterparty" list="organ-options" autocomplete="off" placeholder="other-cell">
        </label>
        <label>
          <span>Topic</span>
          <input id="proposal-topic" name="topic" autocomplete="off" placeholder="donation, repair, food">
        </label>
        <label>
          <span>Post target</span>
          <select id="proposal-organ" name="organ">${proposalOrganOptions()}</select>
        </label>
        <div class="formActions">
          <button type="button" class="primary" data-action="submit-create-proposal">Create proposal</button>
          <button type="button" data-action="cancel-create" data-keep-enabled="true">Cancel</button>
        </div>
      </form>
    `;
  }

  function renderGossipDetail() {
    let transfer = gossipByUid(selectedGossipTransferUid);
    if (!transfer && snapshot.gossipTransfers.length) {
      transfer = snapshot.gossipTransfers[0];
      selectedGossipTransferUid = transfer.transferUid;
    }
    if (!transfer) {
      transferDetail.innerHTML = `<div class="emptyBlock">Select an observed Transfer.</div>`;
      return;
    }
    const packageText = JSON.stringify(transfer.package || {}, null, 2);
    transferDetail.innerHTML = `
      <div class="detailHeader">
        <div>
          <h2>${escapeHtml(transfer.title || "Observed Transfer")}</h2>
          <div class="meta">${escapeHtml(transfer.transferUid || "")}</div>
          <div class="meta">updated ${escapeHtml(transfer.updatedAt || "")}</div>
        </div>
        <div class="chips">
          ${chip("observed", "idle")}
          ${chip(String(transfer.eventCount || 0) + " events", "ok")}
          ${chip(transfer.state || "unknown", "warn")}
        </div>
      </div>

      <div class="sideGrid">
        ${sideHtml("Contribution", transfer.contribution || {})}
        ${sideHtml("Need", transfer.need || {})}
      </div>

      <div class="actionBox">
        <div class="actionTitle">Gossip metadata</div>
        <div class="meta">source ${escapeHtml(transfer.sourceBaseUrl || "unknown")}</div>
        <div class="meta">target ${escapeHtml(transfer.targetBaseUrl || "unknown")}</div>
        <div class="meta">observed from ${escapeHtml(transfer.observedFromBaseUrl || "unknown")}</div>
        <div class="meta">first seen ${escapeHtml(transfer.firstSeenAt || "")}</div>
        <div class="meta">latest event ${escapeHtml(transfer.latestEventCreatedAt || "unknown")}</div>
      </div>

      <div class="packageGrid">
        <label>
          <span>Read-only package</span>
          <textarea readonly rows="10" data-keep-enabled="true">${escapeHtml(packageText)}</textarea>
        </label>
      </div>
    `;
  }

  function eventHtml(event) {
    const payload = JSON.stringify(event.payload || {}, null, 2);
    return `
      <li class="event">
        <div class="eventName">
          <span>#${escapeHtml(event.id)} ${escapeHtml(event.eventKind)}</span>
          ${chip(event.signatureValid ? "signature ok" : "bad signature", event.signatureValid ? "ok" : "danger")}
        </div>
        <div class="meta">${escapeHtml(event.actorLabel)} / ${escapeHtml(shortKey(event.actorPublicKey))} / ${escapeHtml(event.createdAt || "")}</div>
        <pre>${escapeHtml(payload)}</pre>
      </li>
    `;
  }

  function render() {
    renderIdentity();
    renderIngress();
    renderNetwork();
    renderRecords();
    renderOrgans();
    renderTransferList();
    renderDetail();
    renderTransferGraph();
    updateStaticDisabledState();
  }

  function updateStaticDisabledState() {
    if (proposalSubmit) {
      proposalSubmit.disabled = busy;
      proposalSubmit.title = "";
    }
  }

  async function loadContract() {
    if (contractLoading) {
      reloadQueued = true;
      return;
    }
    contractLoading = true;
    setBusy(true);
    try {
      const response = await fetch(contractUrl(), { cache: "no-store" });
      const body = await response.json().catch(() => null);
      if (!response.ok) throw new Error(body?.error || "Unable to load Transfer contract.");
      snapshot = body?.snapshot || snapshot;
      reconcileSelectedTransfer();
      render();
      setStatus("Ready", "ok");
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error), "danger");
    } finally {
      contractLoading = false;
      setBusy(false);
      if (reloadQueued) {
        reloadQueued = false;
        loadContract();
      }
    }
  }

  function connectTransferStream() {
    if (!window.EventSource || !instanceId) return;
    const source = new EventSource(streamUrl());
    source.addEventListener("transfer-changed", () => {
      loadContract();
    });
    source.onerror = () => {
      source.close();
      window.setTimeout(connectTransferStream, 2000);
    };
  }

  async function postAction(action, payload) {
    setBusy(true);
    setStatus("Working...", "warn");
    try {
      const response = await fetch(actionUrl(action), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload || {}),
      });
      const body = await response.json().catch(() => null);
      if (!response.ok) throw new Error(body?.error || `Action ${action} failed.`);
      snapshot = body?.snapshot || snapshot;
      confirmingProgressAction = null;
      if (action === "create-proposal") {
        activeTransferTab = "mine";
        detailMode = "selected";
        selectedTransferId = null;
        selectedGossipTransferUid = null;
        persistUiState();
      }
      reconcileSelectedTransfer();
      render();
      setStatus(body?.message || "Done", "ok");
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error), "danger");
    } finally {
      setBusy(false);
    }
  }

  async function discoverContacts() {
    const baseUrl = String(contactDiscoveryBaseUrl.value || "").trim().replace(/\/+$/, "");
    if (!baseUrl) {
      setStatus("Enter a node URL first.", "danger");
      contactDiscoveryBaseUrl.focus();
      return;
    }
    const query = new URLSearchParams();
    const search = String(contactDiscoverySearch.value || "").trim();
    if (search) query.set("search", search);
    query.set("limit", "50");
    setBusy(true);
    setStatus("Discovering contacts...", "warn");
    try {
      const response = await fetch(baseUrl + "/organs/discover?" + query.toString());
      const body = await response.json().catch(() => null);
      if (!response.ok) throw new Error(body?.error || "Contact discovery failed.");
      discoveredContacts = Array.isArray(body?.contacts) ? body.contacts : [];
      renderNetwork();
      setStatus("Contact discovery loaded.", "ok");
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error), "danger");
    } finally {
      setBusy(false);
    }
  }

  async function addDiscoveredContact(index) {
    const contact = discoveredContacts[Number(index)];
    if (!contact?.baseUrl) {
      setStatus("Discovered contact is missing a base URL.", "danger");
      return;
    }
    setBusy(true);
    setStatus("Adding contact...", "warn");
    try {
      const response = await fetch("/transfer/contacts", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name: contact.name || contact.baseUrl,
          baseUrl: contact.baseUrl,
        }),
      });
      const body = await response.json().catch(() => null);
      if (!response.ok) throw new Error(body?.error || "Adding contact failed.");
      await loadContract();
      setStatus("Contact added as unknown.", "ok");
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error), "danger");
    } finally {
      setBusy(false);
    }
  }

  async function patchPeer(organ, patch) {
    setBusy(true);
    setStatus("Updating peer...", "warn");
    try {
      const response = await fetch("/organ/" + encodeURIComponent(String(organ.id)), {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name: organ.name,
          base_url: organ.baseUrl,
          trust_state: patch.trustState ?? organ.trustState,
          contact_discovery_enabled: patch.contactDiscoveryEnabled ?? Boolean(organ.contactDiscoveryEnabled),
        }),
      });
      const body = await response.json().catch(() => null);
      if (!response.ok) throw new Error(body?.error || "Peer update failed.");
      await loadContract();
      setStatus("Peer updated.", "ok");
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error), "danger");
    } finally {
      setBusy(false);
    }
  }

  function saveIdentity() {
    const label = String(identityLabel.value || "").trim();
    if (!label) {
      setStatus("Enter a local party label first.", "danger");
      identityLabel.focus();
      return;
    }
    postAction("configure-local-party", { label: identityLabel.value });
  }

  identitySave.addEventListener("click", saveIdentity);

  identityReset.addEventListener("click", () => {
    setResetModalOpen(true);
  });

  identityForm.addEventListener("submit", (event) => {
    event.preventDefault();
    saveIdentity();
  });

  ingressForm.addEventListener("submit", (event) => {
    event.preventDefault();
    postAction("set-ingress-policy", {
      publicProposalsEnabled: Boolean(publicProposalsEnabled.checked),
    });
  });

  networkPolicyForm.addEventListener("submit", (event) => {
    event.preventDefault();
    postAction("set-network-policy", {
      knownPeerPollingEnabled: Boolean(knownPeerPollingEnabled.checked),
      shareQuantityProjections: Boolean(shareQuantityProjections.checked),
    });
  });

  receiptPolicyForm.addEventListener("submit", (event) => {
    event.preventDefault();
    postAction("set-receipt-policy", {
      sendReceivedReceipts: Boolean(sendReceivedReceipts.checked),
      sendSeenReceipts: Boolean(sendSeenReceipts.checked),
      anonymousPackageViewing: Boolean(anonymousPackageViewing.checked),
    });
  });

  globalSatiationForm?.addEventListener("submit", (event) => {
    event.preventDefault();
    const policy = globalSatiationPolicy?.value || null;
    postAction("set-global-satiation-policy", { policy: policy || null });
  });

  contactDiscoveryForm.addEventListener("submit", (event) => {
    event.preventDefault();
    discoverContacts();
  });

  organLoginForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    const data = new FormData(organLoginForm);
    const organId = Number(data.get("organ") || 0);
    if (!organId) {
      setStatus("Select an Organ to login.", "danger");
      return;
    }
    setBusy(true);
    setStatus("Logging in...", "warn");
    try {
      const response = await fetch("/organ/" + encodeURIComponent(String(organId)) + "/session", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          username: data.get("username") || "",
          password: data.get("password") || "",
        }),
      });
      const body = await response.json().catch(() => null);
      if (!response.ok) throw new Error(body?.error || "Organ login failed.");
      await loadContract();
      setStatus("Organ session connected.", "ok");
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error), "danger");
    } finally {
      setBusy(false);
    }
  });

  recordForm.addEventListener("submit", (event) => {
    event.preventDefault();
    const data = new FormData(recordForm);
    postAction("create-record", {
      head: data.get("head") || null,
      body: data.get("body") || null,
      quantity: parseNumber(data.get("quantity"), 1),
    });
  });

  function createProposal() {
    const proposalForm = document.getElementById("proposal-form");
    if (!proposalForm) {
      detailMode = "create";
      selectedTransferId = null;
      selectedGossipTransferUid = null;
      renderTransferList();
      renderDetail();
      return;
    }
    const data = new FormData(proposalForm);
    const recordId = selectedRecordId(data.get("record"));
    if (!recordId) {
      setStatus("Select a local Record first.", "danger");
      return;
    }
    const organId = selectedOrganId(data.get("organ"));
    const selectedOrgan = snapshot.organs.find((organ) => Number(organ.id) === Number(organId));
    postAction("create-proposal", {
      title: data.get("title") || "Transfer",
      role: data.get("role") || "need",
      recordId,
      quantity: parseNumber(data.get("quantity"), 1),
      counterpartyLabel: data.get("counterparty") || selectedOrgan?.name || "",
      targetOrganId: organId || null,
      topicText: data.get("topic") || null,
    });
  }

  proposalSubmit.addEventListener("click", () => {
    detailMode = "create";
    selectedTransferId = null;
    selectedGossipTransferUid = null;
    pendingDeleteTransferId = null;
    confirmingProgressAction = null;
    renderTransferList();
    renderDetail();
  });

  transferSearch.addEventListener("input", () => {
    selectedTransferId = null;
    selectedGossipTransferUid = null;
    confirmingProgressAction = null;
    renderTransferList();
    renderDetail();
    renderTransferGraph();
  });

  app.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    const toggle = event.target.closest("[data-transfer-toggle]");
    if (!toggle) return;
    event.preventDefault();
    toggleTransferBranch(toggle.dataset.transferToggle || "");
  });

  app.addEventListener("click", async (event) => {
    const openSettings = event.target.closest("[data-action='open-settings']");
    if (openSettings) {
      setSettingsOpen(true);
      return;
    }

    const closeSettings = event.target.closest("[data-action='close-settings']");
    if (closeSettings) {
      setSettingsOpen(false);
      return;
    }

    const toggleTransferVisualization = event.target.closest("[data-action='toggle-transfer-visualization']");
    if (toggleTransferVisualization) {
      setVisualizationMode(visualizationMode === "graph" ? "structured" : "graph");
      return;
    }

    const graphTransferView = event.target.closest("[data-action='graph-transfer-view']");
    if (graphTransferView) {
      setVisualizationMode("graph");
      setSettingsOpen(false);
      return;
    }

    const structuredTransferView = event.target.closest("[data-action='structured-transfer-view']");
    if (structuredTransferView) {
      setVisualizationMode("structured");
      return;
    }

    const fitGraph = event.target.closest("[data-action='fit-transfer-graph']");
    if (fitGraph) {
      fitTransferGraph();
      return;
    }

    const closeGraphDetail = event.target.closest("[data-action='close-graph-detail']");
    if (closeGraphDetail) {
      selectedTransferId = null;
      persistUiState();
      renderDetail();
      renderTransferGraph();
      return;
    }

    const cancelResetKey = event.target.closest("[data-action='cancel-reset-key']");
    if (cancelResetKey) {
      setResetModalOpen(false);
      return;
    }

    const confirmResetKey = event.target.closest("[data-action='confirm-reset-key']");
    if (confirmResetKey) {
      const label = String(identityLabel.value || snapshot.localIdentity?.label || "local-cell").trim();
      setResetModalOpen(false);
      postAction("reset-local-party", { label });
      return;
    }

    const refreshButton = event.target.closest("[data-action='refresh']");
    if (refreshButton) {
      postAction("refresh");
      return;
    }

    const addContact = event.target.closest("[data-contact-add]");
    if (addContact) {
      addDiscoveredContact(addContact.dataset.contactAdd);
      return;
    }

    const peerAction = event.target.closest("[data-peer-action]");
    if (peerAction) {
      const organ = snapshot.organs.find((item) => Number(item.id) === Number(peerAction.dataset.peerId || 0));
      if (!organ) {
        setStatus("Peer not found.", "danger");
        return;
      }
      const action = peerAction.dataset.peerAction || "";
      if (action === "poll") {
        postAction("poll-transfer-peer", { organId: Number(organ.id) });
        return;
      }
      if (action === "save-proximity") {
        const proximityInput = document.querySelector(`[data-peer-proximity="${CSS.escape(String(organ.id))}"]`);
        postAction("set-organ-proximity", {
          organId: Number(organ.id),
          proximity: Math.max(0, Math.trunc(parseNumber(proximityInput?.value, organ.proximity ?? 100))),
        });
        return;
      }
      if (action === "save-receipts") {
        const receivedInput = document.querySelector(`[data-peer-received-receipts="${CSS.escape(String(organ.id))}"]`);
        const seenInput = document.querySelector(`[data-peer-seen-receipts="${CSS.escape(String(organ.id))}"]`);
        postAction("set-organ-receipt-policy", {
          organId: Number(organ.id),
          sendReceivedReceipts: Boolean(receivedInput?.checked),
          sendSeenReceipts: Boolean(seenInput?.checked),
        });
        return;
      }
      if (action === "trust-known") {
        patchPeer(organ, { trustState: "known" });
        return;
      }
      if (action === "trust-unknown") {
        patchPeer(organ, { trustState: "unknown" });
        return;
      }
      if (action === "trust-blocked") {
        patchPeer(organ, { trustState: "blocked" });
        return;
      }
      if (action === "show-contact") {
        patchPeer(organ, { contactDiscoveryEnabled: true });
        return;
      }
      if (action === "hide-contact") {
        patchPeer(organ, { contactDiscoveryEnabled: false });
        return;
      }
    }

    const cancelCreate = event.target.closest("[data-action='cancel-create']");
    if (cancelCreate) {
      detailMode = "selected";
      persistUiState();
      renderDetail();
      return;
    }

    const submitCreateProposal = event.target.closest("[data-action='submit-create-proposal']");
    if (submitCreateProposal) {
      createProposal();
      return;
    }

    const tabButton = event.target.closest("[data-transfer-tab]");
    if (tabButton) {
      activeTransferTab = tabButton.dataset.transferTab || "mine";
      selectedTransferId = null;
      selectedGossipTransferUid = null;
      pendingDeleteTransferId = null;
      confirmingProgressAction = null;
      detailMode = "selected";
      persistUiState();
      renderTransferList();
      renderDetail();
      return;
    }

    const fillRecord = event.target.closest("[data-fill-record]");
    if (fillRecord) {
      detailMode = "create";
      persistUiState();
      renderDetail();
      const proposalRecord = document.getElementById("proposal-record");
      if (proposalRecord) {
        proposalRecord.value = fillRecord.dataset.fillRecord || "";
      }
      return;
    }

    const gossipRow = event.target.closest("[data-gossip-transfer-uid]");
    if (gossipRow) {
      selectedGossipTransferUid = gossipRow.dataset.gossipTransferUid || "";
      detailMode = "selected";
      persistUiState();
      renderTransferList();
      renderDetail();
      return;
    }

    const toggle = event.target.closest("[data-transfer-toggle]");
    if (toggle) {
      event.preventDefault();
      event.stopPropagation();
      toggleTransferBranch(toggle.dataset.transferToggle || "");
      return;
    }

    const row = event.target.closest("[data-transfer-id]");
    if (row) {
      selectedTransferId = Number(row.dataset.transferId);
      pendingDeleteTransferId = null;
      confirmingProgressAction = null;
      detailMode = "selected";
      persistUiState();
      renderTransferList();
      renderDetail();
      return;
    }

    const transfer = selectedTransfer();
    if (!transfer) return;

    const progressButton = event.target.closest("[data-progress-action]");
    if (progressButton) {
      if (progressButton.disabled) return;
      const action = progressButton.dataset.progressAction || "";
      const side = progressButton.dataset.progressSide || "";
      const step = progressButton.dataset.progressStep || action;
      if (side !== transfer.localRole) return;
      const label = progressButton.dataset.progressLabel || progressButton.textContent || action;
      const key = progressActionKey(transfer, side, step);
      if (confirmingProgressAction !== key) {
        confirmingProgressAction = key;
        renderDetail();
        setStatus("Click Confirm " + label + " to continue.", "warn");
        return;
      }
      confirmingProgressAction = null;
      postAction(action, { transferId: transfer.id });
      return;
    }

    const transferAction = event.target.closest("[data-transfer-action]");
    if (transferAction) {
      if (transferAction.disabled) return;
      const action = transferAction.dataset.transferAction;
      if (action === "update-transfer-local-item") {
        const recordId = selectedRecordId(document.getElementById("local-record-input")?.value || "");
        if (!recordId) {
          setStatus("Select a local Record first.", "danger");
          return;
        }
        postAction(action, {
          transferId: transfer.id,
          title: transfer.title || "Transfer",
          itemTitle: document.getElementById("local-item-title")?.value || "Record",
          recordId,
          quantity: parseNumber(document.getElementById("local-quantity-input")?.value, transfer.quantity || 1),
        });
        return;
      }
      if (action === "update-transfer-work") {
        const prefix = transferAction.dataset.workPrefix || "transfer-work";
        postAction(action, {
          transferId: transfer.id,
          work: collectWorkPayload(prefix),
        });
        return;
      }
      if (action === "update-transfer-item-work") {
        const prefix = transferAction.dataset.workPrefix || "";
        const structuredItemId = Number(transferAction.dataset.structuredItemId || 0);
        if (!prefix || !structuredItemId) {
          setStatus("Missing Transfer item work target.", "danger");
          return;
        }
        postAction(action, {
          transferId: transfer.id,
          structuredItemId,
          work: collectWorkPayload(prefix),
        });
        return;
      }
      if (action === "update-transfer-interaction-work") {
        const prefix = transferAction.dataset.workPrefix || "";
        const interactionId = Number(transferAction.dataset.interactionId || 0);
        if (!prefix || !interactionId) {
          setStatus("Missing Transfer interaction work target.", "danger");
          return;
        }
        postAction(action, {
          transferId: transfer.id,
          interactionId,
          work: collectWorkPayload(prefix),
        });
        return;
      }
      if (action === "duplicate-proposal") {
        const role = document.getElementById("duplicate-role")?.value || "contribution";
        const recordId = Number(document.getElementById("duplicate-record")?.value || 0);
        if (!recordId) {
          setStatus("Select a local Record for the duplicate.", "danger");
          return;
        }
        postAction(action, {
          transferId: transfer.id,
          localRole: role,
          localRecordId: recordId,
        });
        return;
      }
      if (action === "post-transfer") {
        const target = String(document.getElementById("post-organ")?.value || "");
        if (!target) {
          setStatus("Select an Organ or reply target.", "danger");
          return;
        }
        if (target.startsWith("url:")) {
          postAction(action, {
            transferId: transfer.id,
            baseUrl: target.slice(4),
          });
        } else {
          postAction(action, {
            transferId: transfer.id,
            organId: Number(target.replace(/^organ:/, "")),
          });
        }
        return;
      }
      if (action === "create-child-transfer") {
        const recordId = selectedRecordId(document.getElementById("child-record")?.value || "");
        if (!recordId) {
          setStatus("Select a local Record for the child Transfer.", "danger");
          return;
        }
        postAction(action, {
          parentTransferId: transfer.id,
          title: document.getElementById("child-title")?.value || "Child Transfer",
          role: document.getElementById("child-role")?.value || "need",
          recordId,
          quantity: parseNumber(document.getElementById("child-quantity")?.value, 1),
          counterpartyLabel: document.getElementById("child-counterparty")?.value || transfer.counterpartyLabel || "",
          targetOrganId: transfer.targetOrganId || null,
        });
        return;
      }
      if (action === "create-transfer-tree-from-record") {
        const rootRecordId = selectedRecordId(document.getElementById("tree-record-root")?.value || "");
        if (!rootRecordId) {
          setStatus("Select a root Record for the Transfer tree.", "danger");
          return;
        }
        const syncInput = app.querySelector("input[name='tree-sync-create']:checked");
        postAction(action, {
          parentTransferId: transfer.id,
          rootRecordId,
          role: document.getElementById("tree-role")?.value || "need",
          quantity: parseNumber(document.getElementById("tree-quantity")?.value, 1),
          counterpartyLabel: transfer.counterpartyLabel || "",
          targetOrganId: transfer.targetOrganId || null,
          recordSyncMode: syncInput?.value || "live",
        });
        return;
      }
      if (action === "set-transfer-branch-mode") {
        postAction(action, {
          transferId: transfer.id,
          branchMode: document.getElementById("tree-branch-mode")?.value || "inherit",
        });
        return;
      }
      if (action === "set-transfer-tree-sync-mode") {
        postAction(action, {
          transferId: transfer.id,
          recordSyncMode: document.getElementById("tree-sync-mode")?.value || "none",
        });
        return;
      }
      if (action === "set-transfer-reservation-policy") {
        const policy = document.getElementById("tree-reservation-policy")?.value || null;
        postAction(action, { transferId: transfer.id, reservationPolicy: policy || null });
        return;
      }
      if (action === "set-transfer-visibility") {
        const mode = document.getElementById("visibility-mode")?.value || "hidden";
        const proximityRaw = String(document.getElementById("visibility-proximity")?.value || "").trim();
        const organIds = Array.from(app.querySelectorAll("input[name='visibility-organ']:checked"))
          .map((input) => Number(input.value))
          .filter((id) => Number.isFinite(id) && id > 0);
        postAction(action, {
          transferId: transfer.id,
          visibilityMode: mode,
          maxVisibleProximity: mode === "restricted" && proximityRaw ? Math.max(0, Math.trunc(parseNumber(proximityRaw, 0))) : null,
          organIds: mode === "restricted" ? organIds : [],
        });
        return;
      }
      if (action === "apply-visibility-wave") {
        const proximityRaw = String(document.getElementById("visibility-wave-proximity")?.value || "").trim();
        postAction(action, {
          transferId: transfer.id,
          maxVisibleProximity: proximityRaw ? Math.max(0, Math.trunc(parseNumber(proximityRaw, 0))) : null,
          reason: document.getElementById("visibility-wave-reason")?.value || "karma_wave",
        });
        return;
      }
      if (action === "sync-transfer-tree") {
        postAction(action, { transferId: transfer.id });
        return;
      }
      if (action === "delete-transfer") {
        if (Number(pendingDeleteTransferId) !== Number(transfer.id)) {
          pendingDeleteTransferId = transfer.id;
          renderDetail();
          setStatus("Click Confirm delete to remove this Transfer from this node.", "warn");
          return;
        }
        pendingDeleteTransferId = null;
      }
      if (action === "create-transfer-item") {
        const transferId = Number(transferAction.dataset.transferId || transfer.id);
        postAction(action, {
          transferId,
          role: document.getElementById("new-item-role")?.value || "contribution",
          title: document.getElementById("new-item-title")?.value || "",
          description: document.getElementById("new-item-desc")?.value || null,
          sourceRecordId: Number(document.getElementById("new-item-record")?.value || 0) || null,
          quantity: parseNumber(document.getElementById("new-item-qty")?.value, null),
          unit: document.getElementById("new-item-unit")?.value || null,
        });
        return;
      }
      if (action === "edit-transfer-item") {
        const itemId = Number(transferAction.dataset.itemId || 0);
        const transferId = Number(transferAction.dataset.transferId || transfer.id);
        if (!itemId) { setStatus("Missing item id.", "danger"); return; }
        postAction(action, {
          itemId,
          transferId,
          role: document.getElementById("edit-item-role-" + itemId)?.value || null,
          title: document.getElementById("edit-item-title-" + itemId)?.value || null,
          description: document.getElementById("edit-item-desc-" + itemId)?.value || null,
          sourceRecordId: Number(document.getElementById("edit-item-record-" + itemId)?.value || 0) || null,
          quantity: parseNumber(document.getElementById("edit-item-qty-" + itemId)?.value, null),
          unit: document.getElementById("edit-item-unit-" + itemId)?.value || null,
        });
        return;
      }
      if (action === "delete-transfer-item") {
        const itemId = Number(transferAction.dataset.itemId || 0);
        if (!itemId) { setStatus("Missing item id.", "danger"); return; }
        postAction(action, { itemId });
        return;
      }
      if (action === "create-transfer-interaction") {
        const transferId = Number(transferAction.dataset.transferId || transfer.id);
        postAction(action, {
          transferId,
          fromItemId: Number(document.getElementById("new-ia-from")?.value || 0) || null,
          toItemId: Number(document.getElementById("new-ia-to")?.value || 0) || null,
          interactionKind: document.getElementById("new-ia-kind")?.value || "contributes_to",
          direction: document.getElementById("new-ia-dir")?.value || "outgoing",
          dependencyKind: document.getElementById("new-ia-dep")?.value || null,
          quantity: parseNumber(document.getElementById("new-ia-qty")?.value, null),
        });
        return;
      }
      if (action === "edit-transfer-interaction") {
        const interactionId = Number(transferAction.dataset.interactionId || 0);
        if (!interactionId) { setStatus("Missing interaction id.", "danger"); return; }
        postAction(action, {
          interactionId,
          interactionKind: document.getElementById("edit-ia-kind-" + interactionId)?.value || null,
          direction: document.getElementById("edit-ia-dir-" + interactionId)?.value || null,
          dependencyKind: document.getElementById("edit-ia-dep-" + interactionId)?.value || null,
          quantity: parseNumber(document.getElementById("edit-ia-qty-" + interactionId)?.value, null),
        });
        return;
      }
      if (action === "delete-transfer-interaction") {
        const interactionId = Number(transferAction.dataset.interactionId || 0);
        if (!interactionId) { setStatus("Missing interaction id.", "danger"); return; }
        postAction(action, { interactionId });
        return;
      }
      if (action === "send-transfer-message") {
        const transferId = Number(transferAction.dataset.transferId || transfer.id);
        const body = String(document.getElementById("message-body")?.value || "").trim();
        if (!body) { setStatus("Message body is empty.", "danger"); return; }
        const parentMessageId = Number(document.getElementById("message-reply-to")?.value || 0) || null;
        postAction(action, {
          transferId,
          body,
          parentMessageId,
          interactionId: null,
        });
        document.getElementById("message-body").value = "";
        document.getElementById("message-reply-to").value = "";
        document.getElementById("message-reply-label").style.display = "none";
        document.getElementById("message-cancel-reply").style.display = "none";
        return;
      }
      if (action === "delete-transfer-message") {
        const messageId = Number(transferAction.dataset.messageId || 0);
        if (!messageId) { setStatus("Missing message id.", "danger"); return; }
        postAction(action, { messageId });
        return;
      }
      if (action === "add-chain-link") {
        const direction = String(document.getElementById("chain-direction")?.value || "upstream");
        const otherTransferId = Number(document.getElementById("chain-other-transfer")?.value || 0);
        const amountKind = String(document.getElementById("chain-amount-kind")?.value || "constant");
        const amountValue = Number(document.getElementById("chain-amount-value")?.value || 0);
        if (!otherTransferId) { setStatus("Enter the other Transfer ID.", "danger"); return; }
        const upstreamTransferId = direction === "upstream" ? transfer.id : otherTransferId;
        const downstreamTransferId = direction === "upstream" ? otherTransferId : transfer.id;
        postAction(action, { upstreamTransferId, upstreamItemId: null, downstreamTransferId, downstreamItemId: null, amountKind, amountValue });
        return;
      }
      if (action === "remove-chain-link") {
        const linkId = Number(transferAction.dataset.linkId || 0);
        if (!linkId) { setStatus("Missing link id.", "danger"); return; }
        postAction(action, { linkId });
        return;
      }
      if (action === "add-spectator") {
        const sourceUid = String(document.getElementById("spectator-source-uid")?.value || "").trim();
        const watchedRole = String(document.getElementById("spectator-role")?.value || "need");
        const recordId = Number(document.getElementById("spectator-record-id")?.value || 0);
        const amountKind = String(document.getElementById("spectator-amount-kind")?.value || "constant");
        const amountValue = Number(document.getElementById("spectator-amount-value")?.value || 0);
        if (!sourceUid) { setStatus("Enter the source Transfer UID.", "danger"); return; }
        if (!recordId) { setStatus("Enter the Record ID to receive the delta.", "danger"); return; }
        postAction(action, { watcherRecordId: recordId, watchedSourceTransferUid: sourceUid, watchedRole, amountKind, amountValue });
        return;
      }
      if (action === "remove-spectator") {
        const spectatorId = Number(transferAction.dataset.spectatorId || 0);
        if (!spectatorId) { setStatus("Missing spectator id.", "danger"); return; }
        postAction(action, { spectatorId });
        return;
      }
      if (action === "set-satiation-policy") {
        const policy = String(document.getElementById("satiation-policy")?.value || "") || null;
        postAction(action, { transferId: transfer.id, policy });
        return;
      }
      if (action === "settle-all-local") {
        postAction(action, { transferId: transfer.id });
        return;
      }
      postAction(action, { transferId: transfer.id });
      return;
    }

    const replyButton = event.target.closest("[data-reply-to]");
    if (replyButton) {
      const messageId = replyButton.dataset.replyTo;
      const replyInput = document.getElementById("message-reply-to");
      const replyLabel = document.getElementById("message-reply-label");
      const cancelBtn = document.getElementById("message-cancel-reply");
      if (replyInput) replyInput.value = messageId;
      if (replyLabel) { replyLabel.textContent = `Replying to message #${messageId}`; replyLabel.style.display = ""; }
      if (cancelBtn) cancelBtn.style.display = "";
      document.getElementById("message-body")?.focus();
      return;
    }
    const cancelReply = event.target.closest("#message-cancel-reply");
    if (cancelReply) {
      const replyInput = document.getElementById("message-reply-to");
      const replyLabel = document.getElementById("message-reply-label");
      if (replyInput) replyInput.value = "";
      if (replyLabel) replyLabel.style.display = "none";
      cancelReply.style.display = "none";
      return;
    }
    if (event.target.closest("[data-copy-package]")) {
      const output = document.getElementById("package-output");
      try {
        await navigator.clipboard.writeText(output?.value || "");
        setStatus("Package copied.", "ok");
      } catch (_) {
        output?.select();
        setStatus("Package selected.", "warn");
      }
      return;
    }

    if (event.target.closest("[data-import-package]")) {
      const input = document.getElementById("package-input");
      const raw = String(input?.value || "").trim();
      if (!raw) {
        setStatus("Paste a Transfer package first.", "danger");
        return;
      }
      postAction("import-package", { package: raw });
    }
  });

  app.addEventListener("submit", (event) => {
    if (!event.target.closest("#proposal-form")) return;
    event.preventDefault();
    createProposal();
  });

  app.addEventListener("change", (event) => {
    const proposalOrgan = event.target.closest("#proposal-organ");
    if (!proposalOrgan) return;
    const organId = selectedOrganId(proposalOrgan.value);
    const organ = snapshot.organs.find((item) => Number(item.id) === Number(organId));
    const proposalCounterparty = document.getElementById("proposal-counterparty");
    if (organ && proposalCounterparty && !proposalCounterparty.value.trim()) {
      proposalCounterparty.value = organ.name;
    }
  });

  window.addEventListener("resize", () => {
    if (visualizationMode === "graph") {
      renderTransferGraph();
    }
  });

  connectTransferStream();
  bindHostState();
  setVisualizationMode(visualizationMode);
  loadContract();
})();
"##.to_string()
}
