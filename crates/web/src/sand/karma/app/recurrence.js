// Recurring gains and costs: declaring a rule, pausing it, and answering the
// dates it produces.
//
// The distinction this surface has to keep visible is between a *declaration*
// and a *change*. Creating a rule moves nothing and writes no Fact — it states
// what is expected. A date becomes real only when applied, and applying is an
// ordinary capture whose idempotency key names the rule and the date, so the
// same month can never be paid twice.

import {
  amountText,
  civilFromDateTimeInput,
  conceptLabel,
  dateTimeInputValue,
  el,
  instantFromDateTimeInput,
  instantLabel,
  replaceChildren,
  requestId,
  setHidden,
  signOf,
} from "./format.js";
import { catalogFrom, readable } from "./blocks.js";
import { act, conceptToken, state, setNotice } from "./state.js";

function conceptCatalog() {
  return catalogFrom([], [], state.concepts);
}

/**
 * The step components, largest first.
 *
 * Order matters twice over: it is the order the backend applies them in, and
 * it is the order they read in when spoken back, so "1 month and 10ms" never
 * comes out as "10ms and 1 month".
 */
const STEP_UNITS = [
  { key: "years", field: "stepYears", one: "year", many: "years" },
  { key: "months", field: "stepMonths", one: "month", many: "months" },
  { key: "weeks", field: "stepWeeks", one: "week", many: "weeks" },
  { key: "days", field: "stepDays", one: "day", many: "days" },
  { key: "hours", field: "stepHours", one: "hour", many: "hours" },
  { key: "minutes", field: "stepMinutes", one: "minute", many: "minutes" },
  { key: "seconds", field: "stepSeconds", one: "second", many: "seconds" },
  { key: "milliseconds", field: "stepMilliseconds", one: "ms", many: "ms" },
];

const WEEKDAY_NAMES = {
  monday: "Monday",
  tuesday: "Tuesday",
  wednesday: "Wednesday",
  thursday: "Thursday",
  friday: "Friday",
  saturday: "Saturday",
  sunday: "Sunday",
};

/** The presets, expressed in the same components a custom rule uses. */
const PRESETS = {
  monthly: { months: 1 },
  weekly: { weeks: 1 },
  fortnightly: { weeks: 2 },
  daily: { days: 1 },
  yearly: { years: 1 },
};

/** Build the Cadence the backend deserializes, from the components alone. */
function cadenceFrom(elements) {
  const every = {};
  for (const unit of STEP_UNITS) {
    const value = Number(elements[unit.field]?.value) || 0;
    if (value > 0) every[unit.key] = value;
  }
  const landOn = selectedWeekdays(elements);
  const cadence = {
    every,
    invalid_day: elements.ruleInvalidDay.value || "clamp",
    bound: boundFrom(elements),
  };
  // Omitted rather than sent empty: the backend's weekday set refuses to be
  // empty, and "no landing rule" is the absence of one, not a set of none.
  if (landOn.length > 0) cadence.land_on = landOn;
  return cadence;
}

/**
 * Where the rule stops — part of the rule, not a field beside it.
 *
 * "Once, on that day" is this set to one occurrence. That is the whole of a
 * one-off promise, and it is why the backend needs no separate shape for one.
 */
function boundFrom(elements) {
  const kind = elements.ruleBound?.value || "unbounded";
  if (kind === "count") {
    const occurrences = Math.max(1, Number(elements.ruleBoundCount?.value) || 1);
    return { kind: "count", occurrences };
  }
  if (kind === "until") {
    const at = civilFromDateTimeInput(elements.ruleBoundUntil?.value);
    if (at) return { kind: "until", at };
  }
  return { kind: "unbounded" };
}

/**
 * What the rule does, as the ordered list the backend stores.
 *
 * The number half comes first and the concept half second, because a rule that
 * both moves a quantity and reclassifies reads as "changed by this much, and it
 * is now that" — and the backend applies them in exactly the order written.
 *
 * Returns `{ consequences }` or `{ error }`. A rule that does nothing is an
 * error here rather than at the action boundary, so the person hears it while
 * the form is still in front of them.
 */
/** Whether the form declares a reading for the rule to work a figure out from. */
function hasCondition(elements) {
  return elements.ruleCondition.value.trim() !== "";
}

