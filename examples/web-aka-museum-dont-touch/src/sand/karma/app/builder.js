// The rule builder: condition, threshold, consequence, in that order.
//
// This is the surface a rule is actually made on. Everything below the "slop
// down here" divider is the older surface, still working, still authoritative
// for the fields this one does not yet cover.
//
// The arithmetic of *finding* a block lives in `blocks.js` as pure functions.
// This file is the DOM half: it reads the caret, paints the list, and turns
// what was picked into a typed Action.

import {
  activeQuery,
  applyCompletion,
  blocksIn,
  catalogFrom,
  conceptName,
  insertAtCaret,
  rankBlocks,
  readable,
} from "./blocks.js";
import { el, replaceChildren, requestId, setHidden } from "./format.js";
import { act, setNotice, state } from "./state.js";

/** Which suggestion the arrow keys have landed on. */
let highlighted = 0;
/** The blocks currently offered, so Tab and a click accept the same thing. */
let offered = [];
/** The query those blocks answer, kept so accepting knows what to replace. */
let offeredFor = null;

function catalog() {
  return catalogFrom(state.records, state.frequencies, state.concepts);
}

// ------------------------------------------------------------- the condition

/** Repaint the chip strip: every block the condition names, in order. */
function renderChips(elements) {
  const found = blocksIn(elements.conditionInput.value, catalog());
  replaceChildren(
    elements.conditionChips,
    found.map((block) =>
      el("span", {
        class: "tokenChip",
        text: block.head,
        "data-kind": block.kind,
        title: block.kind === "unknown" ? `Nothing is called ${block.source}` : block.source,
      }),
    ),
  );
  // A slug nothing answers to is the one error worth saying before the Action
  // refuses it, because it reads as an empty reading rather than a mistake.
  const unknown = found.filter((block) => block.kind === "unknown");
  setHidden(elements.conditionError, unknown.length === 0);
  if (unknown.length) {
    const names = unknown.map((block) => block.source).join(", ");
    elements.conditionError.textContent = `Nothing here is called ${names}.`;
  }
}

function closeSuggestions(elements) {
  offered = [];
  offeredFor = null;
  highlighted = 0;
  setHidden(elements.conditionSuggest, true);
  elements.conditionInput.setAttribute("aria-expanded", "false");
}

function renderSuggestions(elements) {
  const input = elements.conditionInput;
  const active = activeQuery(input.value, input.selectionStart);
  if (!active) {
    closeSuggestions(elements);
    return;
  }
  const matches = rankBlocks(catalog(), active.kind, active.query);
  if (!matches.length) {
    closeSuggestions(elements);
    return;
  }
  offered = matches;
  offeredFor = active;
  highlighted = Math.min(highlighted, matches.length - 1);

  replaceChildren(
    elements.conditionSuggest,
    matches.map((block, index) => {
      const item = el("li", {
        class: "suggestItem",
        role: "option",
        "aria-selected": index === highlighted ? "true" : "false",
      });
      item.appendChild(el("span", { class: "suggestName", text: block.head }));
      item.appendChild(
        el("span", {
          class: "suggestMeta",
          text:
            block.kind === "frequency"
              ? `freq(@${block.slug})`
              : block.kind === "concept"
                ? `#${block.slug}`
                : `@${block.slug}`,
        }),
      );
      // mousedown, not click: the input must not lose focus and close the list
      // out from under the pointer before the pick lands.
      item.addEventListener("mousedown", (event) => {
        event.preventDefault();
        accept(elements, index);
      });
      return item;
    }),
  );
  setHidden(elements.conditionSuggest, false);
  input.setAttribute("aria-expanded", "true");
}

function accept(elements, index) {
  const block = offered[index];
  if (!block || !offeredFor) return;
  const input = elements.conditionInput;
  const next = applyCompletion(input.value, offeredFor, block.kind, block.slug);
  input.value = next.text;
  input.setSelectionRange(next.caret, next.caret);
  closeSuggestions(elements);
  renderChips(elements);
  input.focus();
}

