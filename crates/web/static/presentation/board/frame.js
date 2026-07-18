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
  const laneHandlers = new Set(); // {room, handler}
  let live = false;
  const liveHandlers = new Set();
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
          else waiter.reject(new Error(data.message || "action failed"));
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
      case "lince:lane-event":
        for (const entry of laneHandlers) if (entry.room === data.room) entry.handler(data.payload, data.from);
        break;
      case "lince:live":
        live = Boolean(data.live);
        for (const h of liveHandlers) h(live);
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

    // Control plane: sand-to-sand ABI over ephemeral lanes (never the Ledger).
    joinRoom(room) { post({ type: "lince:lane-join", room }); },
    emit(room, payload) { post({ type: "lince:lane-send", room, payload }); },
    onLane(room, handler) {
      const entry = { room, handler };
      laneHandlers.add(entry);
      return () => laneHandlers.delete(entry);
    },

    onLive(handler) {
      liveHandlers.add(handler);
      handler(live);
      return () => liveHandlers.delete(handler);
    },

    // Host state: the card's persisted UI prefs (host chrome, not sand data).
    getCardState() { return cardState; },
    onCardState(handler) {
      cardStateHandlers.add(handler);
      handler(cardState);
      return () => cardStateHandlers.delete(handler);
    },
    patchCardState(patch) { post({ type: "lince:patch-card-state", patch }); },

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
