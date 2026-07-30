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

const byId = (id) => document.getElementById(id);

const elements = {
  liveDot: byId("live-dot"),
  notice: byId("notice"),

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
  ruleRecord: byId("rule-record"),
  ruleAmount: byId("rule-amount"),
  ruleConcept: byId("rule-concept"),
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
  for (const select of [elements.captureRecord, elements.ruleRecord]) {
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
  renderEntries(elements);
  renderRules(elements);
  renderGraph(elements);
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

  wireCapture(elements);
  wireEntryFilter(elements, subscribeEntries);
  wireRecurrenceForm(elements);
  wireGraph(elements);

  onChange(render);
  subscribeAll();
  render();
}

start();
