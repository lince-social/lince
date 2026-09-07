// Presentation only.
//
// Every amount that reaches this sand is exact decimal *text* computed by
// Protein. Nothing here parses one into a Number to add it, because that is
// exactly how "-10.10 + -20.20" becomes -30.299999999999997 and the ledger
// stops being exact. The only place a number appears is the graph geometry,
// where a pixel coordinate is approximate by definition and is never read back.

/** Sign of an exact decimal string, without arithmetic. */
export function signOf(text) {
  const value = String(text ?? "").trim();
  if (!value) return "zero";
  if (value.startsWith("-")) return /[1-9]/.test(value) ? "negative" : "zero";
  return /[1-9]/.test(value) ? "positive" : "zero";
}

/** An amount as written, with an explicit + so a gain reads as one. */
export function amountText(text) {
  const value = String(text ?? "").trim();
  if (!value) return "—";
  return signOf(value) === "positive" ? `+${value}` : value;
}

/** `2026-03` or `2026-03-14` as something a person reads. */
export function bucketLabel(bucket) {
  const value = String(bucket ?? "");
  const parts = value.split("-");
  if (parts.length < 2) return value;
  const months = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun",
    "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
  ];
  const month = months[Number(parts[1]) - 1] || parts[1];
  if (parts.length === 2) return `${month} ${parts[0]}`;
  return `${parts[2]} ${month} ${parts[0]}`;
}

/**
 * An RFC3339 instant as a short date.
 *
 * Rendered in UTC, deliberately. Captures are stored at midnight UTC (see
 * `instantFromDateInput`), so formatting in the viewer's zone would show a
 * 1 March rent as 28 February for everyone west of Greenwich — the date on
 * screen must match the date that was stored.
 */
export function dateLabel(value) {
  if (!value) return "";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return String(value);
  return parsed.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    timeZone: "UTC",
  });
}

/**
 * An RFC3339 instant as the `yyyy-mm-dd` an <input type="date"> wants.
 * UTC for the same reason as `dateLabel`: re-opening an entry for editing must
 * show the day it was actually stored on.
 */
export function dateInputValue(value) {
  const parsed = value ? new Date(value) : new Date();
  if (Number.isNaN(parsed.getTime())) return "";
  return parsed.toISOString().slice(0, 10);
}

/**
 * A `yyyy-mm-dd` from a date input as an RFC3339 instant.
 *
 * Midnight UTC on purpose: a captured change is dated, not timed, and letting
 * the browser's zone decide would put a 1st-of-the-month rent in the previous
 * month for anyone west of Greenwich.
 */
export function instantFromDateInput(value) {
  if (!value) return null;
  return `${value}T00:00:00Z`;
}

/**
 * A `<input type="datetime-local">` value as an RFC3339 instant.
 *
 * Read as UTC rather than as local time, matching `instantFromDateInput`. A
 * rule's anchor fixes the day of the month and the time of day that every
 * occurrence inherits, so letting the browser's zone shift it would move a
 * month-end rule into the following month for anyone east of Greenwich.
 */
export function instantFromDateTimeInput(value) {
  const civil = civilFromDateTimeInput(value);
  return civil === null ? null : `${civil}Z`;
}

/**
 * The same wall-clock reading, as the canonical civil string a cadence bound
 * wants: no zone marker, always three fractional digits.
 *
 * A bound is wall-clock for the same reason the step is — "stop before April"
 * means April where the person lives, and attaching a zone here would be the
 * one place the rule quietly disagreed with itself.
 */
export function civilFromDateTimeInput(value) {
  if (!value) return null;
  // The control omits seconds when they are zero, and milliseconds unless the
  // step asks for them.
  let text = String(value);
  if (text.length === 16) text = `${text}:00`;
  if (text.length === 19) text = `${text}.000`;
  return text;
}

/** An RFC3339 instant as the value a `datetime-local` input wants, in UTC. */
export function dateTimeInputValue(value) {
  const parsed = value ? new Date(value) : new Date();
  if (Number.isNaN(parsed.getTime())) return "";
  return parsed.toISOString().slice(0, 23);
}

/**
 * A date with its time, for rules that step faster than a day.
 *
 * A ten-millisecond rule whose dates all render as the same day would look
 * broken, so the precision shown follows the precision the instant carries.
 */
export function instantLabel(value) {
  if (!value) return "";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return String(value);
  const midnight =
    parsed.getUTCHours() === 0 &&
    parsed.getUTCMinutes() === 0 &&
    parsed.getUTCSeconds() === 0 &&
    parsed.getUTCMilliseconds() === 0;
  if (midnight) return dateLabel(value);
  const millis = parsed.getUTCMilliseconds();
  const time = [
    String(parsed.getUTCHours()).padStart(2, "0"),
    String(parsed.getUTCMinutes()).padStart(2, "0"),
    String(parsed.getUTCSeconds()).padStart(2, "0"),
  ].join(":");
  const fraction = millis ? `.${String(millis).padStart(3, "0")}` : "";
  return `${dateLabel(value)} ${time}${fraction}`;
}

/** Text for a concept uid, falling back to the uid when it has no name yet. */
export function conceptLabel(uid, names) {
  if (!uid) return "unclassified";
  return names.get(uid) || uid;
}

export function setHidden(element, hidden) {
  if (!element) return;
  if (hidden) element.setAttribute("hidden", "");
  else element.removeAttribute("hidden");
}

/** Replace an element's children with built nodes — never innerHTML. */
export function replaceChildren(parent, nodes) {
  if (!parent) return;
  parent.replaceChildren(...nodes);
}

export function el(tag, props = {}, children = []) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (value === null || value === undefined || value === false) continue;
    if (key === "class") node.className = String(value);
    else if (key === "text") node.textContent = String(value);
    else if (key.startsWith("on") && typeof value === "function") {
      node.addEventListener(key.slice(2).toLowerCase(), value);
    } else node.setAttribute(key, String(value));
  }
  for (const child of children) {
    if (child) node.appendChild(child);
  }
  return node;
}

/** A fresh idempotency key, so a retry cannot move a quantity twice. */
export function requestId(prefix) {
  const random = Math.random().toString(36).slice(2, 10);
  return `${prefix}-${Date.now().toString(36)}-${random}`;
}