function wireCondition(elements) {
  const input = elements.conditionInput;

  input.addEventListener("input", () => {
    renderChips(elements);
    renderSuggestions(elements);
  });
  // Moving the caret changes which query it sits in, so the list follows it.
  input.addEventListener("click", () => renderSuggestions(elements));
  input.addEventListener("blur", () => {
    // After the pointer has had its chance to land on an item.
    setTimeout(() => closeSuggestions(elements), 120);
  });

  input.addEventListener("keydown", (event) => {
    if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
      // The caret moves first; the list catches up after.
      setTimeout(() => renderSuggestions(elements), 0);
      return;
    }
    if (!offered.length) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      highlighted = (highlighted + 1) % offered.length;
      renderSuggestions(elements);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      highlighted = (highlighted - 1 + offered.length) % offered.length;
      renderSuggestions(elements);
    } else if (event.key === "Tab" || event.key === "Enter") {
      // Tab takes the first match, which is the whole promise of typing
      // `freq(dai` and reaching for the key without looking.
      event.preventDefault();
      accept(elements, highlighted);
    } else if (event.key === "Escape") {
      event.preventDefault();
      closeSuggestions(elements);
    }
  });
}

// ------------------------------------------------------------ record picker

function wireRecordPicker(elements) {
  elements.recordSearch.addEventListener("input", () => renderRecordResults(elements));

  elements.recordResults.addEventListener("click", (event) => {
    const button = event.target.closest?.("[data-slug]");
    if (!button) return;
    const input = elements.conditionInput;
    const next = insertAtCaret(input.value, input.selectionStart, "record", button.dataset.slug);
    input.value = next.text;
    input.focus();
    input.setSelectionRange(next.caret, next.caret);
    renderChips(elements);
  });
}

function renderRecordResults(elements) {
  const needle = String(elements.recordSearch?.value ?? "").trim().toLowerCase();
  const matches = rankBlocks(
    catalog().filter((block) => block.kind === "record"),
    "record",
    needle,
    12,
  );
  replaceChildren(
    elements.recordResults,
    matches.map((block) => {
      const item = el("li");
      const button = el("button", {
        class: "recordResult",
        type: "button",
        "data-slug": block.slug,
        "data-uid": block.uid || "",
      });
      button.appendChild(el("span", { text: block.head }));
      button.appendChild(el("span", { class: "recordResultSlug", text: `@${block.slug}` }));
      item.appendChild(button);
      return item;
    }),
  );
  setHidden(elements.recordResultsEmpty, matches.length > 0);
}

// -------------------------------------------------- reusable parts (banks)

/**
 * The conditions other rules already read.
 *
 * Derived rather than stored: a distinct `condition` string across the rules
 * Protein already serves is a real, reusable part, and needs no table to
 * become one. Naming them is what the backend adds later.
 */
export function conditionBank(rules) {
  const seen = new Map();
  for (const rule of rules || []) {
    const source = String(rule?.condition || "").trim();
    if (!source) continue;
    if (!seen.has(source)) seen.set(source, { source, uses: 0 });
    seen.get(source).uses += 1;
  }
  return [...seen.values()].sort((a, b) => b.uses - a.uses || a.source.localeCompare(b.source));
}

/** The same, for what other rules do. Keyed on the shape, not the wording. */
export function consequenceBank(rules) {
  const seen = new Map();
  for (const rule of rules || []) {
    const list = Array.isArray(rule?.consequences) ? rule.consequences : [];
    for (const consequence of list) {
      if (!consequence?.kind) continue;
      const key = JSON.stringify(consequence);
      if (!seen.has(key)) seen.set(key, { consequence, uses: 0 });
      seen.get(key).uses += 1;
    }
  }
  return [...seen.values()].sort(
    (a, b) => b.uses - a.uses || a.consequence.kind.localeCompare(b.consequence.kind),
  );
}

