import { partyName } from "../model.js";
import { selectedActingPerson, setSelectedActingPerson } from "../occurrence.js";
import { button, compactId, el, empty, formatDate, formatQuantity, statusLabel } from "./shared.js";

const OCCURRENCE_ACTIONS = ["create_remainder_draft", "create_reversing_transfer"];

export function renderCorrections(row, rows, options) {
  const occurrences = row.occurrences.filter(hasOccurrenceCorrection);
  const promises = row.promises.filter(hasPromiseCorrection);
  const lineage = Array.isArray(row.correction_lineage) ? row.correction_lineage : [];
  if (!occurrences.length && !promises.length && !lineage.length) return null;

  const section = el("section", "inspectionBand correctionBand");
  const heading = el("div", "inspectionBandHeading correctionHeading");
  heading.append(
    el("div", "eyebrow", "Append-only controls"),
    el("h4", "", "Corrections and successors"),
  );
  section.append(heading);

  let person = selectedActingPerson(row, options);
  if (options.viewer?.local && hasAvailableCorrection(occurrences, promises)) {
    const field = el("label", "correctionPersonField");
    field.append(el("span", "", "Acting Person"));
    const picker = el("select", "occurrencePersonSelect");
    picker.append(new Option("Select acting Person", ""));
    for (const party of row.parties) picker.append(new Option(partyName(party), party.actor));
    picker.value = person;
    picker.addEventListener("change", () => {
      setSelectedActingPerson(row, picker.value);
      section.replaceWith(renderCorrections(row, rows, options));
    });
    field.append(picker);
    section.append(field);
    person = picker.value;
  }

  if (occurrences.length) {
    const list = el("div", "correctionOccurrenceList");
    for (const occurrence of occurrences) list.append(occurrenceCorrection(row, occurrence, person, options));
    section.append(list);
  }

  if (promises.length) {
    const list = el("div", "correctionPromiseList");
    for (const promise of promises) list.append(promiseCorrection(row, promise, person, options));
    section.append(list);
  }

  if (lineage.length) section.append(correctionLineage(lineage, rows, options));
  return section;
}

function occurrenceCorrection(transfer, occurrence, person, options) {
  const block = el("article", "correctionOccurrence");
  const heading = el("header", "correctionItemHeading");
  heading.append(
    el("strong", "", occurrence.subject_head || occurrence.subject_slug || occurrence.subject || occurrence.uid),
    el("span", "", compactId(occurrence.uid)),
  );
  block.append(heading);

  const progress = occurrence.settlement_progress || {};
  const facts = el("dl", "correctionQuantityFacts");
  facts.append(
    correctionFact("Canonical total", quantity(progress.canonical_quantity, occurrence)),
    correctionFact("Settled", quantity(progress.settled_quantity, occurrence)),
    correctionFact("Exact remainder", quantity(progress.remaining_quantity, occurrence)),
  );
  block.append(facts);

  const actions = el("div", "correctionActions");
  actions.append(
    remainderAction(transfer, occurrence, person, options),
    reversalAction(transfer, occurrence, person, options),
  );
  block.append(actions);
  return block;
}

function remainderAction(transfer, occurrence, person, options) {
  const canCreate = occurrence.capabilities?.create_remainder_draft === true;
  const blockers = blockerValues(occurrence.blocking_reasons?.create_remainder_draft);
  const remaining = occurrence.settlement_progress?.remaining_quantity;
  const key = `correction:${transfer.uid}:${occurrence.uid}:remainder:${person}:${remaining}`;
  const block = el("section", "correctionAction remainderAction");
  block.append(
    el("strong", "", "Unsent remainder draft"),
    el("p", "", `Copies exactly ${quantity(remaining, occurrence)} into a hidden creator-only draft.`),
  );

  if (canCreate) {
    const control = actionButton("Create remainder draft", key, options);
    control.disabled ||= localPersonMissing(options, person) || !positiveNumber(remaining);
    control.addEventListener("click", () => options.onAction?.(key, withLocalPerson({
      action: "create-transfer-remainder-draft",
      occurrence: occurrence.uid,
      expected_revision: transfer.revision,
      expected_remaining_quantity: remaining,
      request_id: requestId("remainder"),
    }, options, person)));
    block.append(control);
    appendActionState(block, key, options, rowsFromOptions(options));
  }
  appendBlockers(block, blockers);
  return block;
}

