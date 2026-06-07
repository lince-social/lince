pub(super) fn script() -> String {
    r##"
(() => {
  const app = document.getElementById("transfer-app");
  const status = document.getElementById("transfer-status");
  const identitySummary = document.getElementById("identity-summary");
  const identityForm = document.getElementById("identity-form");
  const identityLabel = document.getElementById("identity-label");
  const identitySave = document.getElementById("identity-save");
  const ingressSummary = document.getElementById("ingress-summary");
  const ingressForm = document.getElementById("ingress-form");
  const publicProposalsEnabled = document.getElementById("public-proposals-enabled");
  const organLoginForm = document.getElementById("organ-login-form");
  const loginOrgan = document.getElementById("login-organ");
  const recordForm = document.getElementById("record-form");
  const recordList = document.getElementById("record-list");
  const proposalForm = document.getElementById("proposal-form");
  const proposalSubmit = document.getElementById("proposal-create");
  const proposalRecord = document.getElementById("proposal-record");
  const proposalOrgan = document.getElementById("proposal-organ");
  const proposalCounterparty = document.getElementById("proposal-counterparty");
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
  let pendingDeleteTransferId = null;
  let busy = false;

  function contractUrl() {
    return "/host/widgets/" + encodeURIComponent(instanceId) + "/contract";
  }

  function actionUrl(action) {
    return "/host/widgets/" + encodeURIComponent(instanceId) + "/actions/" + encodeURIComponent(action);
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
    if (activeTransferTab === "mine") {
      return snapshot.transfers.filter((transfer) => transfer.proposerLabel === localLabel);
    }
    return snapshot.transfers.filter((transfer) =>
      transfer.proposerLabel !== localLabel && (transfer.localRole || transfer.controls?.canDuplicate)
    );
  }

  function selectedTransfer() {
    const transfers = visibleTransfers();
    if (!selectedTransferId && transfers.length) {
      selectedTransferId = transfers[0].id;
    }
    return transferById(selectedTransferId);
  }

  function reconcileSelectedTransfer() {
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
          No local signing identity.
          <div class="meta">Save one once on this node. It is not a JWT and it is not sent to other nodes as a secret.</div>
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
    const options = [`<option value="">Local only</option>`].concat(
      snapshot.organs.map((organ) => {
        const auth = organ.authenticated ? "" : " (login for replies)";
        return `<option value="${escapeHtml(organ.id)}">${escapeHtml(organ.name + auth)}</option>`;
      })
    );
    proposalOrgan.innerHTML = options.join("");
    loginOrgan.innerHTML = [`<option value="">Select Organ</option>`].concat(
      snapshot.organs.map((organ) => {
        const state = organ.authenticated ? "connected" : "login needed";
        return `<option value="${escapeHtml(organ.id)}">${escapeHtml(organ.name + " / " + state)}</option>`;
      })
    ).join("");
  }

  function sideHtml(label, side) {
    return `
      <div class="sideBox">
        <div class="sideTitle">
          <span>${escapeHtml(label)}</span>
          <span class="quantity">${escapeHtml(formatQuantity(side?.quantity || 0))}</span>
        </div>
        <div class="meta">${escapeHtml(side?.actorLabel || "unbound")} / ${escapeHtml(side?.head || "Record")}</div>
        <div class="meta">record #${escapeHtml(side?.recordId || 0)} / key ${escapeHtml(shortKey(side?.publicKey))}</div>
      </div>
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
      return `
        <button type="button" class="transferRow" data-transfer-id="${escapeHtml(transfer.id)}" data-active="${active ? "true" : "false"}" data-keep-enabled="true">
          <span class="transferTitle">${escapeHtml(transfer.title || "Transfer")}</span>
          <span class="meta">#${escapeHtml(transfer.id)} ${escapeHtml(shortKey(transfer.transferUid))}</span>
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
    transferCount.textContent = snapshot.gossipTransfers.length + " observed";
    if (!snapshot.gossipTransfers.length) {
      transferList.innerHTML = `<div class="emptyBlock">No observed gossip packages.</div>`;
      return;
    }
    transferList.innerHTML = snapshot.gossipTransfers.map((transfer) => {
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

  function agreementActionLabel(transfer) {
    const agreement = transfer?.agreement || {};
    const localRole = transfer?.localRole || "";
    const count = localRole === "contribution"
      ? Number(agreement.contribution || 0)
      : localRole === "need"
        ? Number(agreement.need || 0)
        : 0;
    return count > 0 ? "Confirm Agreement" : "First Agreement";
  }

  function renderDetail() {
    if (activeTransferTab === "observed") {
      renderGossipDetail();
      return;
    }
    const transfer = selectedTransfer();
    if (!transfer) {
      transferDetail.innerHTML = `<div class="emptyBlock">Select or create a Transfer.</div>`;
      return;
    }
    const controls = transfer.controls || {};
    const packageText = JSON.stringify(transfer.package || {}, null, 2);
    transferDetail.innerHTML = `
      <div class="detailHeader">
        <div>
          <h2>${escapeHtml(transfer.title || "Transfer")}</h2>
          <div class="meta">#${escapeHtml(transfer.id)} / ${escapeHtml(transfer.transferUid || "")}</div>
        </div>
        <div class="chips">${transferChips(transfer)}</div>
      </div>

      <div class="sideGrid">
        ${sideHtml("Contribution", transfer.contribution || {})}
        ${sideHtml("Need", transfer.need || {})}
      </div>

      <div class="actionGrid">
        ${controls.canDuplicate ? `
          <div class="actionBox">
            <div class="actionTitle">Duplicate locally</div>
            <div class="inlineControls">
              <select id="duplicate-role">
                <option value="contribution">Contribution</option>
                <option value="need">Need</option>
              </select>
              ${recordSelectHtml("duplicate-record")}
              ${actionButton("duplicate-proposal", "Duplicate", false)}
            </div>
          </div>
        ` : ""}
        <div class="actionBox">
          <div class="actionTitle">Local signatures</div>
          <div class="inlineControls">
            ${actionButton("sign-agreement", agreementActionLabel(transfer), !controls.canSignAgreement)}
            ${actionButton("confirm-delivery", "Confirm Delivery", !controls.canConfirmDelivery)}
            ${actionButton("confirm-receipt", "Confirm Receipt", !controls.canConfirmReceipt)}
            ${actionButton("settle-local", "Settle Local Record", !controls.canSettleLocal, 'class="primary"')}
          </div>
        </div>
        <div class="actionBox">
          <div class="actionTitle">Danger zone</div>
          <div class="inlineControls">
            ${actionButton(
              "delete-transfer",
              Number(pendingDeleteTransferId) === Number(transfer.id) ? "Confirm delete" : "Delete transfer",
              false,
              'class="danger"'
            )}
          </div>
        </div>
        <div class="actionBox">
          <div class="actionTitle">Post package</div>
          <div class="inlineControls">
            ${postTargetSelectHtml("post-organ", transfer)}
            ${actionButton("post-transfer", "Post to Organ", false)}
          </div>
        </div>
      </div>

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
      proposalSubmit.disabled = busy || !snapshot.localIdentity;
      proposalSubmit.title = snapshot.localIdentity ? "" : "Save local identity first";
    }
  }

  async function loadContract() {
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
      setBusy(false);
    }
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
    if (!snapshot.localIdentity) {
      setStatus("Save local identity first.", "danger");
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

  proposalSubmit.addEventListener("click", createProposal);

  proposalForm.addEventListener("submit", (event) => {
    event.preventDefault();
    createProposal();
  });

  app.addEventListener("click", async (event) => {
    const refreshButton = event.target.closest("[data-action='refresh']");
    if (refreshButton) {
      postAction("refresh");
      return;
    }

    const tabButton = event.target.closest("[data-transfer-tab]");
    if (tabButton) {
      activeTransferTab = tabButton.dataset.transferTab || "mine";
      selectedTransferId = null;
      selectedGossipTransferUid = null;
      pendingDeleteTransferId = null;
      renderTransferList();
      renderDetail();
      return;
    }

    const fillRecord = event.target.closest("[data-fill-record]");
    if (fillRecord) {
      proposalRecord.value = fillRecord.dataset.fillRecord || "";
      return;
    }

    const gossipRow = event.target.closest("[data-gossip-transfer-uid]");
    if (gossipRow) {
      selectedGossipTransferUid = gossipRow.dataset.gossipTransferUid || "";
      renderTransferList();
      renderDetail();
      return;
    }

    const row = event.target.closest("[data-transfer-id]");
    if (row) {
      selectedTransferId = Number(row.dataset.transferId);
      pendingDeleteTransferId = null;
      renderTransferList();
      renderDetail();
      return;
    }

    const transfer = selectedTransfer();
    if (!transfer) return;

    const transferAction = event.target.closest("[data-transfer-action]");
    if (transferAction) {
      const action = transferAction.dataset.transferAction;
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

  proposalOrgan.addEventListener("change", () => {
    const organId = selectedOrganId(proposalOrgan.value);
    const organ = snapshot.organs.find((item) => Number(item.id) === Number(organId));
    if (organ && !proposalCounterparty.value.trim()) {
      proposalCounterparty.value = organ.name;
    }
  });

  loadContract();
})();
"##.to_string()
}