export function consequencesFrom(elements) {
  const list = [];
  const numberAction = elements.ruleNumberAction.value;
  if (numberAction !== "none") {
    const amount = elements.ruleAmount.value.trim();
    // A blank amount on a level or a movement means "whatever the condition
    // works out" — which is the point of writing a condition that computes a
    // figure rather than merely deciding yes or no. A capture still needs a
    // number, because a rule with no reading has nothing to fall back on and
    // would silently capture zero forever.
    const carries = !amount && numberAction !== "capture-entry" && hasCondition(elements);
    if (!amount && !carries) {
      return {
        error: hasCondition(elements)
          ? "That rule needs an amount, or set the number to do nothing."
          : "That rule needs an amount, or a reading to work one out from.",
      };
    }
    if (numberAction === "capture-entry") {
      list.push({
        kind: "capture-entry",
        amount,
        concept: conceptToken(elements.ruleConcept.value),
      });
    } else if (numberAction === "set-quantity-where") {
      const assertion = conceptToken(elements.ruleConcept.value);
      if (!assertion) {
        return { error: "Setting everything with a concept needs the concept." };
      }
      list.push(
        carries
          ? { kind: "set-quantity-where", assertion }
          : { kind: "set-quantity-where", assertion, value: amount },
      );
    } else {
      // `set-quantity` names a level and `add-quantity` names a movement, so
      // the field is spelled differently on purpose: assigning -1 is not a
      // movement of -1, and a projection may only sum what is summable.
      // Omitting the figure entirely is how "take the number the condition
      // computed" is said.
      if (carries) {
        list.push({ kind: numberAction });
      } else {
        list.push(
          numberAction === "set-quantity"
            ? { kind: "set-quantity", value: amount }
            : { kind: "add-quantity", delta: amount },
        );
      }
    }
  }

  const conceptAction = elements.ruleConceptAction.value;
  if (conceptAction !== "none") {
    const to = conceptToken(elements.ruleConceptTo.value);
    const from = conceptToken(elements.ruleConceptFrom.value);
    if (conceptAction === "move") {
      if (!from || !to) return { error: "Moving a concept needs both a from and a to." };
      // Two consequences, one intention. Removing before adding is what makes
      // a column move a move rather than a moment in both columns at once.
      list.push({ kind: "remove-concept", concept: from });
      list.push({ kind: "add-concept", concept: to });
    } else {
      if (!to) return { error: "That rule needs a concept to act on." };
      const kind =
        conceptAction === "add"
          ? "add-concept"
          : conceptAction === "remove"
            ? "remove-concept"
            : "set-concept";
      list.push({ kind, concept: to });
    }
  }

  if (list.length === 0) return { error: "A rule has to do something — a number, a concept, or both." };
  return { consequences: list };
}

/**
 * The *if* half, as the three fields the backend stores.
 *
 * An empty reading means unconditional, and then gate and carry must be absent
 * too — sending a gate with nothing to gate is refused, because dropping it
 * silently would turn a rule that fires sometimes into one that fires always.
 */
export function conditionFrom(elements) {
  const source = elements.ruleCondition.value.trim();
  if (!source) return { condition: null, gate: null, carry: null };

  const gateKind = elements.ruleGate.value;
  let gate = gateKind;
  if (!["!=0", "always"].includes(gateKind)) {
    const bound = elements.ruleGateValue.value.trim();
    if (!bound) return { error: "That comparison needs a number to compare against." };
    gate = `${gateKind}${bound}`;
  }

  const carryKind = elements.ruleCarry.value;
  let carry = carryKind;
  if (carryKind === "const") {
    const fixed = elements.ruleCarryValue.value.trim();
    if (!fixed) return { error: "A fixed amount needs a number." };
    carry = `const:${fixed}`;
  }
  return { condition: source, gate, carry };
}