function consequenceLabel(consequence) {
  const amount = consequence.amount ?? consequence.delta ?? consequence.value;
  switch (consequence.kind) {
    case "capture-entry":
      return `capture ${amount ?? "what it carried"}${consequence.concept ? ` for @${consequence.concept}` : ""}`;
    case "add-quantity":
      return `add ${amount ?? "what it carried"}`;
    case "set-quantity":
      return `set to ${amount ?? "what it carried"}`;
    case "set-quantity-where":
      return `set everything #${conceptName(consequence.assertion, catalog())} to ${amount ?? "what it carried"}`;
    case "add-concept":
      return `add @${consequence.concept}`;
    case "remove-concept":
      return `remove @${consequence.concept}`;
    case "run-command":
      return `run ${consequence.command}`;
    default:
      return consequence.kind;
  }
}

function renderBanks(elements) {
  const conditions = conditionBank(state.rules);
  replaceChildren(
    elements.conditionBank,
    conditions.map((entry) => {
      const item = el("li");
      const button = el("button", { class: "bankItem", type: "button" });
      const shown = readable(entry.source, catalog());
      button.appendChild(el("span", { text: shown }));
      button.appendChild(
        el("span", {
          class: "bankItemSource",
          text: entry.uses > 1 ? `${entry.uses} rules` : "1 rule",
        }),
      );
      button.addEventListener("click", () => {
        elements.conditionInput.value = shown;
        renderChips(elements);
        elements.conditionInput.focus();
      });
      item.appendChild(button);
      return item;
    }),
  );
  setHidden(elements.conditionBankEmpty, conditions.length > 0);

  const consequences = consequenceBank(state.rules);
  replaceChildren(
    elements.consequenceBank,
    consequences.map((entry) => {
      const item = el("li");
      const button = el("button", { class: "bankItem", type: "button" });
      button.appendChild(el("span", { text: consequenceLabel(entry.consequence) }));
      button.appendChild(
        el("span", {
          class: "bankItemSource",
          text: entry.uses > 1 ? `${entry.uses} rules` : "1 rule",
        }),
      );
      button.addEventListener("click", () => applyConsequence(elements, entry.consequence));
      item.appendChild(button);
      return item;
    }),
  );
  setHidden(elements.consequenceBankEmpty, consequences.length > 0);
}

/** Load a borrowed consequence back into the controls that can express it. */
function applyConsequence(elements, consequence) {
  elements.builderConsequence.value = consequence.kind;
  const amount = consequence.amount ?? consequence.delta ?? consequence.value;
  elements.builderAmount.value = amount ?? "";
  elements.builderConcept.value = conceptName(
    consequence.concept || consequence.assertion || "",
    catalog(),
  );
  elements.builderCommand.value = consequence.command || "";
  updateConsequenceFields(elements);
}

// ---------------------------------------------------------------- the form

/** Only the fields the chosen consequence actually answers. */
function updateConsequenceFields(elements) {
  const kind = elements.builderConsequence.value;
  setHidden(
    elements.builderAmountField,
    !["capture-entry", "add-quantity", "set-quantity", "set-quantity-where"].includes(kind),
  );
  setHidden(
    elements.builderConceptField,
    !["capture-entry", "add-concept", "remove-concept", "set-quantity-where"].includes(kind),
  );
  setHidden(elements.builderCommandField, kind !== "run-command");
}

function updateGateFields(elements) {
  const gate = elements.builderGate.value;
  setHidden(elements.builderGateValueField, gate === "!=0" || gate === "always");
}

/** The gate as the kernel spells it: an operator with its number attached. */
export function gateFrom(gate, value) {
  if (gate === "!=0" || gate === "always") return gate;
  const number = String(value ?? "").trim();
  if (!number) return null;
  return `${gate}${number}`;
}

