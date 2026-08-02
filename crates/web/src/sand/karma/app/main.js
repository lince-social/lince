// Karma sand entry point.
//
// Wiring only: find the elements, open the subscriptions, and re-render when a
// snapshot arrives. Every total, bucket and graph point on this screen was
// computed by Protein; this file moves them into the DOM and does no arithmetic.

import {
  dateInputValue,
  dateTimeInputValue,
  el,
  replaceChildren,
  setHidden,
} from "./format.js";
import {
  hasHost,
  onChange,
  setNotice,
  state,
  subscribeAll,
  subscribeEntries,
} from "./state.js";
import { renderEntries, wireCapture, wireEntryFilter } from "./entries.js";
import { renderRules, wireRecurrenceForm } from "./recurrence.js";
import { renderGraph, wireGraph } from "./graph.js";
import { renderCanvas, wireCanvas } from "./canvas.js";
import { renderBuilder, wireBuilder } from "./builder.js";
import { renderFrequencies, wireFrequencyForm } from "./frequency.js";

const byId = (id) => document.getElementById(id);

const elements = {
  liveDot: byId("live-dot"),
  notice: byId("notice"),

  karmaCanvas: byId("karma-canvas"),
  canvasEmpty: byId("canvas-empty"),
  openRulesPanel: byId("open-rules-panel"),
  closeRulesPanel: byId("close-rules-panel"),
  rulesPanel: byId("rules-panel"),

  // --- rule builder
  ruleBuilderForm: byId("rule-builder-form"),
  conditionInput: byId("condition-input"),
  conditionSuggest: byId("condition-suggest"),
  conditionChips: byId("condition-chips"),
  conditionError: byId("condition-error"),
  conditionBank: byId("condition-bank"),
  conditionBankEmpty: byId("condition-bank-empty"),
  conditionBankToggle: byId("condition-bank-toggle"),
  builderGate: byId("builder-gate"),
  builderGateValue: byId("builder-gate-value"),
  builderGateValueField: byId("builder-gate-value-field"),
  builderTarget: byId("builder-target"),
  builderConsequence: byId("builder-consequence"),
  builderAmount: byId("builder-amount"),
  builderAmountField: byId("builder-amount-field"),
  builderConcept: byId("builder-concept"),
  builderConceptField: byId("builder-concept-field"),
  builderCommand: byId("builder-command"),
  builderCommandField: byId("builder-command-field"),
  consequenceBank: byId("consequence-bank"),
  consequenceBankEmpty: byId("consequence-bank-empty"),
  consequenceBankToggle: byId("consequence-bank-toggle"),
  recordSearch: byId("record-search"),
  recordResults: byId("record-results"),
  recordResultsEmpty: byId("record-results-empty"),
  builderSubmit: byId("builder-submit"),
  builderReset: byId("builder-reset"),

  // --- frequencies
  toggleFrequencyForm: byId("toggle-frequency-form"),
  frequencyForm: byId("frequency-form"),
  cancelFrequency: byId("cancel-frequency"),
  frequencySlug: byId("frequency-slug"),
  frequencyPreset: byId("frequency-preset"),
  freqYears: byId("freq-years"),
  freqMonths: byId("freq-months"),
  freqWeeks: byId("freq-weeks"),
  freqDays: byId("freq-days"),
  freqHours: byId("freq-hours"),
  freqMinutes: byId("freq-minutes"),
  freqSeconds: byId("freq-seconds"),
  freqMilliseconds: byId("freq-milliseconds"),
  frequencyAnchor: byId("frequency-anchor"),
  frequencyPreview: byId("frequency-preview"),
  frequencyList: byId("frequency-list"),
  frequencyEmpty: byId("frequency-empty"),

  captureForm: byId("capture-form"),
  captureRecord: byId("capture-record"),
  captureAmount: byId("capture-amount"),
  captureConcept: byId("capture-concept"),
  captureAt: byId("capture-at"),
  captureNote: byId("capture-note"),
  conceptOptions: byId("concept-options"),

  entriesFilter: byId("entries-filter"),
  entryList: byId("entry-list"),
  entriesEmpty: byId("entries-empty"),

  toggleRecurrenceForm: byId("toggle-recurrence-form"),
  recurrenceForm: byId("recurrence-form"),
  cancelRecurrence: byId("cancel-recurrence"),
  recurrenceSubmit: byId("recurrence-submit"),
  ruleRecord: byId("rule-record"),
  ruleCondition: byId("rule-condition"),
  ruleGate: byId("rule-gate"),
  ruleGateField: byId("rule-gate-field"),
  ruleGateValue: byId("rule-gate-value"),
  ruleGateValueField: byId("rule-gate-value-field"),
  ruleCarry: byId("rule-carry"),
  ruleCarryField: byId("rule-carry-field"),
  ruleCarryValue: byId("rule-carry-value"),
  ruleCarryValueField: byId("rule-carry-value-field"),
  ruleNumberAction: byId("rule-number-action"),
  ruleAmount: byId("rule-amount"),
  ruleAmountField: byId("rule-amount-field"),
  ruleConcept: byId("rule-concept"),
  ruleConceptField: byId("rule-concept-field"),
  ruleConceptAction: byId("rule-concept-action"),
  ruleConceptFrom: byId("rule-concept-from"),
  ruleConceptFromField: byId("rule-concept-from-field"),
  ruleConceptTo: byId("rule-concept-to"),
  ruleConceptToField: byId("rule-concept-to-field"),
  rulePreset: byId("rule-preset"),
  stepYears: byId("step-years"),
  stepMonths: byId("step-months"),
  stepWeeks: byId("step-weeks"),
  stepDays: byId("step-days"),
  stepHours: byId("step-hours"),
  stepMinutes: byId("step-minutes"),
  stepSeconds: byId("step-seconds"),
  stepMilliseconds: byId("step-milliseconds"),
  landOnBoxes: document.querySelectorAll('input[name="land-on"]'),
  ruleInvalidDay: byId("rule-invalid-day"),
  ruleInvalidDayField: byId("rule-invalid-day-field"),
  rulePreview: byId("rule-preview"),
  ruleAnchor: byId("rule-anchor"),
  ruleBound: byId("rule-bound"),
  ruleBoundField: byId("rule-bound-field"),
  ruleBoundCount: byId("rule-bound-count"),
  ruleBoundCountField: byId("rule-bound-count-field"),
  ruleBoundUntil: byId("rule-bound-until"),
  ruleBoundUntilField: byId("rule-bound-until-field"),
  ruleNote: byId("rule-note"),
  recurrenceList: byId("recurrence-list"),
  recurrenceEmpty: byId("recurrence-empty"),
  occurrenceList: byId("occurrence-list"),
  occurrenceEmpty: byId("occurrence-empty"),
  occurrenceMore: byId("occurrence-more"),

  graphConcept: byId("graph-concept"),
  graphWindow: byId("graph-window"),
  stateCurrent: byId("state-current"),
  stateOpening: byId("state-opening"),
  stateExpected: byId("state-expected"),
  timelineGraph: byId("timeline-graph"),
  timelineTable: byId("timeline-table"),
  contributorList: byId("contributor-list"),
  contributorEmpty: byId("contributor-empty"),
};

