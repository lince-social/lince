// Runs INSIDE every sand iframe. Exposes `window.LinceWidgetHost` — the sand's
// only door to the Cell. Reads are Protein subscriptions, writes are Actions;
// both are relayed to the board bridge (bridge.js) over postMessage, which
// multiplexes them onto the one transport WebSocket.
//
// A sand never sees SQL, tables, serverId or viewId — only Protein and Actions
// (blueprint VII.4). The control-plane methods (ABI emit/onEvent over ephemeral
// lanes, host metadata) sit beside the data plane, unchanged in spirit.
(function () {
  const instanceId =
    window.frameElement?.dataset?.linceInstanceId ||
    // The web board tags each package iframe with `data-package-instance-id`
    // (the card id); use it so this sand's subscriptions/lanes route per-card.
    window.frameElement?.dataset?.packageInstanceId ||
    `sand-${Math.random().toString(36).slice(2)}`;

  const parent = window.parent;
  const proteinHandlers = new Map(); // subId -> handler({rows})
  const actionWaiters = new Map(); // reqId -> {resolve, reject}
  const terminalSessions = new Map(); // sessionId -> callbacks + open waiter
  const scanWaiters = new Map(); // scanId -> {resolve, reject}
  const laneHandlers = new Set(); // {room, handler}
  const collabHandlers = new Map(); // recordUid -> Set<handler(snapshotBase64)>
  const collabAckHandlers = new Map(); // recordUid -> Set<handler(token)>
  const collabResetHandlers = new Map(); // recordUid -> Set<handler()>
  let live = false;
  const liveHandlers = new Set();
  // Which Organ this board is driving, or null for our own Cell. A sand that
  // does not check this will present someone else.s data as the user.s own.
  let liveOrgan = null;
  const liveOrganHandlers = new Set();
  let signingState = Object.freeze({
    status: "connecting",
    available: false,
    required: null,
    person: null,
    code: "session_challenge_pending",
    message: "Waiting for the Cell signing challenge.",
  });
  const signingStateHandlers = new Set();
  // Per-card host state (widgetState) — board chrome, never the Ledger. The
  // bridge pushes it down as `lince:bridge-state`; sands persist UI prefs
  // (e.g. kanban body modes) back up with patchCardState.
  let cardState = {};
  const cardStateHandlers = new Set();
  // The logged-in user's identity + permissions (null pre-login or when auth
  // isn't required) — a display hint for the sand's own UI (e.g. a delete
  // button's visibility). The engine, not this value, is what actually
  // enforces record:delete / record:delete_own.
  let viewer = null;
  const viewerHandlers = new Set();
  let reqSeq = 0;
  const nextReqId = () => `${instanceId}:a${++reqSeq}`;
  let terminalSeq = 0;
  let scanSeq = 0;

  function bytesToBase64(value) {
    const bytes = typeof value === "string"
      ? new TextEncoder().encode(value)
      : value instanceof Uint8Array
        ? value
        : ArrayBuffer.isView(value)
          ? new Uint8Array(value.buffer, value.byteOffset, value.byteLength)
          : value instanceof ArrayBuffer
            ? new Uint8Array(value)
            : new Uint8Array();
    let binary = "";
    for (let offset = 0; offset < bytes.length; offset += 0x8000) {
      binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
    }
    return window.btoa(binary);
  }

  function base64ToBytes(value) {
    const binary = window.atob(String(value || ""));
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index);
    }
    return bytes;
  }

  function terminalHandle(sessionId, info) {
    return {
      id: sessionId,
      shell: String(info?.shell || ""),
      cwd: String(info?.cwd || ""),
      write(data) {
        post({ type: "lince:terminal-input", sessionId, dataBase64: bytesToBase64(data) });
      },
      resize(geometry = {}) {
        post({ type: "lince:terminal-resize", sessionId, geometry });
      },
      close() {
        post({ type: "lince:terminal-close", sessionId });
      },
    };
  }

  function post(msg) {
    parent.postMessage({ instanceId, ...msg }, "*");
  }

  window.addEventListener("message", (event) => {
    const data = event.data;
    if (!data || typeof data !== "object") return;
    switch (data.type) {
      case "lince:protein-rows": {
        const handler = proteinHandlers.get(data.subId);
        if (handler) handler({ rows: data.rows || [] });
        break;
      }
      case "lince:protein-error":
        console.warn("[sand] protein error", data.subId, data.message);
        break;
      case "lince:action-result": {
        const waiter = actionWaiters.get(data.reqId);
        if (waiter) {
          actionWaiters.delete(data.reqId);
          if (data.ok) waiter.resolve({ created: data.created, facts: data.facts, warnings: data.warnings || [] });
          else {
            const error = new Error(data.message || "action failed");
            if (data.code) error.code = data.code;
            waiter.reject(error);
          }
        }
        break;
      }
      case "lince:terminal-opened": {
        const entry = terminalSessions.get(data.sessionId);
        if (entry && !entry.opened) {
          entry.opened = true;
          entry.resolve(terminalHandle(data.sessionId, data));
        }
        break;
      }
      case "lince:terminal-data": {
        const entry = terminalSessions.get(data.sessionId);
        if (entry) entry.onData(base64ToBytes(data.dataBase64));
        break;
      }
      case "lince:terminal-exit": {
        const entry = terminalSessions.get(data.sessionId);
        if (entry) {
          terminalSessions.delete(data.sessionId);
          entry.onExit(data.exitCode ?? null);
        }
        break;
      }
      case "lince:terminal-error": {
        const entry = terminalSessions.get(data.sessionId);
        if (entry) {
          if (!entry.opened) {
            terminalSessions.delete(data.sessionId);
            entry.reject(new Error(data.message || "terminal session failed"));
          } else {
            entry.onError(new Error(data.message || "terminal session failed"));
          }
        }
        break;
      }
      case "lince:scan-result": {
        const waiter = scanWaiters.get(data.scanId);
        if (waiter) {
          scanWaiters.delete(data.scanId);
          if (data.ok) waiter.resolve(data.text == null ? null : String(data.text));
          else waiter.reject(new Error(data.message || "scan failed"));
        }
        break;
      }
      case "lince:collab-state": {
        const handlers = collabHandlers.get(data.recordUid);
        if (handlers) for (const h of [...handlers]) h(data.snapshotBase64 || "");
        break;
      }
      case "lince:collab-ack": {
        const handlers = collabAckHandlers.get(data.recordUid);
        if (handlers) for (const h of [...handlers]) h(String(data.token || ""));
        break;
      }
      case "lince:collab-reset": {
        const handlers = collabResetHandlers.get(data.recordUid);
        if (handlers) for (const h of [...handlers]) h();
        break;
      }
      case "lince:lane-event":
        // Three arguments, and the third is the only one safe to show a human:
        // `from` is a connection id (an internal routing handle), `identity` is
        // the sender's subject and arrives ONLY when the host decided this
        // viewer may know it. A sand that renders `from` as a name is printing
        // an internal id at a person.
        for (const entry of laneHandlers)
          if (entry.room === data.room)
            entry.handler(data.payload, data.from, data.identity || null);
        break;
      case "lince:live":
        live = Boolean(data.live);
        for (const h of liveHandlers) h(live);
        break;
      case "lince:live-organ":
        liveOrgan = data.organ == null ? null : String(data.organ);
        for (const h of liveOrganHandlers) h(liveOrgan);
        break;
      case "lince:signing-state":
        signingState = Object.freeze({
          status: String(data.status || "unavailable"),
          available: Boolean(data.available),
          required: data.required == null ? null : Boolean(data.required),
          person: data.person == null ? null : String(data.person),
          code: String(data.code || ""),
          message: String(data.message || ""),
        });
        for (const h of signingStateHandlers) h(signingState);
        break;
      case "lince:bridge-state": {
        const next = data.payload?.meta?.cardState;
        if (next && typeof next === "object") {
          cardState = next;
          for (const h of cardStateHandlers) h(cardState);
        }
        const nextViewer = data.payload?.meta?.viewer;
        if (nextViewer !== undefined && nextViewer !== viewer) {
          viewer = nextViewer;
          for (const h of viewerHandlers) h(viewer);
        }
        break;
      }
    }
  });

  window.LinceWidgetHost = {
    instanceId,

    // READ: open a live Protein subscription. `handler({rows})` fires on the
    // first snapshot and on every recomputed update. Returns an unsubscribe fn.
    subscribeProtein(subId, protein, handler) {
      if (typeof handler !== "function") return () => {};
      proteinHandlers.set(subId, handler);
      post({ type: "lince:protein-subscribe", subId, protein });
      return () => {
        proteinHandlers.delete(subId);
        post({ type: "lince:protein-unsubscribe", subId });
      };
    },

    // READ: subscribe to a saved Protein by slug/uid (blueprint VII.1).
    subscribeSaved(subId, name, handler) {
      if (typeof handler !== "function") return () => {};
      proteinHandlers.set(subId, handler);
      post({ type: "lince:protein-subscribe-saved", subId, name });
      return () => {
        proteinHandlers.delete(subId);
        post({ type: "lince:protein-unsubscribe", subId });
      };
    },

    // WRITE: forward a typed Action; resolves { created, facts, warnings } or
    // rejects. Warnings are non-fatal advisories (cycles, Proof loops) — show
    // them, never treat them as errors.
    act(action) {
      const reqId = nextReqId();
      return new Promise((resolve, reject) => {
        actionWaiters.set(reqId, { resolve, reject });
        post({ type: "lince:action", reqId, action });
      });
    },

    // Ephemeral host capability: PTY bytes share the board's transport socket
    // but are not Protein rows, Actions, lanes, or persisted state.
    openTerminalSession(options = {}) {
      const sessionId = `t${++terminalSeq}`;
      const geometry = options.geometry || options;
      return new Promise((resolve, reject) => {
        terminalSessions.set(sessionId, {
          resolve,
          reject,
          opened: false,
          onData: typeof options.onData === "function" ? options.onData : () => {},
          onExit: typeof options.onExit === "function" ? options.onExit : () => {},
          onError: typeof options.onError === "function" ? options.onError : () => {},
        });
        post({ type: "lince:terminal-open", sessionId, geometry });
      });
    },

    // Ephemeral host capability: ask the CHROME to scan a QR code with the
    // camera. Resolves with the decoded text, or null if the user cancelled.
    //
    // The sand never receives pixels and never touches the camera. The chrome
    // owns the stream, shows the preview, and sends frames to the backend
    // decoder; only the decoded string crosses back. So a sand holding
    // `media_capture` can read a code the user deliberately pointed at — it
    // cannot watch the room.
    scanCode() {
      const scanId = `s${++scanSeq}`;
      return new Promise((resolve, reject) => {
        scanWaiters.set(scanId, { resolve, reject });
        post({ type: "lince:scan-code", scanId });
      });
    },

    // Control plane: sand-to-sand ABI over ephemeral lanes (never the Ledger).
    joinRoom(room) { post({ type: "lince:lane-join", room }); },
    emit(room, payload) { post({ type: "lince:lane-send", room, payload }); },
    onLane(room, handler) {
      const entry = { room, handler };
      laneHandlers.add(entry);
      return () => laneHandlers.delete(entry);
    },

    // Live collab (Ontology §11 "Collab"): join a record's shared Loro doc.
    // `handler(snapshotBase64)` fires with the server doc's snapshot on join
    // and again on every change (a sibling session typing, or a peer Organ
    // syncing in) — import the bytes into the sand's LoroDoc; imports dedupe
    // by version vector. Returns a leave fn. Send local edits with
    // `collabUpdate` (base64 Loro update bytes since the last send).
    collabJoin(recordUid, handler) {
      if (typeof handler !== "function") return () => {};
      let handlers = collabHandlers.get(recordUid);
      if (!handlers) {
        handlers = new Set();
        collabHandlers.set(recordUid, handlers);
      }
      handlers.add(handler);
      post({ type: "lince:collab-join", recordUid });
      return () => {
        handlers.delete(handler);
        if (handlers.size === 0) {
          collabHandlers.delete(recordUid);
          post({ type: "lince:collab-leave", recordUid });
        }
      };
    },
    // Send a local delta. `token` is echoed back through `onCollabAck` once the
    // Cell has merged and logged it — a sand that advances its "already sent"
    // version on send rather than on ack silently drops work when a socket
    // dies mid-flight.
    collabUpdate(recordUid, updateBase64, token) {
      post({
        type: "lince:collab-update",
        recordUid,
        updateBase64: String(updateBase64 || ""),
        token: String(token || ""),
      });
    },
    /** `handler(token)` when the Cell confirms that update landed. */
    onCollabAck(recordUid, handler) {
      if (typeof handler !== "function") return () => {};
      let handlers = collabAckHandlers.get(recordUid);
      if (!handlers) {
        handlers = new Set();
        collabAckHandlers.set(recordUid, handlers);
      }
      handlers.add(handler);
      return () => {
        handlers.delete(handler);
        if (handlers.size === 0) collabAckHandlers.delete(recordUid);
      };
    },
    /**
     * `handler()` when the connection was re-established and anything still
     * unacked must be re-exported. The rejoin snapshot only heals Cell -> sand.
     */
    onCollabReset(recordUid, handler) {
      if (typeof handler !== "function") return () => {};
      let handlers = collabResetHandlers.get(recordUid);
      if (!handlers) {
        handlers = new Set();
        collabResetHandlers.set(recordUid, handlers);
      }
      handlers.add(handler);
      return () => {
        handlers.delete(handler);
        if (handlers.size === 0) collabResetHandlers.delete(recordUid);
      };
    },

    onLive(handler) {
      liveHandlers.add(handler);
      handler(live);
      return () => liveHandlers.delete(handler);
    },

    // Live mode: drive another Organ.s Cell. Every subscription and Action
    // from every sand follows — you are working in their Cell, not merging
    // their data into yours. `null` comes home.
    enterLive(organUid) { post({ type: "lince:enter-live", organ: organUid || null }); },
    leaveLive() { post({ type: "lince:enter-live", organ: null }); },
    getLiveOrgan() { return liveOrgan; },
    onLiveOrgan(handler) {
      liveOrganHandlers.add(handler);
      handler(liveOrgan);
      return () => liveOrganHandlers.delete(handler);
    },

    // Authenticated writes are signed by the board host, outside the sand.
    // This is status only: no sand receives the CryptoKey or chooses the Person
    // bound by the server's authenticated session.
    getSigningState() { return signingState; },
    onSigningState(handler) {
      signingStateHandlers.add(handler);
      handler(signingState);
      return () => signingStateHandlers.delete(handler);
    },

    // Host state: the card's persisted UI prefs (host chrome, not sand data).
    getCardState() { return cardState; },
    onCardState(handler) {
      cardStateHandlers.add(handler);
      handler(cardState);
      return () => cardStateHandlers.delete(handler);
    },
    patchCardState(patch) { post({ type: "lince:patch-card-state", patch }); },

    // Host capability: ask the board chrome to export the CURRENT workspace as
    // one static, request-free HTML file (this sand's own card is excluded).
    // The chrome does the capture — a sand cannot read sibling iframes.
    archiveWorkspace(options = {}) {
      post({ type: "lince:archive-workspace", options: options || {} });
    },

    // The logged-in user (null pre-login / no-auth Cells) — display hint
    // only, see the `viewer` comment above.
    getViewer() { return viewer; },
    onViewer(handler) {
      viewerHandlers.add(handler);
      handler(viewer);
      return () => viewerHandlers.delete(handler);
    },
  };

  post({ type: "lince:ready" });
})();