/** Put a stored condition back into the fields that describe it. */
export function fillCondition(elements, rule) {
  elements.ruleCondition.value = readable(rule.condition || "", conceptCatalog());
  const gate = rule.gate || "!=0";
  if (gate === "!=0" || gate === "always") {
    elements.ruleGate.value = gate;
    elements.ruleGateValue.value = "";
  } else {
    // Longest operator first, or `<=3` reads as `<` with a bound of `=3`.
    const op = ["<=", ">=", "==", "<", ">"].find((o) => gate.startsWith(o)) || "!=0";
    elements.ruleGate.value = op;
    elements.ruleGateValue.value = gate.slice(op.length);
  }
  const carry = rule.carry || "value";
  if (carry.startsWith("const:")) {
    elements.ruleCarry.value = "const";
    elements.ruleCarryValue.value = carry.slice("const:".length);
  } else {
    elements.ruleCarry.value = carry;
    elements.ruleCarryValue.value = "";
  }
}

/** Say a condition back as the sentence it means. */
export function conditionLabel(rule) {
  if (!rule?.condition) return "";
  const gate = rule.gate || "!=0";
  const said =
    gate === "!=0"
      ? "is not zero"
      : gate === "always"
        ? "is anything"
        : `is ${{ "<=": "at most", ">=": "at least", "==": "exactly", "<": "under", ">": "over" }[
            ["<=", ">=", "==", "<", ">"].find((o) => gate.startsWith(o))
          ] || gate} ${gate.replace(/^[<>=]+/, "")}`;
  return `only when ${readable(rule.condition, conceptCatalog())} ${said}`;
}

/** Say back what a rule does, in the same words the form asked for it. */
export function consequencesLabel(consequences, conceptNames) {
  if (!Array.isArray(consequences)) return "";
  const said = consequences.map((c) => {
    const at = (slug) => `@${conceptLabel(slug, conceptNames)}`;
    switch (c.kind) {
      case "capture-entry":
        return c.concept
          ? `captures ${amountText(c.amount)} for ${at(c.concept)}`
          : `captures ${amountText(c.amount)}`;
      case "set-quantity":
        return c.value == null
          ? "sets the quantity to what the reading works out"
          : `sets the quantity to ${amountText(c.value)}`;
      case "add-quantity":
        return c.delta == null
          ? "adds what the reading works out to the quantity"
          : `adds ${amountText(c.delta)} to the quantity`;
      case "set-concept":
        return `replaces every concept with ${at(c.concept)}`;
      case "add-concept":
        return `adds ${at(c.concept)}`;
      case "remove-concept":
        return `removes ${at(c.concept)}`;
      default:
        return c.kind;
    }
  });
  return joinWords(said);
}

function selectedWeekdays(elements) {
  return Array.from(elements.landOnBoxes || [])
    .filter((box) => box.checked)
    .map((box) => box.value);
}

/** Say a cadence back in the words a person used to build it. */
export function cadenceLabel(cadence) {
  if (!cadence || typeof cadence !== "object") return "";
  const step = cadence.every || {};
  const parts = [];
  for (const unit of STEP_UNITS) {
    const value = Number(step[unit.key]) || 0;
    if (value <= 0) continue;
    // A lone "1 month" reads better as "every month"; anything compound keeps
    // its counts so the components stay legible.
    parts.push({ value, unit });
  }
  const bound = cadence.bound || { kind: "unbounded" };
  // A rule with no step is legal in exactly one case, and it is the most
  // ordinary one there is: this happens once, on the day it is anchored to.
  if (parts.length === 0) {
    return bound.kind === "count" && Number(bound.occurrences) === 1 ? "happens once" : "";
  }
  const words =
    parts.length === 1 && parts[0].value === 1
      ? parts[0].unit.one
      : joinWords(
          parts.map(({ value, unit }) => `${value} ${value === 1 ? unit.one : unit.many}`),
        );
  const landing = landingLabel(cadence.land_on);
  const step_text = landing ? `every ${words}, then ${landing}` : `every ${words}`;
  return `repeats ${step_text}${boundLabel(bound)}`;
}

function boundLabel(bound) {
  if (bound.kind === "count") {
    const times = Number(bound.occurrences) || 0;
    return times === 1 ? ", once" : `, ${times} times`;
  }
  if (bound.kind === "until") return `, until ${String(bound.at).slice(0, 10)}`;
  return "";
}

function landingLabel(landOn) {
  if (!Array.isArray(landOn) || landOn.length === 0) return "";
  const names = landOn.map((day) => WEEKDAY_NAMES[day] || day);
  return `forward to ${joinWords(names)}`;
}