/** Keep the two resource pickers in step with what actually exists. */
function renderRecordPickers() {
  for (const select of [
    elements.captureRecord,
    elements.ruleRecord,
    elements.builderTarget,
  ]) {
    const chosen = select.value;
    replaceChildren(
      select,
      state.records.map((record) =>
        el("option", {
          value: record.uid,
          text: record.head || record.slug || record.uid,
        }),
      ),
    );
    if (chosen && state.records.some((r) => r.uid === chosen)) select.value = chosen;
  }
}

function renderConceptOptions() {
  replaceChildren(
    elements.conceptOptions,
    state.concepts
      .filter((concept) => concept?.name || concept?.canonical_name)
      .map((concept) =>
        el("option", { value: concept.name || concept.canonical_name }),
      ),
  );
}

function renderNotice() {
  const notice = state.notice;
  setHidden(elements.notice, !notice);
  if (notice) {
    elements.notice.textContent = notice.message;
    elements.notice.setAttribute("data-tone", notice.tone);
  }
}

function render() {
  elements.liveDot.setAttribute("data-live", state.live ? "true" : "false");
  renderNotice();
  renderRecordPickers();
  renderConceptOptions();
  renderBuilder(elements);
  renderFrequencies(elements);
  renderEntries(elements);
  renderRules(elements);
  renderGraph(elements);
  renderCanvas(elements);
}

function start() {
  // Dating a capture today is the overwhelmingly common case; backdating stays
  // one field away rather than being the default nobody fills in.
  elements.captureAt.value = dateInputValue(null);
  // The anchor carries a time as well as a date now, because a rule stepping in
  // seconds is phased by the time of day it started at.
  elements.ruleAnchor.value = dateTimeInputValue(null);

  if (!hasHost()) {
    setNotice("This host cannot subscribe to Protein or submit Actions.");
    render();
    return;
  }

  wireBuilder(elements);
  wireFrequencyForm(elements);
  wireCapture(elements);
  wireEntryFilter(elements, subscribeEntries);
  wireRecurrenceForm(elements);
  wireGraph(elements);
  wireCanvas(elements);

  onChange(render);
  subscribeAll();
  render();
}

start();
