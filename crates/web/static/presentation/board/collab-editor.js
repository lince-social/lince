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
//
// PRESENCE lives here too, rather than in each surface. It is the same
// question in every box — "who else is in this text, where is their selection,
// are they still there" — and putting it beside the document is what lets a
// kanban card and the record sand show the same cursors without either of them
// owning cursor logic. Presence rides ephemeral lanes and NEVER the Ledger: a
// caret position written to history would be both useless and permanent.

/** How often presence may go out. Cursors are chatty; the doc is not. */
const PRESENCE_THROTTLE_MS = 120;
/** No activity for this long and this editor reports itself idle. */
const IDLE_AFTER_MS = 30000;
/** A peer unheard-from for this long is gone: a closed tab sends no goodbye. */
const PEER_TTL_MS = 45000;
/** How often to sweep expired peers. */
const PEER_SWEEP_MS = 5000;

/**
 * @param {object} options
 * @param {string} options.recordUid   Record whose doc this edits.
 * @param {object} options.host        The sand bridge (`LinceWidgetHost`).
 * @param {Function} options.LoroDoc   The LoroDoc constructor.
 * @param {object} options.surface     `{ read(): {head, body}, write({head, body}) }`.
 * @param {string} [options.field]     Which field this surface edits, for
 *                                     presence. Two surfaces on ONE record
 *                                     share a lane room, so without this a
 *                                     kanban card body and a record title
 *                                     render each other's carets at offsets
 *                                     that mean nothing.
 * @param {Function} [options.onError] Called with a message when a remote
 *                                     document cannot be read.
 * @param {Function} [options.onPresence] Called with the peer list whenever it
 *                                     changes. Peers are
 *                                     `{key, name, field, anchor, focus, idle}`.
 * @param {boolean} [options.presence] Set false to skip lanes entirely.
 */