function reversalAction(transfer, occurrence, person, options) {
  const canCreate = occurrence.capabilities?.create_reversing_transfer === true;
  const blockers = blockerValues(occurrence.blocking_reasons?.create_reversing_transfer);
  const canonical = occurrence.quantity;
  const key = `correction:${transfer.uid}:${occurrence.uid}:reverse:${person}:${canonical}`;
  const block = el("section", "correctionAction reversalAction");
  block.append(
    el("strong", "", "Linked reversing transfer"),
    el("p", "", `Proposes a new hidden draft reversing the full signed quantity ${quantity(canonical, occurrence)}. Original evidence remains unchanged.`),
  );

  if (canCreate) {
    const acknowledgement = reviewedCheck(
      "I reviewed the full signed quantity and understand this creates a separate proposal.",
      options,
    );
    const control = actionButton("Create reversing draft", key, options);
    control.disabled = true;
    const sync = () => {
      const state = options.actionState?.(key);
      control.disabled = options.mutationsEnabled === false || Boolean(state?.busy || state?.waiting)
        || !acknowledgement.input.checked || localPersonMissing(options, person) || !positiveNumber(canonical);
    };
    acknowledgement.input.addEventListener("change", sync);
    sync();
    control.addEventListener("click", () => options.onAction?.(key, withLocalPerson({
      action: "create-reversing-transfer-draft",
      occurrence: occurrence.uid,
      expected_revision: transfer.revision,
      canonical_quantity: canonical,
      request_id: requestId("reversal"),
    }, options, person)));
    block.append(acknowledgement.label, control);
    appendActionState(block, key, options, rowsFromOptions(options));
  }
  appendBlockers(block, blockers);
  return block;
}

function promiseCorrection(transfer, promise, person, options) {
  const block = el("article", "correctionPromise");
  const heading = el("header", "correctionItemHeading");
  heading.append(
    el("strong", "", promise.record_head || promise.record_slug || promise.record || promise.uid),
    el("span", "", `Promise ${compactId(promise.uid)}`),
  );
  block.append(heading);

  if (promise.capabilities?.reopen === true) block.append(reopenForm(transfer, promise, person, options));
  appendBlockers(block, blockerValues(promise.blocking_reasons?.reopen));
  const lineage = promiseLineage(promise);
  if (lineage) block.append(lineage);
  return block;
}

function reopenForm(transfer, promise, person, options) {
  const key = `correction:${transfer.uid}:${promise.uid}:reopen:${person}`;
  const form = el("form", "reopenPromiseForm");
  const explanation = el("p", "correctionExplanation", "Creates a proposed successor in a new signed revision. The terminal promise and its evidence remain unchanged.");

  const endField = el("label", "correctionField");
  const end = el("input");
  end.type = "datetime-local";
  end.disabled = options.mutationsEnabled === false;
  endField.append(el("span", "", "Successor deadline"), end);

  const openField = el("label", "correctionCheckField");
  const open = el("input");
  open.type = "checkbox";
  open.disabled = options.mutationsEnabled === false;
  openField.append(open, el("span", "", "Publish successor as OPEN"));

  const acknowledgement = reviewedCheck(
    "I reviewed the successor terms and understand every agreement level resets for the new revision.",
    options,
  );
  const submit = actionButton("Sign successor revision", key, options);
  submit.type = "submit";
  submit.disabled = true;
  const sync = () => {
    const state = options.actionState?.(key);
    submit.disabled = options.mutationsEnabled === false || Boolean(state?.busy || state?.waiting)
      || !acknowledgement.input.checked || localPersonMissing(options, person);
  };
  acknowledgement.input.addEventListener("change", sync);
  sync();
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (submit.disabled) return;
    options.onAction?.(key, withLocalPerson({
      action: "reopen-transfer-promise",
      transfer: transfer.uid,
      promise: promise.uid,
      expected_revision: transfer.revision,
      request_id: requestId("reopen"),
      window_end: end.value ? new Date(end.value).toISOString() : null,
      open: open.checked,
    }, options, person));
  });
  form.append(explanation, endField, openField, acknowledgement.label, submit);
  appendActionState(form, key, options, rowsFromOptions(options));
  return form;
}

function correctionLineage(lineage, rows, options) {
  const block = el("section", "correctionLineage");
  block.append(el("h5", "", "Correction lineage"));
  const list = el("div", "correctionLineageList");
  for (const link of lineage) {
    const item = el("div", "correctionLineageRow");
    const identity = el("div", "correctionLineageIdentity");
    identity.append(
      el("strong", "", statusLabel(link.kind || "correction")),
      el("span", "", `${statusLabel(link.role || "linked")} · ${quantity(link.canonical_quantity)}`),
      el("small", "", [link.at ? formatDate(link.at) : "", link.actor ? `Actor ${compactId(link.actor)}` : ""].filter(Boolean).join(" · ")),
    );
    item.append(identity);
    const target = link.role === "created" ? link.source_transfer : link.created_transfer;
    if (target) item.append(navigationButton(target, rows, options));
    list.append(item);
  }
  if (!lineage.length) list.append(empty("No correction lineage"));
  block.append(list);
  return block;
}

