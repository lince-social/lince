// The blocks a condition is built from, and the typing that finds them.
//
// Everything here is a pure function of text, a caret offset, and a catalog.
// No DOM, no host, no state — which is what lets the whole autocomplete be
// tested by running it, rather than by clicking at it.
//
// A person types one of three openers and the editor absorbs the difference:
//
//   @app          → any block whose slug or head starts with "app"
//   record(app    → the same, narrowed to Records
//   freq(dai      → the same, narrowed to Frequencies
//
// `record(` is not a function in the grammar and never becomes one. It is an
// opener the editor understands and replaces outright with `@apple`, because
// the way a person reaches for a thing and the way the parser spells it are
// two different concerns.

/** What each kind of block spells like once it is accepted. */
export function completionFor(kind, slug) {
  if (kind === "frequency") return `freq(@${slug})`;
  return `@${slug}`;
}

const OPENERS = [
  { text: "record(", kind: "record" },
  { text: "freq(", kind: "frequency" },
  { text: "@", kind: null },
];

/** A partial slug: what may still be being typed after an opener. */
const PARTIAL = /^[A-Za-z0-9_.-]*$/;

/**
 * The query the caret currently sits inside, or null.
 *
 * Returns `{ start, end, kind, query }`, where `start`..`end` is the stretch of
 * source that accepting a block replaces, and `kind` narrows the catalog
 * (null means "any kind").
 */
export function activeQuery(source, caret) {
  const text = String(source ?? "");
  const at = Math.max(0, Math.min(Number(caret) || 0, text.length));
  const before = text.slice(0, at);

  let best = null;
  for (const opener of OPENERS) {
    const start = before.lastIndexOf(opener.text);
    if (start < 0) continue;
    const query = before.slice(start + opener.text.length);
    if (!PARTIAL.test(query)) continue;
    // The nearest opener to the caret is the one being typed into.
    if (!best || start > best.start) {
      best = { start, end: at, kind: opener.kind, query };
    }
  }
  if (!best) return null;

  // `freq(@dai` matches both `freq(` and `@`. The `@` wins on position, but
  // the block being named is a Frequency and accepting it has to produce the
  // whole `freq(@daily)` — closing paren included — so the opener behind the
  // `@` is what actually decides. Widen back onto it.
  if (best.kind === null) {
    for (const opener of OPENERS) {
      if (!opener.kind) continue;
      if (before.slice(0, best.start).endsWith(opener.text)) {
        return {
          start: best.start - opener.text.length,
          end: best.end,
          kind: opener.kind,
          query: best.query,
        };
      }
    }
  }
  return best;
}

/**
 * The catalog, filtered to `kind` and ranked against `query`.
 *
 * A block is findable by slug or by head, because a person remembers one or
 * the other and should not have to know which the system filed it under.
 * Something that *starts* with what was typed outranks something that merely
 * contains it — typing "app" should not offer "pineapple" above "apple".
 */
export function rankBlocks(catalog, kind, query, limit = 8) {
  const needle = String(query ?? "").trim().toLowerCase();
  const scored = [];
  for (const block of catalog || []) {
    if (!block?.slug) continue;
    if (kind && block.kind !== kind) continue;
    const slug = String(block.slug).toLowerCase();
    const head = String(block.head ?? "").toLowerCase();
    if (!needle) {
      scored.push({ block, score: 2 });
      continue;
    }
    if (slug.startsWith(needle) || head.startsWith(needle)) {
      scored.push({ block, score: 0 });
    } else if (slug.includes(needle) || head.includes(needle)) {
      scored.push({ block, score: 1 });
    }
  }
  scored.sort(
    (a, b) =>
      a.score - b.score ||
      String(a.block.head || a.block.slug).localeCompare(
        String(b.block.head || b.block.slug),
      ),
  );
  return scored.slice(0, limit).map((entry) => entry.block);
}

/**
 * Accept a block: the source with the query replaced, and where the caret goes.
 *
 * The caret lands after the inserted text — including past the closing paren of
 * a `freq(...)` — so typing can carry straight on into the next operator.
 */
export function applyCompletion(source, active, kind, slug) {
  const text = String(source ?? "");
  const inserted = completionFor(kind, slug);
  const start = active ? active.start : text.length;
  const end = active ? active.end : text.length;
  return {
    text: text.slice(0, start) + inserted + text.slice(end),
    caret: start + inserted.length,
  };
}

/**
 * Drop a block at the caret without a query to replace.
 *
 * This is what clicking a Record in the picker does. Separate from
 * `applyCompletion` because there is nothing being completed: the person did
 * not type a partial name, so nothing may be eaten.
 */
export function insertAtCaret(source, caret, kind, slug) {
  const text = String(source ?? "");
  const at = Math.max(0, Math.min(Number(caret) || 0, text.length));
  const inserted = completionFor(kind, slug);
  return { text: text.slice(0, at) + inserted + text.slice(at), caret: at + inserted.length };
}

const TOKEN = /freq\(\s*@([A-Za-z0-9_][\w.-]*)\s*\)|@([A-Za-z0-9_][\w.-]*)/g;

/**
 * Every block a condition names, in the order it names them.
 *
 * Drives the chip strip under the input. A slug nothing answers to is reported
 * as `kind: "unknown"` rather than dropped, because a typo that quietly shows
 * no chip looks exactly like a condition that reads nothing.
 */
export function blocksIn(source, catalog) {
  const known = new Map();
  for (const block of catalog || []) {
    if (block?.slug) known.set(`${block.kind}:${block.slug}`, block);
  }
  const found = [];
  for (const match of String(source ?? "").matchAll(TOKEN)) {
    const isFreq = match[1] !== undefined;
    const slug = isFreq ? match[1] : match[2];
    const kind = isFreq ? "frequency" : "record";
    const block = known.get(`${kind}:${slug}`);
    found.push({
      kind: block ? kind : "unknown",
      slug,
      head: block?.head || slug,
      source: match[0],
    });
  }
  return found;
}

/**
 * The catalog offered to the editor, from what Protein already knows.
 *
 * Frequencies come from `state.frequencies`, which stays empty until the
 * Frequency table exists — the section renders its empty state honestly rather
 * than dressing up rules-that-are-secretly-schedules as frequencies.
 */
export function catalogFrom(records, frequencies) {
  const blocks = [];
  for (const record of records || []) {
    if (!record?.slug) continue;
    blocks.push({
      kind: "record",
      slug: record.slug,
      head: record.head || record.slug,
      uid: record.uid,
      quantity: record.quantity,
    });
  }
  for (const frequency of frequencies || []) {
    if (!frequency?.slug) continue;
    blocks.push({
      kind: "frequency",
      slug: frequency.slug,
      head: frequency.head || frequency.slug,
      uid: frequency.uid,
    });
  }
  return blocks;
}
