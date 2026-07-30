// One concept drawn through time: settled past, the position now, and the
// declared future.
//
// Every number here arrives already computed by the `timeline` Protein source,
// including the running cumulative. This module converts exact decimal *text*
// into pixel coordinates and nothing else — a coordinate is approximate by
// definition and is never read back into a total, which is what keeps the
// client's floating point away from the ledger's exactness.
//
// The chart is decorative. The table beneath it carries the same points and is
// what a screen reader and a keyboard actually use, so no number here exists
// only as a shape.

import {
  amountText,
  bucketLabel,
  dateLabel,
  el,
  replaceChildren,
  setHidden,
  signOf,
} from "./format.js";
import { state, subscribeTimeline, conceptToken } from "./state.js";

const SVG_NS = "http://www.w3.org/2000/svg";
const WIDTH = 720;
const HEIGHT = 220;
const PAD_X = 46;
const PAD_Y = 18;

export function wireGraph(elements) {
  elements.graphConcept.addEventListener("change", () => {
    state.filters.graphConcept = conceptToken(elements.graphConcept.value) || "";
    subscribeTimeline();
  });
  elements.graphWindow.addEventListener("change", () => {
    state.filters.graphMonths = Number(elements.graphWindow.value) || 6;
    subscribeTimeline();
  });
}

export function renderGraph(elements) {
  const timeline = state.timeline;
  const points = timeline?.points ?? [];
  const context = timeline?.context ?? null;

  // `current` is null when the concept spans more than one unit, because there
  // is no single number to show — adding kilograms to hours is exactly
  // what the backend refuses to do, and the headline must not do it either.
  elements.stateCurrent.textContent = headline(context, "current", "current_by_unit");
  elements.stateCurrent.setAttribute(
    "data-sign",
    context?.current ? signOf(context.current) : "zero",
  );
  elements.stateOpening.textContent = headline(context, "opening", "opening_by_unit");
  elements.stateExpected.textContent = declaredAheadText(points);

  renderChart(elements.timelineGraph, points, context);
  renderTable(elements.timelineTable, points);
  renderContributors(elements, timeline?.sources ?? []);
}

/**
 * A single figure when the concept has one unit, or one per unit when it does
 * not. Never a sum across units.
 */
function headline(context, scalarKey, mapKey) {
  if (!context) return "—";
  const scalar = context[scalarKey];
  if (scalar !== null && scalar !== undefined) return amountText(scalar);
  const byUnit = context[mapKey] || {};
  const parts = Object.entries(byUnit).map(
    ([unit, value]) => `${amountText(value)} ${unit || "?"}`,
  );
  return parts.length ? parts.join(" · ") : "—";
}

/**
 * How much is declared ahead, counted by *listing* rather than adding.
 *
 * The sand must not sum exact decimals, so this reports the count of declared
 * points instead of a total the backend did not compute. Saying "4 dates
 * declared" is honest; inventing a sum here would not be.
 */
function declaredAheadText(points) {
  const declared = points.filter((p) => p.expected_net !== null && p.expected_net !== undefined);
  if (!declared.length) return "none";
  const dates = declared.reduce((sum, p) => sum + (Number(p.expected_count) || 0), 0);
  return `${dates} on ${declared.length} ${declared.length === 1 ? "period" : "periods"}`;
}

/** Exact decimal text to a float, for geometry only. Never fed back. */
function coordinateValue(text) {
  const parsed = Number.parseFloat(String(text ?? "0"));
  return Number.isFinite(parsed) ? parsed : 0;
}

function svg(tag, attrs = {}) {
  const node = document.createElementNS(SVG_NS, tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (value === null || value === undefined) continue;
    node.setAttribute(key, String(value));
  }
  return node;
}

