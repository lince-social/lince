// Frequencies: declared once, read by any condition.
//
// A Frequency is a slug and a step. That is the whole object — the same
// `Cadence` the engine already computes dates from, which is why one thing can
// fire a rule and draw a calendar without a second description of "when".
//
// There is deliberately no per-rule cadence here. A rule that wants a beat
// reads `freq(@daily)`; the schedule belongs to the Frequency, and every rule
// naming it shares the one definition rather than each restating it and
// drifting apart the first time one is edited.

import { cadenceLabel } from "./recurrence.js";
import { dateTimeInputValue, el, replaceChildren, requestId, setHidden } from "./format.js";
import { act, setNotice, state } from "./state.js";

/** The presets, as the step components they stand for. */
const PRESETS = {
  daily: { days: 1 },
  weekly: { weeks: 1 },
  fortnightly: { weeks: 2 },
  monthly: { months: 1 },
  yearly: { years: 1 },
};

const UNITS = [
  ["years", "freqYears"],
  ["months", "freqMonths"],
  ["weeks", "freqWeeks"],
  ["days", "freqDays"],
  ["hours", "freqHours"],
  ["minutes", "freqMinutes"],
  ["seconds", "freqSeconds"],
  ["milliseconds", "freqMilliseconds"],
];

/** The step the unit boxes describe, omitting the components left at zero. */
export function stepFrom(values) {
  const step = {};
  for (const [unit] of UNITS) {
    const amount = Number(values?.[unit]) || 0;
    if (amount > 0) step[unit] = amount;
  }
  return step;
}

function readStep(elements) {
  const values = {};
  for (const [unit, key] of UNITS) values[unit] = elements[key]?.value;
  return stepFrom(values);
}

function writeStep(elements, step) {
  for (const [unit, key] of UNITS) {
    if (elements[key]) elements[key].value = String(step?.[unit] ?? 0);
  }
}

function updatePreview(elements) {
  const step = readStep(elements);
  const slug = String(elements.frequencySlug?.value || "").trim();
  if (!Object.keys(step).length) {
    elements.frequencyPreview.textContent = "Set at least one component.";
    return;
  }
  const label = cadenceLabel({ every: step, bound: { kind: "unbounded" } });
  const name = slug ? `freq(@${slug})` : "This frequency";
  elements.frequencyPreview.textContent = `${name} ${label}.`;
}

export function wireFrequencyForm(elements) {
  const toggle = () => {
    const hidden = elements.frequencyForm.hasAttribute("hidden");
    setHidden(elements.frequencyForm, !hidden);
    if (hidden) {
      elements.frequencyAnchor.value = dateTimeInputValue(null);
      updatePreview(elements);
      elements.frequencySlug.focus();
    }
  };
  elements.toggleFrequencyForm.addEventListener("click", toggle);
  elements.cancelFrequency.addEventListener("click", () =>
    setHidden(elements.frequencyForm, true),
  );

  // A preset only fills the components in. There is exactly one description of
  // a step — the boxes — rather than a mode that can disagree with them.
  elements.frequencyPreset.addEventListener("change", () => {
    const preset = PRESETS[elements.frequencyPreset.value];
    if (preset) writeStep(elements, preset);
    updatePreview(elements);
  });

  for (const [, key] of UNITS) {
    elements[key]?.addEventListener("input", () => {
      elements.frequencyPreset.value = "custom";
      updatePreview(elements);
    });
  }
  elements.frequencySlug.addEventListener("input", () => updatePreview(elements));

  elements.frequencyForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    const slug = String(elements.frequencySlug.value || "").trim().replace(/^@/, "");
    if (!slug) {
      setNotice("A frequency needs a name to be read by.");
      return;
    }
    const every = readStep(elements);
    if (!Object.keys(every).length) {
      setNotice("A frequency needs a component to repeat by.");
      return;
    }
    const result = await act({
      action: "create-frequency",
      request_id: requestId("frequency"),
      slug,
      every,
      anchor_at: elements.frequencyAnchor.value
        ? new Date(elements.frequencyAnchor.value).toISOString()
        : new Date().toISOString(),
    });
    if (result) {
      elements.frequencySlug.value = "";
      setHidden(elements.frequencyForm, true);
    }
  });
}

export function renderFrequencies(elements) {
  const frequencies = state.frequencies || [];
  replaceChildren(
    elements.frequencyList,
    frequencies.map((frequency) => {
      const item = el("li", { class: "ruleItem" });
      item.appendChild(el("strong", { text: frequency.head || frequency.slug }));
      item.appendChild(
        el("span", { class: "bankItemSource", text: `freq(@${frequency.slug})` }),
      );
      item.appendChild(
        el("span", {
          class: "frequencyBeat",
          text: cadenceLabel({
            every: frequency.every || {},
            bound: { kind: "unbounded" },
          }),
        }),
      );
      return item;
    }),
  );
  setHidden(elements.frequencyEmpty, frequencies.length > 0);
}