export function createCollabEditor({
  recordUid,
  host,
  LoroDoc,
  surface,
  field = "body",
  onError,
  onPresence,
  onSaveState,
  presence = true,
  sendDebounceMs = 200,
}) {
  const doc = new LoroDoc();
  // The version already SENT. Everything after it is what the next send
  // carries.
  let sentVersion = null;
  // The version the Cell has CONFIRMED. `sentVersion` is optimistic and may
  // run ahead of reality; this one never does, so it is what a reconnect
  // rewinds to. Keeping both is the whole fix: exporting from an optimistic
  // frontier permanently excludes any delta that never arrived.
  let ackedVersion = null;
  // In-flight sends in send order: `{token, version, acked}`. A list rather
  // than a map because the confirmed frontier may only move across a
  // CONTIGUOUS run of acked sends, which is an order question.
  const pending = [];
  let sendToken = 0;
  // Set while a remote change is being written into the surface, so the
  // surface's own change notification is not mistaken for the user typing —
  // which would echo the remote edit straight back out.
  let applying = false;
  let joined = null;
  let unAck = null;
  let unReset = null;
  let leaveRoom = null;
  let sweepTimer = null;
  let idleTimer = null;
  let presenceTimer = null;
  let sendTimer = null;
  let presencePending = null;
  let lastSentAt = 0;
  let idle = false;
  let selection = { field, anchor: 0, focus: 0 };
  const peers = new Map();
  const room = "record:" + recordUid;
  const meId = () => (host && host.instanceId) || "";
  // Presence needs lanes. A host without them (an embed that opted out, or a
  // headless test) still edits the document — it just has no cursors.
  const presenceOn =
    Boolean(presence) &&
    Boolean(host) &&
    typeof host.joinRoom === "function" &&
    typeof host.emit === "function" &&
    typeof host.onLane === "function";

  function pushDelta() {
    const update = sentVersion
      ? doc.export({ mode: "update", from: sentVersion })
      : doc.export({ mode: "update" });
    const at = doc.oplogVersion();
    if (!update || !update.length) {
      sentVersion = at;
      return;
    }
    sendToken += 1;
    const token = String(sendToken);
    pending.push({ token, version: at, acked: false });
    sentVersion = at;
    host.collabUpdate(recordUid, bytesToBase64(update), token);
    reportSaveState();
  }

  /// Tell the surface whether anything is still in flight, so it can render
  /// "saving…" or "saved" without inspecting the binding. `pending` is the
  /// whole truth: it empties only on a confirmed ack.
  function reportSaveState() {
    if (onSaveState) onSaveState({ pending: pending.length, saved: pending.length === 0 });
  }

  /** The Cell merged and logged the update carrying `token`. */
  function onAck(token) {
    const entry = pending.find((item) => item.token === token);
    if (!entry) return;
    entry.acked = true;
    // Advance only across a CONTIGUOUS run of confirmed sends. An update can
    // fail on its own while the socket stays up — a permission refusal answers
    // with an Error rather than an ack — and everything sent after it then
    // sits behind a hole the Cell does not have. Treating a later ack as
    // confirmation of the earlier one would move the frontier past work that
    // never landed, which is precisely the silent loss the ack exists to
    // prevent. So a hole stops the frontier and the gapped work is re-exported
    // on the next reset.
    while (pending.length && pending[0].acked) {
      ackedVersion = pending.shift().version;
    }
    reportSaveState();
  }

  /**
   * The socket came back. Anything unacked never reached the Cell, so rewind
   * the send frontier to the last confirmed version and re-export. Loro
   * dedupes by version vector, so re-sending something that DID land is a
   * no-op — the safe direction to be wrong in.
   */
  function resetUnacked() {
    pending.length = 0;
    reportSaveState();
    sentVersion = ackedVersion;
    pushDelta();
    announce(selection);
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
    scheduleSend();
    active();
  }

  /**
   * Batch a burst of typing into one delta.
   *
   * The document is committed immediately — local state is never delayed — but
   * the SEND waits out the debounce, so holding a key down is one op rather
   * than one per character. Every send path funnels through here so a flush on
   * teardown cannot be forgotten.
   */
  function scheduleSend() {
    if (sendDebounceMs <= 0) {
      pushDelta();
      return;
    }
    if (sendTimer) return;
    sendTimer = setTimeout(() => {
      sendTimer = null;
      pushDelta();
    }, sendDebounceMs);
  }

  /** Send anything the debounce is still holding. */
  function flush() {
    if (sendTimer) {
      clearTimeout(sendTimer);
      sendTimer = null;
    }
    pushDelta();
  }

  // ---- presence -------------------------------------------------------------

  function emitPresence(payload) {
    if (!presenceOn) return;
    host.emit(room, payload);
  }

  /** Send at most one presence frame per throttle window, keeping the last. */
  function schedulePresence(payload) {
    presencePending = payload;
    if (presenceTimer) return;
    const wait = Math.max(0, PRESENCE_THROTTLE_MS - (Date.now() - lastSentAt));
    presenceTimer = setTimeout(() => {
      presenceTimer = null;
      const next = presencePending;
      presencePending = null;
      if (!next) return;
      lastSentAt = Date.now();
      emitPresence(next);
    }, wait);
  }

  /**
   * Report where this editor's selection is. `anchor`/`focus` rather than a
   * single offset: a selection RANGE is what tells a collaborator "they are
   * about to replace this", which a bare caret cannot say.
   */
  function announce(next = {}) {
    if (!presenceOn) return;
    selection = {
      field: next.field || selection.field || field,
      anchor: Number(next.anchor) || 0,
      focus: Number(next.focus) || 0,
    };
    schedulePresence({
      id: meId(),
      field: selection.field,
      anchor: selection.anchor,
      focus: selection.focus,
      idle,
    });
  }

  /** Mark local activity: leaves idle, and restarts the idle countdown. */
  function active() {
    // No lanes, no idle bookkeeping. Beyond being pointless it would arm a
    // 30s timer in every context that merely edits a document — including the
    // headless test runner, which would then sit waiting on it to exit.
    if (!presenceOn) return;
    if (idle) {
      idle = false;
      announce(selection);
    }
    if (idleTimer) clearTimeout(idleTimer);
    idleTimer = setTimeout(() => {
      idle = true;
      // Idle goes out immediately rather than throttled — it is the one
      // presence change that happens when nothing else is happening.
      emitPresence({
        id: meId(),
        field: selection.field,
        anchor: selection.anchor,
        focus: selection.focus,
        idle: true,
      });
      notifyPresence();
    }, IDLE_AFTER_MS);
  }

  function peerList() {
    const out = [];
    for (const [key, peer] of peers) {
      if (key === meId()) continue;
      out.push({
        key,
        name: peer.name,
        field: peer.field,
        anchor: peer.anchor,
        focus: peer.focus,
        idle: peer.idle,
      });
    }
    return out;
  }

  function notifyPresence() {
    if (onPresence) onPresence(peerList());
  }

  function receivePresence(payload, from, identity) {
    if (!payload || typeof payload.anchor !== "number") return;
    const key = String(payload.id || from || "");
    if (!key || key === meId()) return;
    peers.set(key, {
      // `identity` is resolved by the HOST and only for a viewer allowed to
      // know who that is. `from` is a connection id and must never be shown:
      // it is an internal routing handle, not a name. Unnamed is a real state,
      // not a failure — the sand renders the caret without a name.
      name: identity || null,
      field: String(payload.field || "body"),
      anchor: Number(payload.anchor) || 0,
      focus: Number(payload.focus) || Number(payload.anchor) || 0,
      idle: Boolean(payload.idle),
      seen: Date.now(),
    });
    notifyPresence();
  }

  /** Drop peers that stopped announcing — a closed tab sends no goodbye. */
  function sweepPeers() {
    const cutoff = Date.now() - PEER_TTL_MS;
    let changed = false;
    for (const [key, peer] of peers) {
      if (peer.seen < cutoff) {
        peers.delete(key);
        changed = true;
      }
    }
    if (changed) notifyPresence();
  }

  function join() {
    joined = host.collabJoin(recordUid, applyRemote);
    if (typeof host.onCollabAck === "function") {
      unAck = host.onCollabAck(recordUid, onAck);
    }
    if (typeof host.onCollabReset === "function") {
      unReset = host.onCollabReset(recordUid, resetUnacked);
    }
    if (presenceOn) {
      host.joinRoom(room);
      leaveRoom = host.onLane(room, receivePresence);
      sweepTimer = setInterval(sweepPeers, PEER_SWEEP_MS);
      active();
      announce(selection);
    }
    return joined;
  }

  function destroy() {
    // Flush before leaving: a debounce holding the last few keystrokes when a
    // card closes would drop exactly the edit the user just finished.
    flush();
    if (joined) joined();
    if (unAck) unAck();
    if (unReset) unReset();
    if (leaveRoom) leaveRoom();
    if (sweepTimer) clearInterval(sweepTimer);
    if (idleTimer) clearTimeout(idleTimer);
    if (presenceTimer) clearTimeout(presenceTimer);
    if (sendTimer) clearTimeout(sendTimer);
    joined = unAck = unReset = leaveRoom = null;
    sweepTimer = idleTimer = presenceTimer = null;
    peers.clear();
  }

  return {
    recordUid,
    field,
    join,
    destroy,
    applyRemote,
    localEdit,
    announce,
    active,
    flush,
    peers: peerList,
    /** Test/diagnostic seam: is anything still waiting on the Cell? */
    unacked: () => pending.length,
    saveState: () => ({ pending: pending.length, saved: pending.length === 0 }),
    onAck,
    resetUnacked,
    /** Current merged text, for a surface that renders rather than edits. */
    text: () => ({
      head: doc.getText("head").toString(),
      body: doc.getText("body").toString(),
    }),
  };
}

