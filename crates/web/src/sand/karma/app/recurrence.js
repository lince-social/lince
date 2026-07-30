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
  el,
  instantFromDateTimeInput,
  instantLabel,
  replaceChildren,
  requestId,
  setHidden,
  signOf,
} from "./format.js";
import { act, conceptToken, state, setNotice } from "./state.js";

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

export function wireRecurrenceForm(elements) {
  const toggle = () => {
    const hidden = elements.recurrenceForm.hasAttribute("hidden");
    setHidden(elements.recurrenceForm, !hidden);
    if (hidden) elements.ruleAmount.focus();
  };
  elements.toggleRecurrenceForm.addEventListener("click", toggle);
  elements.cancelRecurrence.addEventListener("click", () =>
    setHidden(elements.recurrenceForm, true),
  );

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
    elements.rulePreview.textContent = label
      ? `This rule ${label}.`
      : "Set at least one component, or say it happens once.";
  }
  updatePreview();

  elements.recurrenceForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    const target = elements.ruleRecord.value;
    const amount = elements.ruleAmount.value.trim();
    if (!target || !amount) {
      setNotice("A rule needs a resource and an amount.");
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
    const result = await act({
      action: "create-recurrence",
      target,
      amount,
      concept: conceptToken(elements.ruleConcept.value),
      note: elements.ruleNote.value.trim() || null,
      cadence,
      anchor_at: instantFromDateTimeInput(elements.ruleAnchor.value),
      request_id: requestId("rule"),
    });
    if (result) {
      elements.ruleAmount.value = "";
      elements.ruleNote.value = "";
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
  const sign = signOf(rule.amount);
  return el("li", { "data-rule": rule.uid, class: rule.paused ? "voided" : "" }, [
    el("div", { class: "rowTop" }, [
      el("span", { class: "amount", "data-sign": sign, text: amountText(rule.amount) }),
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
      rule.note ? el("span", { text: rule.note }) : null,
      rule.paused ? el("span", { class: "state", text: "paused" }) : null,
    ]),
    el("div", { class: "rowActions" }, [
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
    ]),
  ]);
}

/**
 * The inbox: what a rule expects that nobody has answered.
 *
 * Applied dates are omitted — they are ordinary changes now and appear in the
 * Changes list like anything else, which is the whole point of applying through
 * the same path.
 */
function renderOccurrences(elements) {
  const all = state.occurrences
    .filter((o) => o.state === "due" || o.state === "planned")
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
  const message = derivationCapped
    ? "This rule repeats faster than this list can hold. These are the next dates, not all of them."
    : pageCapped
      ? `Showing the next ${pending.length} of ${all.length} expected dates.`
      : "";
  elements.occurrenceMore.textContent = message;
  setHidden(elements.occurrenceMore, !message);
}

function renderOccurrence(occurrence) {
  const sign = signOf(occurrence.amount);
  const amountInput = el("input", {
    type: "text",
    inputmode: "decimal",
    value: occurrence.amount,
    "aria-label": "Amount to apply",
    class: "occurrenceAmount",
  });

  return el("li", { "data-occurrence": occurrence.due_at }, [
    el("div", { class: "rowTop" }, [
      el("span", { class: "amount", "data-sign": sign, text: amountText(occurrence.amount) }),
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
    el("div", { class: "editRow" }, [
      el("label", { class: "field" }, [
        el("span", { text: "Apply as" }),
        amountInput,
      ]),
      el("button", {
        type: "button",
        class: "tinyButton primaryButton",
        text: "Apply",
        title: "Turn this expected date into a real change.",
        onClick: () =>
          act({
            action: "apply-recurrence-occurrence",
            recurrence: occurrence.recurrence,
            due_at: occurrence.due_at,
            // Only sent when it differs, so the rule's own figure stays the
            // default and one month's override never edits the rule.
            amount:
              amountInput.value.trim() === occurrence.amount
                ? null
                : amountInput.value.trim(),
            note: null,
          }),
      }),
      el("button", {
        type: "button",
        class: "tinyButton",
        text: "Skip",
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
