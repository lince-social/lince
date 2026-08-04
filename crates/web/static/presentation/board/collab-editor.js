// The collaborative text element (Ontology §11 "Collab").
//
// One Record's `head`/`body` as a shared Loro document, wherever a surface
// wants to put it: the record editor, a kanban card's body, a relation node, a
// table cell. Those are the same question — "edit this Record's text with
// whoever else has it open" — so they are one implementation rather than four.
//
// Deliberately DOM-free. It takes a `surface` (read the text, write the text)
// and a `host` (the sand bridge), which is what lets a kanban card and a
// textarea share it, and what lets it be tested against the real wasm with no
// browser at all.
//
// The protocol is delta-based: each send carries only what changed since the
// last one. Sending a whole snapshot per keystroke also converges, which is
// exactly why it is worth being deliberate — it degrades with document size
// and looks correct until it is expensive.

/**
 * @param {object} options
 * @param {string} options.recordUid   Record whose doc this edits.
 * @param {object} options.host        The sand bridge (`LinceWidgetHost`).
 * @param {Function} options.LoroDoc   The LoroDoc constructor.
 * @param {object} options.surface     `{ read(): {head, body}, write({head, body}) }`.
 * @param {Function} [options.onError] Called with a message when a remote
 *                                     document cannot be read.
 */
export function createCollabEditor({ recordUid, host, LoroDoc, surface, onError }) {
  const doc = new LoroDoc();
  // The version already sent. Everything after it is what the next send
  // carries.
  let sentVersion = null;
  // Set while a remote change is being written into the surface, so the
  // surface's own change notification is not mistaken for the user typing —
  // which would echo the remote edit straight back out.
  let applying = false;
  let joined = null;

  function pushDelta() {
    const update = sentVersion
      ? doc.export({ mode: "update", from: sentVersion })
      : doc.export({ mode: "update" });
    sentVersion = doc.oplogVersion();
    if (update && update.length) {
      host.collabUpdate(recordUid, bytesToBase64(update));
    }
  }

  /** A merged document arrived from the Cell. */
  function applyRemote(snapshotBase64) {
    if (!snapshotBase64) return;
    applying = true;
    try {
      // Imports dedupe by version vector, so the server's echo of THIS
      // client's own work is a no-op rather than text typed twice.
      doc.import(base64ToBytes(snapshotBase64));
      surface.write({
        head: doc.getText("head").toString(),
        body: doc.getText("body").toString(),
      });
    } catch (error) {
      if (onError) onError("Could not read the shared document.");
    } finally {
      applying = false;
    }
  }

  /** The surface changed because a person typed into it. */
  function localEdit() {
    if (applying) return;
    const next = surface.read();
    // `update` lets Loro diff against what it already holds, so an ordinary
    // input stays an ordinary input: no keystroke bookkeeping, and a paste or
    // a select-all-replace is one update like any other.
    doc.getText("head").update(next.head || "");
    doc.getText("body").update(next.body || "");
    doc.commit();
    pushDelta();
  }

  function join() {
    joined = host.collabJoin(recordUid, applyRemote);
    return joined;
  }

  function destroy() {
    if (joined) joined();
    joined = null;
  }

  return {
    recordUid,
    join,
    destroy,
    applyRemote,
    localEdit,
    /** Current merged text, for a surface that renders rather than edits. */
    text: () => ({
      head: doc.getText("head").toString(),
      body: doc.getText("body").toString(),
    }),
  };
}

/**
 * Make one existing input or textarea collaborative.
 *
 * The whole embed story in one call: a kanban card body, a relation node's
 * text and a table cell are the same thing in different boxes, so each of them
 * is this line rather than its own copy of the delta and caret handling.
 *
 * Loads the vendored loro bundle on first use and caches it — several embeds
 * on one board must not each pull the wasm.
 *
 * Returns `{ detach }`, or `null` if collab is unavailable, which callers
 * should treat as "stay a plain textarea" rather than as an error.
 */
let loroPromise = null;
export async function attachCollab(el, { recordUid, host, field = "body", onError }) {
  if (!el || !recordUid || !host || typeof host.collabJoin !== "function") return null;
  try {
    loroPromise ??= (async () => {
      const loro = await import("/board/vendor/loro-index.js");
      await loro.default();
      return loro;
    })();
    const { LoroDoc } = await loroPromise;
    // Only the edited field is bound; the other keeps whatever the document
    // already holds, so an embed editing a body cannot blank a title.
    let other = "";
    const editor = createCollabEditor({
      recordUid,
      host,
      LoroDoc,
      onError,
      surface: {
        read: () =>
          field === "head" ? { head: el.value, body: other } : { head: other, body: el.value },
        write: ({ head, body }) => {
          const mine = field === "head" ? head : body;
          other = field === "head" ? body : head;
          if (el.value === mine) return;
          const at = el.selectionStart;
          el.value = mine;
          try {
            el.setSelectionRange(at, at);
          } catch (_) {}
        },
      },
    });
    const onInput = () => editor.localEdit();
    el.addEventListener("input", onInput);
    editor.join();
    return {
      detach() {
        el.removeEventListener("input", onInput);
        editor.destroy();
      },
    };
  } catch (error) {
    if (onError) onError("Live editing is unavailable here.");
    return null;
  }
}

export function bytesToBase64(bytes) {
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(binary);
}

export function base64ToBytes(value) {
  const binary = atob(String(value || ""));
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) out[i] = binary.charCodeAt(i);
  return out;
}

/**
 * Bind a collab editor to a pair of ordinary input elements.
 *
 * The caret handling is the whole reason this is shared rather than rewritten
 * per surface: replacing an input's value moves the caret to the end, so a
 * remote edit arriving mid-word would yank the cursor away from whoever is
 * typing. Every embed gets that right by construction.
 */
export function bindInputs(editor, { headEl, bodyEl }) {
  const surfaceWrite = ({ head, body }) => {
    for (const [el, value] of [
      [headEl, head],
      [bodyEl, body],
    ]) {
      if (!el || el.value === value) continue;
      const at = el.selectionStart;
      el.value = value;
      try {
        el.setSelectionRange(at, at);
      } catch (_) {
        // Not every input type supports a selection range; losing the caret
        // on those is better than throwing.
      }
    }
  };
  const onInput = () => editor.localEdit();
  if (headEl) headEl.addEventListener("input", onInput);
  if (bodyEl) bodyEl.addEventListener("input", onInput);
  return { surfaceWrite, detach: () => {
    if (headEl) headEl.removeEventListener("input", onInput);
    if (bodyEl) bodyEl.removeEventListener("input", onInput);
  } };
}
