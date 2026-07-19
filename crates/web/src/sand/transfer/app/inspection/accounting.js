import { button, el, empty, formatQuantity, labeledValue, present, projectedArray, status, statusLabel } from "./shared.js";

export function renderAccountingAndNavigation(row, rows, options) {
  const section = el("section", "inspectionBand accountingBand");
  section.append(el("header", "inspectionBandHeading", "Records, quantities, and balance"));
  const navigation = renderNavigation(row, rows, options);
  if (navigation) section.append(navigation);

  const projected = projectedRecordRows(row);
  const recordList = el("div", "inspectionRecordList");
  if (projected.length) {
    for (const record of projected) recordList.append(projectedRecord(record, options.onOpenRecord));
  } else {
    for (const record of fallbackRecordRows(row)) recordList.append(fallbackRecord(record, options.onOpenRecord));
  }
  if (!recordList.childElementCount) recordList.append(empty("No visible shared Records or private Record quantities"));
  section.append(recordList, renderConceptBalance(row));
  return section;
}

function renderNavigation(row, rows, options) {
  const nav = el("nav", "inspectionNavigation");
  nav.setAttribute("aria-label", "Transfer hierarchy and shared Records");
  const byUid = new Map(rows.map((candidate) => [candidate.uid, candidate]));
  const path = (row.hierarchy?.path || []).map((uid) => byUid.get(uid)).filter(Boolean);
  if (path.length) {
    const hierarchy = el("div", "inspectionNavGroup");
    hierarchy.append(el("strong", "", "Hierarchy"));
    const links = el("div", "inspectionNavLinks");
    for (const transfer of path) {
      links.append(button(transfer.head || transfer.uid, "referenceButton", () => options.onSelectBranch?.(transfer.uid)));
    }
    hierarchy.append(links);
    nav.append(hierarchy);
  }
  const shared = sharedRecords(row);
  if (shared.length) {
    const records = el("div", "inspectionNavGroup");
    records.append(el("strong", "", "Shared Records"));
    const links = el("div", "inspectionNavLinks");
    for (const record of shared) {
      const control = button(record.head || record.slug || record.uid, "referenceButton", () => options.onOpenRecord?.(record.uid));
      control.title = record.uid;
      links.append(control);
    }
    records.append(links);
    nav.append(records);
  }
  return nav.childElementCount ? nav : null;
}

function projectedRecordRows(row) {
  const inspection = row.inspection && typeof row.inspection === "object" ? row.inspection : {};
  return projectedArray(
    row.record_quantities || row.record_projection || inspection.records || row.accounting?.records,
    "records",
    "items",
  );
}

function projectedRecord(raw, onOpenRecord) {
  const record = raw && typeof raw === "object" ? raw : {};
  const uid = record.uid || record.record || record.record_uid;
  const article = el("article", "inspectionRecordRow");
  const heading = el("header", "inspectionRecordHeading");
  heading.append(
    el("div", "", record.head || record.record_head || record.slug || uid || "Record"),
    record.state ? status(record.state) : el("span", "inspectionRecordKind", statusLabel(record.kind || "record")),
  );
  if (uid) heading.append(button("Open", "textButton", () => onOpenRecord?.(uid)));
  article.append(heading);
  const facts = el("dl", "inspectionRecordFacts");
  const values = [
    ["Concept", record.concept_name || record.concept],
    ["Unit", record.unit_name || record.unit],
    ["Quantity", quantity(record.quantity, record.unit_name || record.unit)],
    ["Promised", quantity(record.promised_quantity ?? record.planned_quantity, record.unit_name || record.unit)],
    ["Settled", quantity(record.settled_quantity, record.unit_name || record.unit)],
    ["Remaining", quantity(record.remaining_quantity, record.unit_name || record.unit)],
  ].filter(([, value]) => value != null);
  for (const [label, value] of values) facts.append(labeledValue(label, value));
  if (facts.childElementCount) article.append(facts);
  const formula = record.formula || record.application_formula;
  if (formula != null) article.append(formulaRow(formula, record.formula_source || record.application_source));
  const entries = projectedArray(record.entries, "promises", "occurrences", "quantities");
  if (entries.length) {
    const list = el("ul", "inspectionQuantityList");
    for (const entry of entries) list.append(el("li", "", quantityEntry(entry)));
    article.append(list);
  }
  return article;
}