function promiseLineage(promise) {
  const entries = [["Predecessor", promise.predecessor], ["Successor", promise.successor]]
    .filter(([, value]) => value && typeof value === "object");
  if (!entries.length) return null;
  const block = el("dl", "promiseLineage");
  for (const [label, value] of entries) {
    const item = el("div", "promiseLineageRow");
    item.append(
      el("dt", "", label),
      el("dd", "", `${compactId(value.promise)} · revision ${value.revision ?? "?"} · ${value.at ? formatDate(value.at) : "time unavailable"}`),
    );
    block.append(item);
  }
  return block;
}

function navigationButton(uid, rows, options) {
  const visible = rows.some((row) => row.uid === uid);
  return button(visible ? "Open transfer" : "Open record", "textButton", () => {
    if (visible) options.onSelectBranch?.(uid);
    else options.onOpenRecord?.(uid);
  });
}

function reviewedCheck(text, options) {
  const label = el("label", "correctionAcknowledgement");
  const input = el("input");
  input.type = "checkbox";
  input.disabled = options.mutationsEnabled === false;
  label.append(input, el("span", "", text));
  return { label, input };
}

function actionButton(text, key, options) {
  const state = options.actionState?.(key);
  const control = el("button", "primaryButton", state?.waiting ? "Awaiting live evidence" : state?.busy ? "Signing" : text);
  control.type = "button";
  control.disabled = options.mutationsEnabled === false || Boolean(state?.busy || state?.waiting);
  return control;
}

function appendActionState(block, key, options, rows) {
  const state = options.actionState?.(key);
  if (state?.error) {
    const alert = el("div", "inlineAlert", state.error);
    alert.setAttribute("role", "alert");
    block.append(alert);
  }
  if (state?.createdUid) block.append(navigationButton(state.createdUid, rows, options));
}

function appendBlockers(block, blockers) {
  if (!blockers.length) return;
  const reasons = el("div", "correctionBlockers");
  reasons.append(el("strong", "", "Unavailable"));
  const list = el("ul");
  for (const blocker of blockers) list.append(el("li", "", blockerLabel(blocker)));
  reasons.append(list);
  block.append(reasons);
}

function correctionFact(label, value) {
  const item = el("div", "correctionFact");
  item.append(el("dt", "", label), el("dd", "", value));
  return item;
}

function hasOccurrenceCorrection(occurrence) {
  return OCCURRENCE_ACTIONS.some((key) => occurrence.capabilities?.[key] === true
    || blockerValues(occurrence.blocking_reasons?.[key]).length);
}

function hasPromiseCorrection(promise) {
  return promise.capabilities?.reopen === true
    || blockerValues(promise.blocking_reasons?.reopen).length
    || Boolean(promise.predecessor || promise.successor);
}

function hasAvailableCorrection(occurrences, promises) {
  return occurrences.some((occurrence) => OCCURRENCE_ACTIONS.some((key) => occurrence.capabilities?.[key] === true))
    || promises.some((promise) => promise.capabilities?.reopen === true);
}

function blockerValues(raw) {
  if (Array.isArray(raw)) return raw;
  return raw == null || raw === "" ? [] : [raw];
}

function blockerLabel(blocker) {
  if (blocker && typeof blocker === "object") {
    return statusLabel(blocker.message || blocker.code || blocker.kind || "blocked");
  }
  return statusLabel(blocker || "blocked");
}

function quantity(value, occurrence = {}) {
  if (value == null || !Number.isFinite(Number(value))) return "Unavailable";
  return formatQuantity(Number(value), occurrence.unit_name || occurrence.unit);
}

function positiveNumber(value) {
  return Number.isFinite(Number(value)) && Number(value) > 0;
}

function localPersonMissing(options, person) {
  return Boolean(options.viewer?.local && !person);
}

function withLocalPerson(action, options, person) {
  return options.viewer?.local ? { ...action, person } : action;
}

function rowsFromOptions(options) {
  return options.hierarchyRows || [];
}

function requestId(kind) {
  if (globalThis.crypto?.randomUUID) return `transfer-${kind}:${globalThis.crypto.randomUUID()}`;
  return `transfer-${kind}:${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}
