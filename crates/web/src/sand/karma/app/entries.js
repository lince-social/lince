// Individual gains and costs: capturing one, correcting one, undoing one, and
// changing what one was for.
//
// The four operations are deliberately distinct, because they are not the same
// event in the Ledger:
//
//   capture  — appends a signed Fact
//   revise   — compensates the old Fact and appends a replacement
//   void     — compensates, and the change is retracted from the period that
//              claimed it (a refund is a *new capture*, not a void)
//   re-tag   — appends no Fact at all; the quantity did not move, only our
//              account of what it meant
//
// The UI says which is happening, because a person deleting a March expense in
// July needs to know whether March changes.

import {
  amountText,
  conceptLabel,
  dateInputValue,
  dateLabel,
  el,
  instantFromDateInput,
  replaceChildren,
  requestId,
  setHidden,
  signOf,
} from "./format.js";
import { act, conceptToken, state, notifyChanged, setNotice } from "./state.js";

export function wireCapture(elements) {
  elements.captureForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    const target = elements.captureRecord.value;
    const amount = elements.captureAmount.value.trim();
    if (!target || !amount) {
      setNotice("Pick a resource and type an amount.");
      return;
    }
    const at = instantFromDateInput(elements.captureAt.value);
    const result = await act({
      action: "capture-entry",
      target,
      amount,
      concept: conceptToken(elements.captureConcept.value),
      note: elements.captureNote.value.trim() || null,
      at,
      // Supplied always: a double-submit must not move a quantity twice.
      request_id: requestId("capture"),
    });
    if (result) {
      elements.captureAmount.value = "";
      elements.captureNote.value = "";
      elements.captureAmount.focus();
    }
  });
}

export function wireEntryFilter(elements, onFilterChange) {
  elements.entriesFilter.addEventListener("change", () => {
    state.filters.entriesConcept = conceptToken(elements.entriesFilter.value) || "";
    onFilterChange();
  });
}

export function renderEntries(elements) {
  const entries = state.entries;
  setHidden(elements.entriesEmpty, entries.length > 0);
  replaceChildren(
    elements.entryList,
    entries.map((entry) => renderEntry(entry)),
  );
}

function recordName(uid) {
  const record = state.records.find((r) => r.uid === uid);
  return record?.head || record?.slug || uid;
}

function renderEntry(entry) {
  const editing = state.editing === entry.uid;
  const sign = signOf(entry.amount);
  const children = [
    el("div", { class: "rowTop" }, [
      el("span", { class: "amount", "data-sign": sign, text: amountText(entry.amount) }),
      el("span", { class: "rowMetaDate", text: dateLabel(entry.occurred_at) }),
    ]),
    el("div", { class: "rowMeta" }, [
      el("span", { text: recordName(entry.record) }),
      el("span", {
        class: "tag",
        "data-unclassified": entry.concept ? "false" : "true",
        text: entry.concept
          ? `@${conceptLabel(entry.concept, state.conceptNames)}`
          : "unclassified",
      }),
      entry.note ? el("span", { text: entry.note }) : null,
      entry.void ? el("span", { class: "state", text: "void" }) : null,
    ]),
  ];

  if (!entry.void) {
    children.push(
      editing ? renderEditor(entry) : renderActions(entry),
    );
  }

  return el(
    "li",
    { class: entry.void ? "voided" : "", "data-entry": entry.uid },
    children,
  );
}

function renderActions(entry) {
  return el("div", { class: "rowActions" }, [
    el("button", {
      type: "button",
      class: "tinyButton",
      text: "Edit",
      onClick: () => {
        state.editing = entry.uid;
        notifyChanged();
      },
    }),
    el("button", {
      type: "button",
      class: "tinyButton",
      text: "Re-tag",
      title: "Change what this was for. Appends no Fact — the quantity did not move.",
      onClick: () => retag(entry),
    }),
    el("button", {
      type: "button",
      class: "tinyButton",
      text: "Void",
      title:
        "Undo this change. It is retracted from the period it happened in — for a refund, capture a positive amount today instead.",
      onClick: () => voidEntry(entry),
    }),
  ]);
}

function renderEditor(entry) {
  const amount = el("input", {
    type: "text",
    inputmode: "decimal",
    value: entry.amount,
    "aria-label": "Corrected amount",
  });
  const note = el("input", {
    type: "text",
    value: entry.note || "",
    "aria-label": "Corrected note",
  });
  const when = el("input", {
    type: "date",
    value: dateInputValue(entry.occurred_at),
    "aria-label": "Corrected date",
  });

  const save = el("button", {
    type: "button",
    class: "tinyButton primaryButton",
    text: "Save",
    onClick: async () => {
      const result = await act({
        action: "revise-entry",
        entry: entry.uid,
        // Quoted back so a stale surface loses instead of overwriting a change
        // somebody else already made.
        expected_revision: entry.revision,
        request_id: requestId("revise"),
        amount: amount.value.trim(),
        note: note.value.trim() || null,
        at: instantFromDateInput(when.value),
      });
      if (result) {
        state.editing = null;
        notifyChanged();
      }
    },
  });
  const cancel = el("button", {
    type: "button",
    class: "tinyButton",
    text: "Cancel",
    onClick: () => {
      state.editing = null;
      notifyChanged();
    },
  });

  return el("div", {}, [
    el("div", { class: "editRow" }, [
      el("label", { class: "field" }, [el("span", { text: "Amount" }), amount]),
      el("label", { class: "field" }, [el("span", { text: "When" }), when]),
      el("label", { class: "field" }, [el("span", { text: "Note" }), note]),
    ]),
    el("p", {
      class: "hint",
      text:
        "Saving compensates the original change and appends a replacement. Both stay in the Ledger; nothing is rewritten.",
    }),
    el("div", { class: "rowActions" }, [save, cancel]),
  ]);
}

async function retag(entry) {
  if (!entry.fact) {
    setNotice("This change has no Fact to re-tag.");
    return;
  }
  const current = entry.concept ? conceptLabel(entry.concept, state.conceptNames) : "";
  const next = window.prompt(
    "What was this for? Re-tagging appends no Fact — the quantity did not move.",
    current,
  );
  if (next === null) return;
  await act({
    action: "classify-fact",
    fact: entry.fact,
    concept: conceptToken(next),
    note: null,
  });
}

async function voidEntry(entry) {
  const confirmed = window.confirm(
    "Undo this change?\n\nThe amount is returned by a compensating Fact carrying the same category, and the change is retracted from the period it happened in. If the quantity really moved and came back, capture a positive amount today instead.",
  );
  if (!confirmed) return;
  await act({
    action: "void-entry",
    entry: entry.uid,
    expected_revision: entry.revision,
    request_id: requestId("void"),
  });
}
