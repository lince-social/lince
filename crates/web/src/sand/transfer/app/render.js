import {
  agreementLabel,
  balanceEntries,
  formatDate,
  formatQuantity,
  needsAttention,
  partyName,
  progressLabel,
  promiseName,
  statusLabel,
} from "./model.js";

export function renderSummary(root, summary) {
  root.replaceChildren(
    metric("Total", summary.total),
    metric("Attention", summary.attention, summary.attention > 0 ? "danger" : ""),
    metric("Open", summary.open),
    metric("Settled", summary.settled, "success"),
  );
}

export function renderList(root, rows, selectedUid, onSelect) {
  const fragment = document.createDocumentFragment();
  for (const row of rows) fragment.append(transferCard(row, row.uid === selectedUid, onSelect));
  root.replaceChildren(fragment);
}

export function renderDetail(root, row, { onBack, onOpenRecord }) {
  if (!row) {
    root.hidden = true;
    root.replaceChildren();
    return;
  }

  // Promises point at the Person record; agreement rows have their own uid
  // and expose that Person as `actor`.
  const partyByUid = new Map(row.parties.map((party) => [party.actor, party]));
  const header = el("header", "detailHeader");
  const back = button("Back", "backButton", onBack);
  back.setAttribute("aria-label", "Back to transfer overview");
  const heading = el("div", "detailHeading");
  heading.append(
    el("div", "eyebrow", row.slug || "Transfer"),
    el("h2", "", row.head),
    status(row.status),
  );
  header.append(back, heading);

  const facts = el("dl", "factGrid");
  facts.append(
    fact("Agreement", agreementLabel(row)),
    fact("Visibility", visibility(row)),
    fact("Settlement", statusLabel(row.settlement || "manual")),
    fact("Confirmation", row.require_confirmation ? "Delivery and receipt" : "Not required"),
    fact("Promise progress", progressLabel(row.progress)),
    fact("Default reserve", statusLabel(row.reserve_default || "none")),
  );

  const readiness = detailSection("Agreement", "agreement-section");
  readiness.append(agreementMeter(row));
  const agreementPeople = el("div", "agreementPeople");
  for (const party of row.parties) {
    const item = el("div", "agreementPerson");
    item.append(
      el("span", "personName", partyName(party)),
      el("span", "levelLabel", agreementLevel(party.level)),
    );
    agreementPeople.append(item);
  }
  if (!row.parties.length) agreementPeople.append(emptyInline("No parties attached"));
  readiness.append(agreementPeople);

  const promises = detailSection(`Promises (${row.promises.length})`, "promises-section");
  const promiseList = el("div", "promiseList");
  for (const promise of row.promises) {
    promiseList.append(promiseRow(promise, partyByUid.get(promise.party), onOpenRecord));
  }
  if (!row.promises.length) promiseList.append(emptyInline("No promises attached"));
  promises.append(promiseList);

  const accounting = detailSection("Balance", "balance-section");
  const balanceList = el("div", "balanceList");
  const entries = balanceEntries(row);
  for (const [concept, value] of entries) {
    const item = el("div", "balanceRow");
    item.append(el("span", "", concept), el("strong", value === 0 ? "zero" : value > 0 ? "positive" : "negative", formatQuantity(value)));
    balanceList.append(item);
  }
  if (!entries.length) balanceList.append(emptyInline("No classified quantities"));
  accounting.append(balanceList);

  const confirmations = detailSection(`Evidence (${row.confirmations.length})`, "evidence-section");
  const evidenceList = el("div", "evidenceList");
  for (const confirmation of row.confirmations) {
    const item = el("div", "evidenceRow");
    item.append(
      el("strong", "", statusLabel(confirmation.kind)),
      el("span", "", confirmation.actor ? `Actor ${confirmation.actor}` : "Local actor"),
      el("time", "", formatDate(confirmation.at)),
    );
    evidenceList.append(item);
  }
  if (!row.confirmations.length) evidenceList.append(emptyInline("No confirmations recorded"));
  confirmations.append(evidenceList);

  const lineage = detailSection("Transfer tree", "lineage-section");
  const lineageGrid = el("div", "lineageGrid");
  lineageGrid.append(
    linkedReference("Parent", row.parent, row.parent_head || row.parent_slug, onOpenRecord),
    linkedReference("Source", row.source, row.source_head || row.source_slug, onOpenRecord),
    fact("Transfer ID", row.uid),
  );
  lineage.append(lineageGrid);

  root.hidden = false;
  root.replaceChildren(header, facts, readiness, promises, accounting, confirmations, lineage);
}

