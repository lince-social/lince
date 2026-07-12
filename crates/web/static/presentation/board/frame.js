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
  const laneHandlers = new Set(); // {room, handler}
  let live = false;
  const liveHandlers = new Set();
  let reqSeq = 0;
  const nextReqId = () => `${instanceId}:a${++reqSeq}`;

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
          if (data.ok) waiter.resolve({ created: data.created, facts: data.facts });
          else waiter.reject(new Error(data.message || "action failed"));
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

    // WRITE: forward a typed Action; resolves { created, facts } or rejects.
    act(action) {
      const reqId = nextReqId();
      return new Promise((resolve, reject) => {
        actionWaiters.set(reqId, { resolve, reject });
        post({ type: "lince:action", reqId, action });
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
  };

  post({ type: "lince:ready" });
})();
