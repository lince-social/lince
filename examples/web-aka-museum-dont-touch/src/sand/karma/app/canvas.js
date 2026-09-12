// The main view: a static deck of record cards, panned by the camera.
//
// A record earns a card by being named in a rule — either as the reading a
// condition tests (a `@slug` token inside its arithmetic) or as the target a
// consequence writes to. Cards never move on their own: their positions are a
// pure function of which records currently qualify. What moves is the
// camera, dragged and wheeled with d3-zoom exactly like the relations sand —
// minus its force simulation, because nothing here is connected to anything
// else that would need to settle.

import { state } from "./state.js";
import { setHidden } from "./format.js";

const d3 = typeof window !== "undefined" ? window.d3 : undefined;

const SLUG_TOKEN = /@([A-Za-z0-9_][\w.-]*)/g;

export const CARD_WIDTH = 220;
export const CARD_HEIGHT = 64;
export const CARD_GAP = 16;

/**
 * Every record uid a rule touches: its own target, plus whatever `@slug`
 * readings its condition names. `freq(@x)`, `value(@x)` and a bare `@x` all
 * point at a record the same way, so one token scan covers every reading.
 */
export function touchedRecordUids(records, rules) {
  const uidBySlug = new Map();
  for (const record of records || []) {
    if (record?.slug) uidBySlug.set(record.slug, record.uid);
  }
  const uids = new Set();
  for (const rule of rules || []) {
    if (rule?.record) uids.add(rule.record);
    const condition = String(rule?.condition || "");
    for (const match of condition.matchAll(SLUG_TOKEN)) {
      const uid = uidBySlug.get(match[1]);
      if (uid) uids.add(uid);
    }
  }
  return [...uids].sort();
}

/** One card per record, stacked — the second sits below the first, and so on. */
export function layoutStack(records) {
  return (records || []).map((record, index) => ({
    record,
    x: -CARD_WIDTH / 2,
    y: index * (CARD_HEIGHT + CARD_GAP),
    w: CARD_WIDTH,
    h: CARD_HEIGHT,
  }));
}

/** The touched records, resolved and laid out — what the canvas actually draws. */
export function cardsFor(records, rules) {
  const byUid = new Map((records || []).filter((r) => r?.uid).map((r) => [r.uid, r]));
  const resolved = touchedRecordUids(records, rules)
    .map((uid) => byUid.get(uid))
    .filter(Boolean);
  return layoutStack(resolved);
}

function formatQuantity(value) {
  const n = Number(value);
  if (!Number.isFinite(n)) return "0";
  return String(Math.round(n * 10000) / 10000);
}

function truncate(text, max) {
  const value = String(text ?? "");
  return value.length > max ? `${value.slice(0, max - 1)}…` : value;
}