function joinWords(items) {
  if (items.length <= 1) return items.join("");
  if (items.length === 2) return `${items[0]} and ${items[1]}`;
  return `${items.slice(0, -1).join(", ")} and ${items[items.length - 1]}`;
}

/**
 * The rule being edited, or null when the form is declaring a new one.
 *
 * The revision is carried along because a revise is refused if the rule moved
 * underneath the form — two people editing one rule must not have the slower
 * save silently win.
 */
let editing = null;

/**
 * Set once the form is wired. The rule list renders independently of the form,
 * so this is how a row reaches it without the two importing each other.
 */
let openRuleEditor = () => {};

/**
 * A stored consequence names its concept by uid, because the Action resolved it
 * when the rule was saved. The form asks for the name a person typed, so the
 * uid has to be spoken back as that name — otherwise editing a rule would show
 * an identifier where its author wrote "rent".
 */
function conceptField(uid) {
  return uid ? `@${conceptLabel(uid, state.conceptNames)}` : "";
}

/** Put a stored consequence list back into the two halves of the *Then* block. */
export function fillConsequences(elements, consequences) {
  const list = Array.isArray(consequences) ? consequences : [];
  const number = list.find((c) =>
    ["capture-entry", "set-quantity", "add-quantity"].includes(c.kind),
  );
  elements.ruleNumberAction.value = number ? number.kind : "none";
  elements.ruleAmount.value = number
    ? String(number.amount ?? number.value ?? number.delta ?? "")
    : "";
  elements.ruleConcept.value = conceptField(number?.concept);

  const removed = list.find((c) => c.kind === "remove-concept");
  const added = list.find((c) => c.kind === "add-concept");
  const replaced = list.find((c) => c.kind === "set-concept");
  // A remove paired with an add is what the form calls a move, so it has to
  // read back as one — otherwise saving an untouched rule would rewrite it
  // into a shape its author never chose.
  if (removed && added) {
    elements.ruleConceptAction.value = "move";
    elements.ruleConceptFrom.value = conceptField(removed.concept);
    elements.ruleConceptTo.value = conceptField(added.concept);
    return;
  }
  const single = replaced || added || removed;
  elements.ruleConceptAction.value = single
    ? { "set-concept": "set", "add-concept": "add", "remove-concept": "remove" }[single.kind]
    : "none";
  elements.ruleConceptFrom.value = "";
  elements.ruleConceptTo.value = conceptField(single?.concept);
}

/** Put a stored cadence back into the components that describe it. */
export function fillCadence(elements, cadence) {
  const step = cadence?.every || {};
  for (const unit of STEP_UNITS) {
    const field = elements[unit.field];
    if (field) field.value = String(Number(step[unit.key]) || 0);
  }
  // The components are the rule, so the preset select says "custom" unless the
  // person picks one. Guessing which preset a stored step matches would be a
  // second representation, and the whole point is that there isn't one.
  elements.rulePreset.value = "custom";
  const landOn = Array.isArray(cadence?.land_on) ? cadence.land_on : [];
  for (const box of elements.landOnBoxes || []) {
    box.checked = landOn.includes(box.value);
  }
  elements.ruleInvalidDay.value = cadence?.invalid_day || "clamp";
  const bound = cadence?.bound || { kind: "unbounded" };
  elements.ruleBound.value = bound.kind;
  elements.ruleBoundCount.value = String(Number(bound.occurrences) || 1);
  elements.ruleBoundUntil.value = bound.at ? dateTimeInputValue(bound.at) : "";
}

/** Hand the form back to declaring a new rule. */
function leaveEditMode(elements) {
  editing = null;
  elements.recurrenceSubmit.textContent = "Declare rule";
  // A rule cannot be moved to another Record by revising it — the revision
  // carries no target — so the control that would suggest otherwise comes back
  // only when a new rule is being declared.
  elements.ruleRecord.disabled = false;
}