/**
 * Wire an input/textarea's selection to an editor's presence.
 *
 * Every surface needs exactly this and there is nothing surface-specific in
 * it, which is why it lives here rather than being retyped per sand.
 */
export function bindPresence(editor, el, field) {
  if (!editor || !el) return { detach() {} };
  const report = () => {
    editor.announce({
      field: field || editor.field,
      anchor: el.selectionStart || 0,
      focus: el.selectionEnd || 0,
    });
  };
  const events = ["keyup", "click", "select", "focus", "input"];
  for (const name of events) el.addEventListener(name, report);
  // `selectionchange` is the only event that catches a selection changed by
  // keyboard-held drag or by the browser itself; it fires on the document, so
  // it is filtered down to this element.
  const onDocSelection = () => {
    if (document.activeElement === el) report();
  };
  document.addEventListener("selectionchange", onDocSelection);
  report();
  return {
    detach() {
      for (const name of events) el.removeEventListener(name, report);
      document.removeEventListener("selectionchange", onDocSelection);
    },
  };
}

/**
 * Make one existing input or textarea collaborative.
 *
 * The whole embed story in one call: a kanban card body, a relation node's
 * text and a table cell are the same thing in different boxes, so each of them
 * is this line rather than its own copy of the delta, caret and presence
 * handling.
 *
 * Loads the vendored loro bundle on first use and caches it — several embeds
 * on one board must not each pull the wasm.
 *
 * Returns `{ detach, editor }`, or `null` if collab is unavailable, which
 * callers should treat as "stay a plain textarea" rather than as an error.
 */