function roundRect(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

// --- module-level drawing state --------------------------------------------
// A canvas has nothing to bind card state to, unlike a DOM node per card, so
// the last computed deck and camera transform live here between draws.

let canvas = null;
let ctx = null;
let zoomBehavior = null;
let transform = { x: 0, y: 0, k: 1 };
let cards = [];
let initialized = false;

function ensureCanvasSize() {
  const rect = canvas.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  const width = Math.max(1, Math.round(rect.width * dpr));
  const height = Math.max(1, Math.round(rect.height * dpr));
  if (canvas.width !== width || canvas.height !== height) {
    canvas.width = width;
    canvas.height = height;
  }
  return { cssWidth: rect.width, cssHeight: rect.height, dpr };
}

function drawCard(card) {
  const { x, y, w, h, record } = card;
  const k = transform.k || 1;
  // Everything but the stroke lives in world space and scales as one unit
  // with the card — only the hairline is held constant across zoom levels.
  ctx.fillStyle = "#182029";
  ctx.strokeStyle = "#2b3846";
  ctx.lineWidth = 1 / k;
  roundRect(ctx, x, y, w, h, 10);
  ctx.fill();
  ctx.stroke();

  ctx.textBaseline = "alphabetic";
  ctx.fillStyle = "#eef3f8";
  ctx.font = `600 14px "IBM Plex Sans", sans-serif`;
  ctx.fillText(truncate(record.head || record.slug || record.uid, 28), x + 14, y + 24);

  ctx.font = `600 17px "IBM Plex Sans", sans-serif`;
  ctx.fillStyle = "#8fe3aa";
  ctx.fillText(formatQuantity(record.quantity), x + 14, y + 45);

  ctx.font = `12px "IBM Plex Sans", sans-serif`;
  ctx.fillStyle = "#93a2b2";
  ctx.fillText(record.slug ? `@${record.slug}` : record.uid, x + 14, y + h - 10);
}

function draw() {
  if (!ctx || !canvas) return;
  const { dpr } = ensureCanvasSize();
  ctx.save();
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  ctx.scale(dpr, dpr);
  ctx.translate(transform.x, transform.y);
  ctx.scale(transform.k, transform.k);
  for (const card of cards) drawCard(card);
  ctx.restore();
}

function boundsOf(list) {
  if (!list.length) return null;
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const card of list) {
    minX = Math.min(minX, card.x);
    minY = Math.min(minY, card.y);
    maxX = Math.max(maxX, card.x + card.w);
    maxY = Math.max(maxY, card.y + card.h);
  }
  return { minX, minY, maxX, maxY };
}

/** Frame the deck once. After this, only the person's own drag/wheel moves it. */
function centerCamera() {
  const bounds = boundsOf(cards);
  if (!bounds) return;
  const rect = canvas.getBoundingClientRect();
  const cx = (bounds.minX + bounds.maxX) / 2;
  const cy = (bounds.minY + bounds.maxY) / 2;
  const next = { x: rect.width / 2 - cx, y: rect.height / 2 - cy, k: 1 };
  if (d3?.zoomIdentity && zoomBehavior) {
    const identity = d3.zoomIdentity.translate(next.x, next.y);
    d3.select(canvas).call(zoomBehavior.transform, identity);
  } else {
    transform = next;
    draw();
  }
}

/** Bind the canvas: size it, hand its camera to d3-zoom, wire the panel button. */
export function wireCanvas(elements) {
  canvas = elements.karmaCanvas || null;
  if (!canvas) return;
  ctx = canvas.getContext("2d");

  if (d3?.zoom) {
    zoomBehavior = d3
      .zoom()
      .scaleExtent([0.25, 3])
      .on("zoom", (event) => {
        transform = event.transform;
        draw();
      });
    d3.select(canvas).call(zoomBehavior);
  }

  if (typeof ResizeObserver !== "undefined") {
    new ResizeObserver(() => draw()).observe(canvas.parentElement || canvas);
  } else {
    window.addEventListener("resize", draw);
  }

  elements.openRulesPanel?.addEventListener("click", () => {
    elements.rulesPanel?.removeAttribute("hidden");
    // "+ Rule" means declaring one, not just seeing the panel that can — open
    // straight onto a fresh (never a half-finished edit) declare form, the
    // same button the panel offers once it's open.
    if (elements.recurrenceForm?.hasAttribute("hidden")) {
      elements.toggleRecurrenceForm?.click();
    }
  });
  elements.closeRulesPanel?.addEventListener("click", () => {
    elements.rulesPanel?.setAttribute("hidden", "");
  });
}

/** Recompute the deck from the latest snapshot and redraw. */
export function renderCanvas(elements) {
  if (!canvas) return;
  const wasEmpty = cards.length === 0;
  cards = cardsFor(state.records, state.rules);
  setHidden(elements.canvasEmpty, cards.length > 0);
  if (cards.length && (!initialized || wasEmpty)) {
    initialized = true;
    centerCamera();
  } else {
    draw();
  }
}