export function wireRecurrenceForm(elements) {
  const toggle = () => {
    const hidden = elements.recurrenceForm.hasAttribute("hidden");
    // Reopening the form from the "+ New rule" button always means a new rule,
    // never a half-finished edit of whichever one was open last.
    leaveEditMode(elements);
    setHidden(elements.recurrenceForm, !hidden);
    if (hidden) elements.ruleAmount.focus();
  };
  elements.toggleRecurrenceForm.addEventListener("click", toggle);
  elements.cancelRecurrence.addEventListener("click", () => {
    leaveEditMode(elements);
    setHidden(elements.recurrenceForm, true);
  });

  // A preset writes the components and nothing else. There is no hidden second
  // representation, so what the fields say is always what the rule is.
  const applyPreset = () => {
    const preset = PRESETS[elements.rulePreset.value];
    if (!preset) return;
    for (const unit of STEP_UNITS) {
      const field = elements[unit.field];
      if (field) field.value = String(preset[unit.key] ?? 0);
    }
    updatePreview();
  };
  elements.rulePreset.addEventListener("change", applyPreset);

  // Editing a component by hand means the rule is no longer the preset it
  // started from, and saying so beats a select that quietly disagrees.
  const markCustom = () => {
    elements.rulePreset.value = "custom";
    updatePreview();
  };
  for (const unit of STEP_UNITS) {
    elements[unit.field]?.addEventListener("input", markCustom);
  }
  for (const box of elements.landOnBoxes || []) {
    box.addEventListener("change", updatePreview);
  }
  // Only the fields the chosen consequences actually use stay on screen. An
  // amount box beside "do nothing to the number" is a question the rule is not
  // answering, and a filled one would read as an instruction being ignored.
  const updateThen = () => {
    const numberAction = elements.ruleNumberAction.value;
    setHidden(elements.ruleAmountField, numberAction === "none");
    setHidden(
      elements.ruleConceptField,
      !["capture-entry", "set-quantity-where"].includes(numberAction),
    );
    const conceptAction = elements.ruleConceptAction.value;
    setHidden(elements.ruleConceptFromField, conceptAction !== "move");
    setHidden(elements.ruleConceptToField, conceptAction === "none");
    updatePreview();
  };
  // The gate and carry are questions only a conditional rule is asking. An
  // unconditional rule showing "and the amount is…" would offer a choice that
  // changes nothing.
  const updateIf = () => {
    const conditional = elements.ruleCondition.value.trim().length > 0;
    setHidden(elements.ruleGateField, !conditional);
    setHidden(elements.ruleCarryField, !conditional);
    const needsBound =
      conditional && !["!=0", "always"].includes(elements.ruleGate.value);
    setHidden(elements.ruleGateValueField, !needsBound);
    setHidden(elements.ruleCarryValueField, !(conditional && elements.ruleCarry.value === "const"));
    updatePreview();
  };
  elements.ruleCondition.addEventListener("input", updateIf);
  elements.ruleGate.addEventListener("change", updateIf);
  elements.ruleCarry.addEventListener("change", updateIf);
  elements.ruleGateValue.addEventListener("input", updatePreview);
  elements.ruleCarryValue.addEventListener("input", updatePreview);
  updateIf();

  elements.ruleNumberAction.addEventListener("change", updateThen);
  elements.ruleConceptAction.addEventListener("change", updateThen);
  for (const field of [
    elements.ruleAmount,
    elements.ruleConcept,
    elements.ruleConceptFrom,
    elements.ruleConceptTo,
  ]) {
    field?.addEventListener("input", updatePreview);
  }
  updateThen();

  elements.ruleInvalidDay.addEventListener("change", updatePreview);
  elements.ruleBound?.addEventListener("change", updatePreview);
  elements.ruleBoundCount?.addEventListener("input", updatePreview);
  elements.ruleBoundUntil?.addEventListener("input", updatePreview);

  // The short-month question only exists for a step with a calendar part.
  function updatePreview() {
    const cadence = cadenceFrom(elements);
    const months = (Number(cadence.every.years) || 0) * 12 + (Number(cadence.every.months) || 0);
    setHidden(elements.ruleInvalidDayField, months === 0);
    // Only the chosen bound's own input is on screen; the other two would be
    // asking a question this rule is not answering.
    const kind = cadence.bound?.kind || "unbounded";
    setHidden(elements.ruleBoundCountField, kind !== "count");
    setHidden(elements.ruleBoundUntilField, kind !== "until");
    const label = cadenceLabel(cadence);
    if (!label) {
      elements.rulePreview.textContent = "Set at least one component, or say it happens once.";
      return;
    }
    // The half-built states are silent on purpose: an amount box mid-typing is
    // not a rule that does nothing, and saying so on every keystroke would
    // train a person to stop reading this line.
    const { consequences } = consequencesFrom(elements);
    const does = consequences ? consequencesLabel(consequences, state.conceptNames) : "";
    elements.rulePreview.textContent = does
      ? `This rule ${label}, and each time ${does}.`
      : `This rule ${label}.`;
  }
  updatePreview();

  /**
   * Load an existing rule into the form.
   *
   * The same form, not a second one: an edit surface that drifts from the
   * create surface is how a rule ends up with a shape only one of them can
   * express.
   */
  openRuleEditor = (rule) => {
    editing = { uid: rule.uid, revision: rule.revision };
    elements.recurrenceSubmit.textContent = "Save rule";
    elements.ruleRecord.value = rule.record;
    elements.ruleRecord.disabled = true;
    elements.ruleNote.value = rule.note || "";
    elements.ruleAnchor.value = dateTimeInputValue(rule.anchor_at);
    fillConsequences(elements, rule.consequences);
    fillCondition(elements, rule);
    fillCadence(elements, rule.cadence);
    updateThen();
    updateIf();
    setHidden(elements.recurrenceForm, false);
    elements.recurrenceForm.scrollIntoView({ block: "nearest" });
  };

  elements.recurrenceForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    const target = elements.ruleRecord.value;
    if (!target) {
      setNotice("A rule needs a resource.");
      return;
    }
    const { consequences, error } = consequencesFrom(elements);
    if (error) {
      setNotice(error);
      return;
    }
    const cadence = cadenceFrom(elements);
    // An empty step is only meaningful for a rule that never reaches a second
    // occurrence — which is exactly how "once, on that day" is said.
    const once = cadence.bound?.kind === "count" && Number(cadence.bound.occurrences) === 1;
    if (Object.keys(cadence.every).length === 0 && !once) {
      setNotice("A rule needs a component to repeat by, or set it to happen once.");
      return;
    }
    const asked = conditionFrom(elements);
    if (asked.error) {
      setNotice(asked.error);
      return;
    }
    const common = {
      // A rule carries a list of what it does, not a single amount.
      consequences,
      condition: asked.condition,
      gate: asked.gate,
      carry: asked.carry,
      note: elements.ruleNote.value.trim() || null,
      cadence,
      anchor_at: instantFromDateTimeInput(elements.ruleAnchor.value),
    };
    const result = await act(
      editing
        ? {
            action: "revise-recurrence",
            recurrence: editing.uid,
            // Refused if the rule moved under the form. A revise that wins by
            // being slower would silently discard the other edit.
            expected_revision: editing.revision,
            request_id: requestId("revise-rule"),
            ...common,
          }
        : {
            action: "create-recurrence",
            target,
            request_id: requestId("rule"),
            ...common,
          },
    );
    if (result) {
      leaveEditMode(elements);
      // Every field a rule's identity lives in, so the next rule declared does
      // not silently inherit the last one's concepts.
      elements.ruleAmount.value = "";
      elements.ruleNote.value = "";
      elements.ruleConcept.value = "";
      elements.ruleConceptFrom.value = "";
      elements.ruleConceptTo.value = "";
      elements.ruleCondition.value = "";
      elements.ruleGateValue.value = "";
      elements.ruleCarryValue.value = "";
      setHidden(elements.recurrenceForm, true);
    }
  });
}