let loroPromise = null;
export async function attachCollab(
  el,
  { recordUid, host, field = "body", onError, onPresence, onSaveState },
) {
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
      onPresence,
      onSaveState,
      field,
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
    const presence = bindPresence(editor, el, field);
    return {
      editor,
      detach() {
        el.removeEventListener("input", onInput);
        presence.detach();
        editor.destroy();
      },
    };
  } catch (error) {
    if (onError) onError("Live editing is unavailable here.");
    return null;
  }
}

/**
 * Bind ANY of a Record's editable fields to an input, text or not.
 *
 * `path` is either a text container (`head` / `body`) or a map key
 * (`<namespace>.<key>`), and the two take deliberately different routes:
 *
 * - **Text** goes through the record-doc: character-level CRDT merge, because
 *   two people typing into one string is a real conflict with a real answer.
 * - **A map key** goes through the ordinary extension write, which is ALREADY
 *   a per-key LWW op carrying its own HLC (`sync_op` on `record_extension`,
 *   merged by `latest_hlc_for_field` on import). That is the defined merge
 *   semantics for these fields.
 *
 * Putting a map key into the Loro doc instead was considered and rejected: it
 * would create a SECOND source of truth for the same cell, and the only thing
 * two authorities over one value can do is disagree. Nothing is lost — LWW is
 * what a scalar wants; there is no character-wise merge of the number 5.
 *
 * What the surface gets is one call and one shape either way, which is the
 * point: a table cell should not have to know which kind of field it holds.
 *
 * Returns `{ detach, kind }` where `kind` is `"crdt"` or `"lww"`, or `null`
 * when nothing could be bound.
 */
export async function attachField(
  el,
  { recordUid, host, path = "body", onError, onPresence, onSaveState, debounceMs = 400 },
) {
  if (!el || !recordUid || !host) return null;
  if (path === "head" || path === "body") {
    const live = await attachCollab(el, {
      recordUid,
      host,
      field: path,
      onError,
      onPresence,
      onSaveState,
    });
    return live ? { ...live, kind: "crdt" } : null;
  }

  const dot = path.indexOf(".");
  if (dot <= 0 || dot === path.length - 1) {
    if (onError) onError(`"${path}" is not a field this can bind.`);
    return null;
  }
  // A key never contains a dot, so the LAST one splits namespace from key —
  // namespaces are dotted (`work.tracking.estimate`), keys are not.
  const cut = path.lastIndexOf(".");
  const namespace = path.slice(0, cut);
  const key = path.slice(cut + 1);

  let timer = null;
  const commit = () => {
    timer = null;
    // Only this key travels. Writing the whole namespace would clobber
    // sibling keys another Cell edited concurrently — the per-KEY op exists
    // precisely so two Cells editing one namespace never collide.
    Promise.resolve(
      host.act({
        action: "set-extension",
        target: recordUid,
        namespace,
        fds: { [key]: el.value },
      }),
    ).catch(() => {
      if (onError) onError("Could not save that value.");
    });
  };
  const onInput = () => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(commit, debounceMs);
  };
  el.addEventListener("input", onInput);
  return {
    kind: "lww",
    detach() {
      el.removeEventListener("input", onInput);
      // Flush rather than drop: a cell closed mid-edit must not lose the edit.
      if (timer) {
        clearTimeout(timer);
        commit();
      }
    },
  };
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
  // Presence follows the caret into whichever of the two fields holds it, so
  // one editor over a title and a body reports the right field either way.
  const presences = [];
  if (headEl) presences.push(bindPresence(editor, headEl, "head"));
  if (bodyEl) presences.push(bindPresence(editor, bodyEl, "body"));
  return {
    surfaceWrite,
    detach: () => {
      if (headEl) headEl.removeEventListener("input", onInput);
      if (bodyEl) bodyEl.removeEventListener("input", onInput);
      for (const p of presences) p.detach();
    },
  };
}