function fallbackRecordRows(row) {
  const records = new Map();
  const ensure = (uid, head, slug) => {
    if (!uid) return null;
    if (!records.has(uid)) records.set(uid, { uid, head, slug, promises: [], occurrences: [] });
    return records.get(uid);
  };
  for (const promise of row.promises || []) {
    ensure(promise.record, promise.record_head, promise.record_slug)?.promises.push(promise);
  }
  for (const occurrence of row.occurrences || []) {
    ensure(occurrence.subject, occurrence.subject_head, occurrence.subject_slug)?.occurrences.push(occurrence);
  }
  return [...records.values()];
}

function fallbackRecord(record, onOpenRecord) {
  const article = el("article", "inspectionRecordRow");
  const heading = el("header", "inspectionRecordHeading");
  heading.append(
    el("div", "", record.head || record.slug || record.uid),
    el("span", "inspectionRecordKind", "Visible entries"),
    button("Open", "textButton", () => onOpenRecord?.(record.uid)),
  );
  article.append(heading);
  const list = el("ul", "inspectionQuantityList");
  for (const promise of record.promises) {
    list.append(el("li", "", `Promise ${formatQuantity(promise.delta, promise.unit_name)} · ${statusLabel(promise.state)}`));
  }
  for (const occurrence of record.occurrences) {
    const progress = occurrence.settlement_progress;
    const label = [
      `Occurrence ${formatQuantity(occurrence.quantity, occurrence.unit_name)}`,
      progress?.settled_quantity != null && `settled ${formatQuantity(progress.settled_quantity, occurrence.unit_name)}`,
      progress?.remaining_quantity != null && `remaining ${formatQuantity(progress.remaining_quantity, occurrence.unit_name)}`,
    ].filter(Boolean).join(" · ");
    list.append(el("li", "", label));
    if (occurrence.application?.formula != null) {
      const item = el("li", "inspectionFormulaEntry");
      item.append(formulaRow(occurrence.application.formula, occurrence.application.source));
      list.append(item);
    }
  }
  article.append(list);
  return article;
}

function renderConceptBalance(row) {
  const section = el("section", "conceptBalance");
  const heading = el("header", "conceptBalanceHeading");
  heading.append(el("strong", "", "Per-concept balance"), status(row.balanced ? "balanced" : "unbalanced"));
  section.append(heading);
  const entries = Array.isArray(row.balance_detail) ? row.balance_detail : [];
  if (!entries.length) {
    section.append(empty("No server-projected concept balance"));
    return section;
  }
  const list = el("dl", "conceptBalanceList");
  for (const entry of entries) {
    const item = el("div", "conceptBalanceRow");
    item.append(
      el("dt", "", entry.concept_name || entry.concept || "Unclassified"),
      el("dd", Number(entry.delta) === 0 ? "zero" : Number(entry.delta) > 0 ? "positive" : "negative", formatQuantity(entry.delta, entry.unit_name)),
    );
    list.append(item);
  }
  section.append(list);
  return section;
}

function sharedRecords(row) {
  const records = new Map();
  const add = (uid, head, slug) => {
    if (uid && !records.has(uid)) records.set(uid, { uid, head, slug });
  };
  for (const promise of row.promises || []) add(promise.record, promise.record_head, promise.record_slug);
  for (const occurrence of row.occurrences || []) add(occurrence.subject, occurrence.subject_head, occurrence.subject_slug);
  return [...records.values()];
}

function formulaRow(formula, source) {
  const block = el("div", "inspectionFormula");
  block.append(el("span", "", source ? `Formula · ${statusLabel(source)}` : "Formula"), el("code", "", present(formula)));
  return block;
}

function quantity(value, unit) {
  if (value == null || !Number.isFinite(Number(value))) return null;
  return formatQuantity(Number(value), unit);
}

function quantityEntry(entry) {
  if (typeof entry !== "object" || !entry) return present(entry);
  return [
    entry.label || entry.kind || entry.state,
    quantity(entry.quantity ?? entry.delta, entry.unit_name || entry.unit),
    entry.remaining_quantity != null && `remaining ${quantity(entry.remaining_quantity, entry.unit_name || entry.unit)}`,
  ].filter(Boolean).join(" · ");
}