export function renderRules(elements) {
  setHidden(elements.recurrenceEmpty, state.rules.length > 0);
  replaceChildren(elements.recurrenceList, state.rules.map(renderRule));
  renderOccurrences(elements);
}

function recordName(uid) {
  const record = state.records.find((r) => r.uid === uid);
  return record?.head || record?.slug || uid;
}

function renderRule(rule) {
  const does = consequencesLabel(rule.consequences, state.conceptNames);
  // A rule that moves no quantity has no figure to lead with, and a 0 there
  // would be a claim — "this rule moves nothing" — rather than the absence of
  // a number. Such a rule leads with what it does instead.
  const hasAmount = rule.amount !== null && rule.amount !== undefined;
  const lead = hasAmount
    ? el("span", { class: "amount", "data-sign": signOf(rule.amount), text: amountText(rule.amount) })
    : el("span", { class: "state", text: does });
  return el("li", { "data-rule": rule.uid, class: rule.paused ? "voided" : "" }, [
    el("div", { class: "rowTop" }, [
      lead,
      el("span", { class: "rowMetaDate", text: cadenceLabel(rule.cadence) }),
    ]),
    el("div", { class: "rowMeta" }, [
      el("span", { text: recordName(rule.record) }),
      el("span", {
        class: "tag",
        "data-unclassified": rule.concept ? "false" : "true",
        text: rule.concept
          ? `@${conceptLabel(rule.concept, state.conceptNames)}`
          : "unclassified",
      }),
      // The figure above is only the rule's first consequence. A rule that also
      // reclassifies would otherwise look identical to one that only captures,
      // which is the difference a person scanning this list most needs to see.
      hasAmount && rule.consequences?.length > 1 ? el("span", { text: does }) : null,
      // A conditional rule and an unconditional one look identical otherwise,
      // and "why did this not fire?" is the question that difference answers.
      rule.condition ? el("span", { class: "state", text: conditionLabel(rule) }) : null,
      rule.note ? el("span", { text: rule.note }) : null,
      rule.paused ? el("span", { class: "state", text: "paused" }) : null,
    ]),
    el("div", { class: "rowActions" }, [
      el("button", {
        type: "button",
        class: "tinyButton",
        text: "Edit",
        title: "Change what this rule does or when. Dates already applied stay as they were.",
        onClick: () => openRuleEditor(rule),
      }),
      el("button", {
        type: "button",
        class: "tinyButton",
        text: rule.paused ? "Resume" : "Pause",
        title: rule.paused
          ? "Offer this rule's future dates again."
          : "Stop offering future dates. Nothing already applied is disowned.",
        onClick: () =>
          act({
            action: "set-recurrence-paused",
            recurrence: rule.uid,
            expected_revision: rule.revision,
            request_id: requestId("pause"),
            paused: !rule.paused,
          }),
      }),
      el("button", {
        type: "button",
        class: "tinyButton",
        text: "Delete",
        // Worth saying plainly on the button: what a delete removes is the
        // rule's future. Dates it already applied are ordinary changes, and
        // they stay, because the rule proposed them and never owned them.
        title: "Remove this rule and its future dates. Changes it already applied stay.",
        onClick: () => {
          const does = consequencesLabel(rule.consequences, state.conceptNames);
          const confirmed = window.confirm(
            `Delete this rule? It ${does}. Its future dates go; anything it already applied stays.`,
          );
          if (!confirmed) return;
          // No request id: a delete is not replayable. Asking for the same
          // rule twice is genuinely an error, not a duplicate to absorb.
          act({ action: "delete-recurrence", recurrence: rule.uid });
        },
      }),
    ]),
  ]);
}