function renderChart(host, points, context) {
  if (!host) return;
  host.replaceChildren();
  if (!context) {
    host.appendChild(
      el("p", { class: "empty", text: "Pick a concept to chart its past and declared future." }),
    );
    return;
  }
  if (!points.length) {
    host.appendChild(
      el("p", { class: "empty", text: "Nothing recorded or declared for this concept yet." }),
    );
    return;
  }

  const values = points.map((p) => coordinateValue(p.cumulative));
  let min = Math.min(0, ...values);
  let max = Math.max(0, ...values);
  if (min === max) {
    // A flat line still deserves a band rather than a divide-by-zero.
    min -= 1;
    max += 1;
  }
  const span = max - min;
  const stepX = points.length > 1 ? (WIDTH - PAD_X * 2) / (points.length - 1) : 0;
  const xOf = (index) => PAD_X + stepX * index;
  const yOf = (value) => HEIGHT - PAD_Y - ((value - min) / span) * (HEIGHT - PAD_Y * 2);

  const root = svg("svg", {
    viewBox: `0 0 ${WIDTH} ${HEIGHT}`,
    preserveAspectRatio: "xMidYMid meet",
    role: "presentation",
    focusable: "false",
  });

  // Zero is the line that matters on a signed axis — above it is a surplus,
  // below it a hole.
  if (min < 0 && max > 0) {
    root.appendChild(
      svg("line", {
        class: "zeroLine",
        x1: PAD_X,
        x2: WIDTH - PAD_X,
        y1: yOf(0),
        y2: yOf(0),
      }),
    );
  }

  // Split at the present: what settled is solid, what is declared is dashed,
  // and the two must never look like the same claim.
  const firstFuture = points.findIndex((p) => p.phase === "expected");
  const splitAt = firstFuture === -1 ? points.length - 1 : Math.max(0, firstFuture - 1);

  const pastPoints = points.slice(0, splitAt + 1);
  const futurePoints = points.slice(splitAt);
  if (pastPoints.length > 1) {
    root.appendChild(
      svg("polyline", {
        class: "linePast",
        points: pastPoints
          .map((p, i) => `${xOf(i)},${yOf(coordinateValue(p.cumulative))}`)
          .join(" "),
      }),
    );
  }
  if (futurePoints.length > 1) {
    root.appendChild(
      svg("polyline", {
        class: "lineFuture",
        points: futurePoints
          .map((p, i) => `${xOf(splitAt + i)},${yOf(coordinateValue(p.cumulative))}`)
          .join(" "),
      }),
    );
  }

  points.forEach((point, index) => {
    const dot = svg("circle", {
      class: "point",
      "data-phase": point.phase,
      cx: xOf(index),
      cy: yOf(coordinateValue(point.cumulative)),
      r: 3,
    });
    const title = svg("title");
    title.textContent = `${bucketLabel(point.bucket)}: ${amountText(point.cumulative)}`;
    dot.appendChild(title);
    root.appendChild(dot);
  });

  // Only the ends and the middle are labelled; a dense axis is unreadable at
  // this size and the table below carries every period anyway.
  const labelIndexes = new Set([0, points.length - 1, Math.floor((points.length - 1) / 2)]);
  for (const index of labelIndexes) {
    const point = points[index];
    if (!point) continue;
    const text = svg("text", {
      class: "axisText",
      x: xOf(index),
      y: HEIGHT - 4,
      "text-anchor": index === 0 ? "start" : index === points.length - 1 ? "end" : "middle",
    });
    text.textContent = bucketLabel(point.bucket);
    root.appendChild(text);
  }

  for (const [value, anchor] of [[max, "hanging"], [min, "auto"]]) {
    const text = svg("text", {
      class: "axisText",
      x: 4,
      y: value === max ? PAD_Y : HEIGHT - PAD_Y,
      "dominant-baseline": anchor,
    });
    text.textContent = String(value);
    root.appendChild(text);
  }

  host.appendChild(root);
  host.appendChild(
    el("div", { class: "legend" }, [
      el("span", { class: "legendKey" }, [
        el("span", { class: "legendSwatch" }),
        el("span", { text: "settled" }),
      ]),
      el("span", { class: "legendKey" }, [
        el("span", { class: "legendSwatch", "data-kind": "expected" }),
        el("span", { text: "declared ahead" }),
      ]),
    ]),
  );
  host.setAttribute(
    "aria-label",
    `Running total for ${context.concept}, ${points.length} periods, currently ${context.current}.`,
  );
}

function renderTable(table, points) {
  const body = table?.querySelector("tbody");
  if (!body) return;
  replaceChildren(
    body,
    points.map((point) =>
      el("tr", { "data-phase": point.phase }, [
        el("td", { text: bucketLabel(point.bucket) }),
        el("td", { text: point.actual_net ? amountText(point.actual_net) : "—" }),
        el("td", { text: point.expected_net ? amountText(point.expected_net) : "—" }),
        el("td", { text: amountText(point.cumulative) }),
      ]),
    ),
  );
}

/**
 * Every declared point drills to the rule or promise that produced it, so a
 * number on a chart is never one nobody can explain.
 */
function renderContributors(elements, sources) {
  setHidden(elements.contributorEmpty, sources.length > 0);
  replaceChildren(
    elements.contributorList,
    sources.slice(0, 60).map((source) =>
      el("li", {}, [
        el("div", { class: "rowTop" }, [
          el("span", {
            class: "amount",
            "data-sign": signOf(source.amount),
            text: amountText(source.amount),
          }),
          el("span", { class: "state", "data-state": source.state, text: source.state }),
        ]),
        el("div", { class: "rowMeta" }, [
          el("span", { text: dateLabel(source.at) }),
          el("span", {
            class: "tag",
            text: source.origin === "recurrence" ? "recurring rule" : "promise",
          }),
          source.note ? el("span", { text: source.note }) : null,
        ]),
      ]),
    ),
  );
}