function transferCard(row, selected, onSelect) {
  const card = button("", "transferCard", () => onSelect(row.uid));
  card.dataset.selected = String(selected);
  card.dataset.status = row.status;
  card.setAttribute("aria-label", `Open ${row.head}`);

  const top = el("div", "cardTop");
  const identity = el("div", "cardIdentity");
  identity.append(el("strong", "cardTitle", row.head));
  if (row.slug) identity.append(el("span", "cardSlug", row.slug));
  top.append(identity, status(row.status));

  const agreement = agreementMeter(row, true);
  const people = row.parties.map(partyName).join(", ") || "No parties";
  const promises = row.promises.length === 1 ? "1 promise" : `${row.promises.length} promises`;
  const meta = el("div", "cardMeta");
  meta.append(el("span", "", people), el("span", "", promises));

  const promisePreview = el("div", "promisePreview");
  for (const promise of row.promises.slice(0, 3)) {
    const item = el("span", "promiseChip");
    item.append(
      el("b", Number(promise.delta) < 0 ? "out" : "in", formatQuantity(promise.delta, promise.unit_name)),
      document.createTextNode(` ${promiseName(promise)}`),
    );
    promisePreview.append(item);
  }
  if (row.promises.length > 3) promisePreview.append(el("span", "more", `+${row.promises.length - 3}`));

  if (needsAttention(row)) card.dataset.attention = "true";
  card.append(top, agreement, meta, promisePreview);
  return card;
}

function promiseRow(promise, party, onOpenRecord) {
  const row = el("article", "promiseRow");
  const main = el("div", "promiseMain");
  main.append(
    el("strong", "promiseTitle", promiseName(promise)),
    el("span", "promiseParty", party ? partyName(party) : "Unassigned party"),
  );
  const amount = el("div", "promiseAmount");
  amount.append(
    el("strong", Number(promise.delta) < 0 ? "out" : "in", formatQuantity(promise.delta, promise.unit_name)),
    status(promise.state),
  );
  const schedule = el("div", "promiseSchedule");
  schedule.append(
    el("span", "", formatDate(promise.window_end)),
    el("span", "", `Reserve from ${statusLabel(promise.reserve_from || "active")}`),
  );
  row.append(main, amount, schedule);
  if (promise.record) {
    const open = button("Open record", "textButton", () => onOpenRecord(promise.record));
    row.append(open);
  }
  return row;
}

function agreementMeter(row, compact = false) {
  const block = el("div", compact ? "agreementMeter compact" : "agreementMeter");
  const label = el("div", "meterLabel");
  label.append(el("span", "", agreementLabel(row)), el("span", "", row.agreement.policy_satisfied ? "Ready" : "Waiting"));
  const track = el("div", "meterTrack");
  const fill = el("span", "meterFill");
  const denominator = row.agreement.required || row.agreement.total || 1;
  fill.style.width = `${Math.min(100, (row.agreement.committed / denominator) * 100)}%`;
  track.append(fill);
  block.append(label, track);
  return block;
}

function status(value) {
  const pill = el("span", "status", statusLabel(value));
  pill.dataset.status = value || "draft";
  return pill;
}

function metric(label, value, tone = "") {
  const item = el("div", "metric");
  if (tone) item.dataset.tone = tone;
  item.append(el("span", "", label), el("strong", "", String(value)));
  return item;
}

function detailSection(title, id) {
  const section = el("section", "detailSection");
  section.id = id;
  section.append(el("h3", "", title));
  return section;
}

function fact(label, value) {
  const item = el("div", "fact");
  item.append(el("dt", "", label), el("dd", "", value || "None"));
  return item;
}

function linkedReference(label, uid, name, onOpenRecord) {
  if (!uid) return fact(label, "None");
  const item = el("div", "fact");
  const reference = button(name || uid, "referenceButton", () => onOpenRecord(uid));
  reference.title = uid;
  item.append(el("dt", "", label), reference);
  return item;
}

function visibility(row) {
  if (row.visibility === "proximity" && row.max_proximity != null) {
    return `Proximity ${row.max_proximity}`;
  }
  return statusLabel(row.visibility);
}

function agreementLevel(level) {
  if (Number(level) >= 2) return "Committed";
  if (Number(level) >= 1) return "Reviewed";
  return "No agreement";
}

function emptyInline(text) {
  return el("div", "emptyInline", text);
}

function button(text, className, onClick) {
  const node = el("button", className, text);
  node.type = "button";
  node.addEventListener("click", onClick);
  return node;
}

function el(tag, className = "", text = null) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text != null) node.textContent = String(text);
  return node;
}