/**
 * The inbox: what a rule expects, and what has been decided about it.
 *
 * Applied dates are omitted — they are ordinary changes now and appear in the
 * Changes list like anything else, which is the whole point of applying through
 * the same path.
 *
 * Skipped dates *are* listed. Declining is a decision, and the reason skipping
 * exists at all is that "decided against" and "nobody has looked yet" must not
 * read the same — which a list that hides skips quietly undoes. It is also the
 * only place the decision can be taken back.
 *
 * Since the heartbeat applies due dates on its own, `due` now lasts about one
 * beat: this list is mostly what is *coming*, and skipping ahead of time is how
 * a person says "not this one" before the wheel acts.
 */
function renderOccurrences(elements) {
  const all = state.occurrences
    .filter((o) => o.state === "due" || o.state === "planned" || o.state === "skipped")
    .sort((a, b) => String(a.due_at).localeCompare(String(b.due_at)));
  const pending = all.slice(0, 40);

  setHidden(elements.occurrenceEmpty, pending.length > 0);
  replaceChildren(
    elements.occurrenceList,
    pending.map((occurrence) => renderOccurrence(occurrence)),
  );

  // Two different ways this list can be a prefix, and both have to be said. A
  // page that quietly stops looks like an obligation fully answered, which is
  // the one impression a list of unpaid dates must never give.
  const derivationCapped = state.rules.some((rule) => rule.truncated);
  const pageCapped = all.length > pending.length;
  // A third way to be a prefix, and the quietest: the list only asked back so
  // far. Said whenever something past is still unanswered, because that is when
  // a person has reason to wonder whether older dates are waiting too.
  const hasUnanswered = pending.some((o) => o.state === "due");
  const lookback =
    hasUnanswered && state.occurrenceSince
      ? ` Dates before ${instantLabel(state.occurrenceSince)} are not listed.`
      : "";
  const message = derivationCapped
    ? `This rule repeats faster than this list can hold. These are the next dates, not all of them.${lookback}`
    : pageCapped
      ? `Showing the next ${pending.length} of ${all.length} expected dates.${lookback}`
      : lookback.trim();
  elements.occurrenceMore.textContent = message;
  setHidden(elements.occurrenceMore, !message);
}