/** What the rule does, as the one-element list the Action expects. */
export function consequenceFrom(kind, { amount, concept, command } = {}) {
  const number = String(amount ?? "").trim();
  const parsed = number === "" ? null : number;
  switch (kind) {
    case "capture-entry":
      // A capture must name a figure: there is no "capture whatever" that a
      // Ledger could record.
      if (!parsed) return { error: "A capture needs an amount." };
      return { consequence: { kind, amount: parsed, concept: concept || null } };
    case "add-quantity":
      return { consequence: { kind, delta: parsed } };
    case "set-quantity":
      return { consequence: { kind, value: parsed } };
    case "set-quantity-where":
      if (!concept) return { error: "That consequence needs a concept to act on." };
      return { consequence: { kind, assertion: concept, value: parsed } };
    case "add-concept":
    case "remove-concept":
      if (!concept) return { error: "That consequence needs a concept." };
      return { consequence: { kind, concept } };
    case "run-command":
      if (!command) return { error: "That consequence needs a command." };
      return { consequence: { kind, command } };
    default:
      return { error: "Pick what the rule does." };
  }
}

function resetBuilder(elements) {
  elements.conditionInput.value = "";
  elements.builderAmount.value = "";
  elements.builderConcept.value = "";
  elements.builderCommand.value = "";
  elements.builderGateValue.value = "";
  renderChips(elements);
  closeSuggestions(elements);
}

export function wireBuilder(elements) {
  wireCondition(elements);
  wireRecordPicker(elements);

  elements.builderGate.addEventListener("change", () => updateGateFields(elements));
  elements.builderConsequence.addEventListener("change", () =>
    updateConsequenceFields(elements),
  );
  updateGateFields(elements);
  updateConsequenceFields(elements);

  const toggleBank = (button, list, empty) => {
    button.addEventListener("click", () => {
      const showing = !list.hasAttribute("hidden") || !empty.hasAttribute("hidden");
      setHidden(list, showing);
      setHidden(empty, showing || list.childElementCount > 0);
    });
  };
  toggleBank(elements.conditionBankToggle, elements.conditionBank, elements.conditionBankEmpty);
  toggleBank(
    elements.consequenceBankToggle,
    elements.consequenceBank,
    elements.consequenceBankEmpty,
  );

  elements.builderReset.addEventListener("click", () => resetBuilder(elements));

  elements.ruleBuilderForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    const target = elements.builderTarget.value;
    if (!target) {
      setNotice("A rule needs a record to act on.");
      return;
    }
    const condition = elements.conditionInput.value.trim();
    if (!condition) {
      setNotice("A rule needs a condition to read.");
      return;
    }
    const gate = gateFrom(elements.builderGate.value, elements.builderGateValue.value);
    if (!gate) {
      setNotice("That threshold needs a number.");
      return;
    }
    const { consequence, error } = consequenceFrom(elements.builderConsequence.value, {
      amount: elements.builderAmount.value,
      concept: elements.builderConcept.value.trim(),
      command: elements.builderCommand.value.trim(),
    });
    if (error) {
      setNotice(error);
      return;
    }

    const result = await act({
      action: "create-recurrence",
      target,
      request_id: requestId("rule"),
      consequences: [consequence],
      condition,
      gate,
      carry: "value",
      note: null,
      // Until the Frequency table lands, a rule still carries the cadence that
      // decides how often it is *looked at*; the `freq(@x)` term in the
      // condition is what decides whether looking means acting. A daily look is
      // the safe default — the beat is the condition's business, not this
      // form's. This whole field disappears in the backend pass.
      cadence: { every: { days: 1 }, bound: { kind: "unbounded" } },
      anchor_at: new Date().toISOString(),
    });
    if (result) resetBuilder(elements);
  });
}

export function renderBuilder(elements) {
  renderChips(elements);
  renderBanks(elements);
  renderRecordResults(elements);
}
