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

  const frame = window.frameElement;
  const instanceId = String(frame?.dataset?.packageInstanceId || "").trim();

  let snapshot = {
    localIdentity: null,
    ingressPolicy: { publicProposalsEnabled: false },
    records: [],
    organs: [],
    transfers: [],
    gossipTransfers: [],
  };
  let selectedTransferId = null;
  let selectedGossipTransferUid = null;
  let activeTransferTab = "mine";
  let detailMode = "selected";
  let pendingDeleteTransferId = null;
  let busy = false;
  let contractLoading = false;
  let reloadQueued = false;
  let confirmingProgressAction = null;

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

  function transferDepth(transfer, seen = new Set()) {
    const uid = String(transfer?.transferUid || "");
    if (!uid || seen.has(uid)) return 0;
    seen.add(uid);
    const parentUid = transfer?.tree?.parentUid;
    if (!parentUid) return 0;
    const parent = snapshot.transfers.find((item) => String(item.transferUid) === String(parentUid));
    return parent ? 1 + transferDepth(parent, seen) : 1;
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
    return `
      <section class="transferParty" data-local="${local ? "true" : "false"}">
        <div class="partyLabel">${escapeHtml(label)}</div>
        ${processButtons(transfer, sideName)}
        ${body}
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
    transferList.innerHTML = transfers.map((transfer) => {
      const active = Number(transfer.id) === Number(selectedTransferId);
      const depth = Math.min(transferDepth(transfer), 8);
      const childCount = Number(transfer.tree?.childIds?.length || 0);
      return `
        <button type="button" class="transferRow" data-transfer-id="${escapeHtml(transfer.id)}" data-active="${active ? "true" : "false"}" data-keep-enabled="true" style="padding-left: ${12 + depth * 16}px">
          <span class="transferTitle">${escapeHtml(transfer.title || "Transfer")}</span>
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

      <div class="actionGrid">
        <div class="actionBox">
          <div class="actionTitle">Transfer tree</div>
          <div class="meta">parent ${escapeHtml(transfer.tree?.parentId ? "#" + transfer.tree.parentId : "none")} / children ${escapeHtml(String(transfer.tree?.childIds?.length || 0))} / effective ${escapeHtml(transfer.tree?.effectiveBranchMode || "duplicated")}</div>
          <div class="inlineControls">
            <label>
              <span>Branch mode</span>
              <select id="tree-branch-mode" data-keep-enabled="true">
                <option value="inherit" ${branchMode === "inherit" ? "selected" : ""}>Inherit</option>
                <option value="duplicated" ${branchMode === "duplicated" ? "selected" : ""}>Duplicated</option>
                <option value="greedy" ${branchMode === "greedy" ? "selected" : ""}>Greedy</option>
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
    renderRecords();
    renderOrgans();
    renderTransferList();
    renderDetail();
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

    const cancelCreate = event.target.closest("[data-action='cancel-create']");
    if (cancelCreate) {
      detailMode = "selected";
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
      renderTransferList();
      renderDetail();
      return;
    }

    const fillRecord = event.target.closest("[data-fill-record]");
    if (fillRecord) {
      detailMode = "create";
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
      renderTransferList();
      renderDetail();
      return;
    }

    const row = event.target.closest("[data-transfer-id]");
    if (row) {
      selectedTransferId = Number(row.dataset.transferId);
      pendingDeleteTransferId = null;
      confirmingProgressAction = null;
      detailMode = "selected";
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
      postAction(action, { transferId: transfer.id });
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

  connectTransferStream();
  loadContract();
})();
"##.to_string()
}