function renderOccurrence(occurrence) {
  // A date belonging to a rule that captures nothing has no figure and no
  // amount to override. Offering an editable box there would invite a number
  // that the apply path has nowhere to put.
  const hasAmount = occurrence.amount !== null && occurrence.amount !== undefined;
  const rule = state.rules.find((r) => r.uid === occurrence.recurrence);
  const amountInput = hasAmount
    ? el("input", {
        type: "text",
        inputmode: "decimal",
        value: occurrence.amount,
        "aria-label": "Amount to apply",
        class: "occurrenceAmount",
      })
    : null;

  const skipped = occurrence.state === "skipped";

  return el("li", { "data-occurrence": occurrence.due_at, class: skipped ? "voided" : "" }, [
    el("div", { class: "rowTop" }, [
      hasAmount
        ? el("span", {
            class: "amount",
            "data-sign": signOf(occurrence.amount),
            text: amountText(occurrence.amount),
          })
        : el("span", {
            class: "state",
            text: consequencesLabel(rule?.consequences, state.conceptNames),
          }),
      el("span", { class: "state", "data-state": occurrence.state, text: occurrence.state }),
    ]),
    el("div", { class: "rowMeta" }, [
      el("span", { text: instantLabel(occurrence.due_at) }),
      el("span", { text: recordName(occurrence.record) }),
      occurrence.concept
        ? el("span", {
            class: "tag",
            text: `@${conceptLabel(occurrence.concept, state.conceptNames)}`,
          })
        : null,
    ]),
    // A declined date offers one thing: taking the decision back. Leaving
    // Apply beside it would let a skip be overridden without ever being
    // withdrawn, so the list would show "skipped" over a date that ran.
    el("div", { class: "editRow" }, skipped
      ? [
          el("button", {
            type: "button",
            class: "tinyButton",
            text: "Undo skip",
            title: "Expect this date again. The wheel may then apply it like any other.",
            onClick: () =>
              act({
                action: "unskip-recurrence-occurrence",
                recurrence: occurrence.recurrence,
                due_at: occurrence.due_at,
              }),
          }),
        ]
      : [
          amountInput
            ? el("label", { class: "field" }, [el("span", { text: "Apply as" }), amountInput])
            : null,
          el("button", {
            type: "button",
            class: "tinyButton primaryButton",
            text: "Apply",
            title: "Turn this expected date into a real change now, without waiting for the beat.",
            onClick: () =>
              act({
                action: "apply-recurrence-occurrence",
                recurrence: occurrence.recurrence,
                due_at: occurrence.due_at,
                // Only sent when it differs, so the rule's own figure stays the
                // default and one month's override never edits the rule.
                amount:
                  !amountInput || amountInput.value.trim() === occurrence.amount
                    ? null
                    : amountInput.value.trim(),
                note: null,
              }),
          }),
          el("button", {
            type: "button",
            class: "tinyButton",
            text: "Skip",
            // Now the main way to opt out: the heartbeat applies a due date on
            // its own, so declining ahead of time is how a person says "not
            // this one" before the wheel acts.
            title: "Decline this date. Recorded, so it does not read as merely unanswered.",
            onClick: () =>
              act({
                action: "skip-recurrence-occurrence",
                recurrence: occurrence.recurrence,
                due_at: occurrence.due_at,
                note: null,
              }),
          }),
        ]),
  ]);
}
