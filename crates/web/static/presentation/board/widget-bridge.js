import { getSharedTransport } from "./transport.js";

const BRIDGE_STATE_EVENT = "widget-bridge-state";
const HOST_TO_WIDGET_STATE = "lince:bridge-state";
const WIDGET_READY = "lince:widget-ready";
const WIDGET_ACTION = "lince:widget-action";
const WIDGET_ERROR = "lince:bridge-error";
const WIDGET_EVENT = "lince:bridge-event";
const PROTEIN_SUBSCRIBE = "lince:protein-subscribe";
const PROTEIN_SUBSCRIBE_SAVED = "lince:protein-subscribe-saved";
const PROTEIN_UNSUBSCRIBE = "lince:protein-unsubscribe";
const PROTEIN_ROWS = "lince:protein-rows";
const PROTEIN_ACTION = "lince:protein-action";
const PROTEIN_ACTION_RESULT = "lince:protein-action-result";

// New-way (flat) protocol used by the `/board/frame.js` sand host. Unified into
// this one bridge (Stage 8b, base task 1): the same socket + the same ABI
// fan-out/scope logic now serve BOTH the legacy chrome (nested `payload`
// envelope, above) and the new-way sands (flat envelope, below). `frame.js`
// posts these up and expects the flat responses back.
const FLAT_READY = "lince:ready";
const FLAT_ACTION = "lince:action";
const FLAT_ACTION_RESULT = "lince:action-result";
const FLAT_PROTEIN_ERROR = "lince:protein-error";
const FLAT_LANE_JOIN = "lince:lane-join";
const FLAT_LANE_SEND = "lince:lane-send";
const FLAT_LANE_EVENT = "lince:lane-event";
const FLAT_LIVE = "lince:live";

function apiPath(path) {
  if (path.startsWith("http://") || path.startsWith("https://")) {
    return path;
  }

  if (path.startsWith("/host/")) {
    return path;
  }

  return `/host${path.startsWith("/") ? path : `/${path}`}`;
}

function cloneJsonValue(value, fallback = null) {
  try {
    if (value === undefined) {
      return fallback;
    }

    return JSON.parse(JSON.stringify(value));
  } catch {
    return fallback;
  }
}

function normalizeBridgeState(rawState) {
  return {
    printCount: Number(rawState?.printCount) || 0,
    lastSource: String(rawState?.lastSource || "nenhum"),
    lastMessage: String(
      rawState?.lastMessage || "Aguardando interacao entre widgets.",
    ),
  };
}

function normalizeBridgeMeta(rawMeta, instanceId = "") {
  const streams = rawMeta?.streams;
  const globalEnabled = streams?.globalEnabled !== false;
  const cardEnabled = streams?.cardEnabled !== false;

  return {
    instanceId: String(rawMeta?.instanceId || instanceId || "preview"),
    source: String(rawMeta?.source || "host"),
    mode: rawMeta?.mode === "edit" ? "edit" : "view",
    serverId: String(rawMeta?.serverId || ""),
    viewId: rawMeta?.viewId == null ? null : Number(rawMeta.viewId) || null,
    cardState: cloneJsonValue(rawMeta?.cardState, {}),
    shell: cloneJsonValue(rawMeta?.shell, {}),
    streams: {
      globalEnabled,
      cardEnabled,
      enabled:
        typeof streams?.enabled === "boolean"
          ? streams.enabled
          : globalEnabled && cardEnabled,
    },
  };
}

function dispatchBridgeState(node, detail) {
  if (!node) {
    return;
  }

  node.dispatchEvent(
    new CustomEvent(BRIDGE_STATE_EVENT, {
      bubbles: true,
      detail,
    }),
  );
}

function postBridgeState(frame, state, meta) {
  if (!frame?.contentWindow) {
    return;
  }

  const detail = {
    bridge: normalizeBridgeState(state),
    meta: normalizeBridgeMeta(
      meta,
      frame.dataset.packageInstanceId || frame.dataset.packagePreviewId || "",
    ),
  };

  frame.dataset.linceServerId = detail.meta.serverId || "";
  frame.dataset.linceViewId =
    detail.meta.viewId == null ? "" : String(detail.meta.viewId);

  frame.contentWindow.postMessage(
    {
      type: HOST_TO_WIDGET_STATE,
      payload: detail,
    },
    "*",
  );
}

async function requestBridgePrint(instanceId, label) {
  const response = await fetch(apiPath("/widget-bridge/actions/print"), {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      instanceId,
      label,
    }),
  });

  const payload = await response.json().catch(() => null);
  if (!response.ok) {
    throw new Error(payload?.error || "Falha ao registrar acao do widget.");
  }

  return normalizeBridgeState(payload);
}

function createDatastarBootstrapScript() {
  return `<script type="module" src="${apiPath("/static/vendored/datastar.js")}"></script>`;
}

function createBridgeBootstrapScript() {
  return `<script src="${apiPath("/static/presentation/board/widget-frame-bootstrap.js")}"></script>`;
}

function injectHostBootstrap(html, injections) {
  if (html.includes("</head>")) {
    return html.replace("</head>", `${injections}\n</head>`);
  }

  if (html.includes("<body")) {
    return html.replace(/<body([^>]*)>/i, `${injections}\n<body$1>`);
  }

  if (html.includes("<script")) {
    return html.replace(/<script/i, `${injections}\n<script`);
  }

  if (html.includes("</html>")) {
    return html.replace("</html>", `${injections}\n</html>`);
  }

  return `${injections}\n${html}`;
}

export function enhancePackageHtml(rawHtml) {
  const html = String(rawHtml || "");
  if (!html) {
    return "";
  }

  if (
    html.includes("window.__LINCE_WIDGET_HOST__") ||
    html.includes("widget-frame-bootstrap.js") ||
    // New-way sands bring their own host via `/board/frame.js` (Stage 8b);
    // do not also inject the legacy bootstrap, which would overwrite it.
    html.includes("/board/frame.js")
  ) {
    return html;
  }

  const bridgeScript = createBridgeBootstrapScript();
  const datastarScript = html.includes("datastar.js")
    ? ""
    : createDatastarBootstrapScript();
  const injections = [datastarScript, bridgeScript].filter(Boolean).join("\n");
  return injectHostBootstrap(html, injections);
}

export function createWidgetBridge({
  statusNode,
  getFrames,
  initialState,
  getCardMeta,
  getCardAbiListen,
  getCardGroupStack,
  setCardState,
  patchCardState,
  setCardStreamsEnabled,
  handleShellAction,
  invalidateServerAuth,
  onError,
}) {
  let bridgeState = normalizeBridgeState(initialState);
  const transport = getSharedTransport();
  let nextActionRequestId = 1;
  // subscription id -> { instanceId, subId, message, protocol } where protocol
  // is "flat" (new-way frame.js sand) or "nested" (legacy chrome). The protocol
  // decides the shape of the rows/error message posted back to the frame.
  const subscriptions = new Map();
  const pendingActions = new Map();
  // instanceId of new-way (flat-protocol) frames, learned when they announce
  // `lince:ready`. Lets the reconnect signal reach them as `lince:live`.
  const flatFrames = new Set();
  // New-way ABI lane rooms: room -> Set<instanceId> that joined it. Used for
  // in-page, group-scoped fan-out of `lince:lane-send` between sibling sands on
  // this board (the server suppresses self-echo, so same-session siblings must
  // be delivered in-page — this is exactly what kanban's scoped `recordClicked`
  // to its packaged record_info needs).
  const roomMembers = new Map();
  // ABI sand-to-sand events ride transport ephemeral lanes (blueprint VII.3):
  // one room per topic (`abi:<topic>`). The board joins a room for every topic
  // its cards listen to AND every topic it emits, so events reach OTHER
  // sessions/devices. Same-board siblings are still fanned out in-page because
  // the whole board is a single connection and the transport suppresses
  // self-echo — a server round-trip would never come back to this board.
  const joinedRooms = new Set();

  function frameForInstance(instanceId) {
    return getFrames().find(
      (frame) => frame?.dataset?.packageInstanceId === instanceId,
    );
  }

  function namespacedId(instanceId, subId) {
    return `${instanceId || "unknown"}:${subId || "default"}`;
  }

  function postFrame(instanceId, message) {
    frameForInstance(instanceId)?.contentWindow?.postMessage(message, "*");
  }

  function sendTransport(payload) {
    transport.send(payload);
    return true;
  }

  // Retained as a resolved promise so the many `await connectTransport()` call
  // sites keep working: the shared transport auto-connects and `send()` queues
  // frames until the socket opens, so there is nothing to await anymore.
  function connectTransport() {
    return Promise.resolve();
  }

  // Post a Protein rows message in the shape the frame's protocol expects: the
  // new-way sand (frame.js) reads `subId`/`rows` off the top level; legacy
  // chrome reads them from `payload`.
  function postRows(entry, rows, live) {
    if (entry.protocol === "flat") {
      postFrame(entry.instanceId, {
        type: PROTEIN_ROWS,
        subId: entry.subId,
        rows: cloneJsonValue(rows, []),
      });
      return;
    }
    postFrame(entry.instanceId, {
      type: PROTEIN_ROWS,
      payload: { subId: entry.subId, rows: cloneJsonValue(rows, []), live },
    });
  }

  // Post an Action result in the shape the frame's protocol expects.
  function postActionResult(req, ok, created, facts, message) {
    if (req.protocol === "flat") {
      postFrame(req.instanceId, {
        type: FLAT_ACTION_RESULT,
        reqId: req.reqId,
        ok,
        created: created || null,
        facts: Number(facts) || 0,
        message: String(message || ""),
      });
      return;
    }
    postFrame(req.instanceId, {
      type: PROTEIN_ACTION_RESULT,
      payload: {
        reqId: req.reqId,
        ok,
        created: created || null,
        facts: Number(facts) || 0,
        message: String(message || ""),
      },
    });
  }

  function handleTransportMessage(message) {
    const type = String(message?.type || "");
    if (type === "snapshot" || type === "update") {
      const entry = subscriptions.get(String(message.id || ""));
      if (!entry) {
        return;
      }
      postRows(entry, message.rows, true);
      return;
    }

    if (type === "lane_event") {
      const room = String(message.room || "");
      if (room.startsWith("abi:")) {
        // Legacy topic-based ABI from another session/device. `sourceInstanceId`
        // belongs to a remote board, so it won't match a local frame — delivery
        // just fans out to whoever listens here.
        const payload = message.payload || {};
        const topic = String(payload.topic || "").trim();
        if (topic) {
          deliverEventToFrames(
            topic,
            payload.data,
            payload.sourceInstanceId || "",
          );
        }
        return;
      }
      // New-way room-based ABI from another session/device. Deliver flat to the
      // sibling sands that joined this room here.
      deliverLaneEventToRoom(room, message.payload, message.from || "");
      return;
    }

    if (type === "action_ok" || type === "error") {
      const req = pendingActions.get(String(message.id || ""));
      if (!req) {
        return;
      }
      pendingActions.delete(String(message.id || ""));
      postActionResult(
        req,
        type === "action_ok",
        message.created || null,
        message.facts,
        message.message,
      );
    }
  }

  // Deliver a new-way ABI lane event to the sibling sands that joined `room` on
  // this board. Prunes members whose frame has gone away.
  function deliverLaneEventToRoom(room, payload, from) {
    const members = roomMembers.get(room);
    if (!members) {
      return;
    }
    for (const instanceId of [...members]) {
      if (!frameForInstance(instanceId)) {
        members.delete(instanceId);
        continue;
      }
      postFrame(instanceId, {
        type: FLAT_LANE_EVENT,
        room,
        from: from || "",
        payload: cloneJsonValue(payload, null),
      });
    }
    if (members.size === 0) {
      roomMembers.delete(room);
    }
  }

  // Wire the single shared transport: one message handler for every inbound
  // frame, one replay on (re)open, and a connection up/down signal that reaches
  // both protocols.
  transport.onMessage(handleTransportMessage);
  transport.onOpen(() => {
    for (const entry of subscriptions.values()) {
      sendTransport(entry.message);
    }
    // The Cell session is fresh on every (re)connect — re-join our lane rooms so
    // ABI events keep flowing.
    for (const room of joinedRooms) {
      sendTransport({ type: "lane_join", room });
    }
    for (const instanceId of flatFrames) {
      postFrame(instanceId, { type: FLAT_LIVE, live: true });
    }
  });
  transport.onLive((live) => {
    if (live) {
      return;
    }
    // Socket down: tell new-way sands, and blank legacy subscriptions' rows.
    for (const instanceId of flatFrames) {
      postFrame(instanceId, { type: FLAT_LIVE, live: false });
    }
    for (const entry of subscriptions.values()) {
      if (entry.protocol !== "flat") {
        postRows(entry, [], false);
      }
    }
  });

  function frameListensTo(instanceId, topic) {
    if (!instanceId || typeof getCardAbiListen !== "function") {
      return false;
    }

    // Enforcement, not convention: a sand only ever sees the topics its card
    // was configured to listen to (default: nothing).
    const listen = getCardAbiListen(instanceId);
    return Array.isArray(listen) && listen.includes(topic);
  }

  // Group-scoped ABI fan-out (Stage 8b, Phase 4). A GROUPED source sand only
  // reaches sibling sands in its OWN (innermost, tightest) group — e.g. a kanban
  // card's `recordClicked` reaches only the record_info sand packaged with it,
  // not a record_info in another group nor an unrelated sand that merely shares
  // an outer container. An UNGROUPED source still broadcasts board-wide,
  // preserving the pre-grouping behavior. Remote lane events (no local source
  // card) are never restricted here.
  function inEventScope(sourceInstanceId, targetInstanceId) {
    if (typeof getCardGroupStack !== "function") {
      return true;
    }
    const source = getCardGroupStack(sourceInstanceId) || [];
    if (!source.length) {
      return true;
    }
    const innermost = source[source.length - 1];
    const target = getCardGroupStack(targetInstanceId) || [];
    return target.includes(innermost);
  }

  function abiRoom(topic) {
    return `abi:${topic}`;
  }

  // Join (idempotently) a lane room so this board both receives its events and
  // is allowed to send to it (the session only fans out a `LaneSend` to rooms
  // the sender has joined).
  function ensureRoomJoined(room) {
    if (joinedRooms.has(room)) {
      return;
    }
    joinedRooms.add(room);
    void connectTransport()
      .then(() => sendTransport({ type: "lane_join", room }))
      .catch(() => {});
  }

  // Reconcile lane membership with the topics the current cards listen to.
  // Called whenever the frame set / listen config changes.
  function refreshLaneRooms() {
    if (typeof getCardAbiListen !== "function") {
      return;
    }
    const desired = new Set();
    for (const frame of getFrames()) {
      const instanceId = frame?.dataset?.packageInstanceId || "";
      const listen = instanceId ? getCardAbiListen(instanceId) : null;
      if (Array.isArray(listen)) {
        for (const topic of listen) {
          if (topic) {
            desired.add(abiRoom(topic));
          }
        }
      }
    }
    // Leave rooms no card listens to anymore (emit-only rooms are transient and
    // left implicitly on the next reconcile once nothing listens).
    for (const room of [...joinedRooms]) {
      if (!desired.has(room)) {
        joinedRooms.delete(room);
        sendTransport({ type: "lane_leave", room });
      }
    }
    for (const room of desired) {
      ensureRoomJoined(room);
    }
  }

  // Deliver an ABI event to every local frame that listens to the topic and did
  // not source it. Shared by same-board emit and remote lane events.
  function deliverEventToFrames(topic, data, sourceInstanceId) {
    const eventMessage = {
      type: WIDGET_EVENT,
      payload: {
        topic,
        data: cloneJsonValue(data, null),
        sourceInstanceId: sourceInstanceId || "",
      },
    };

    for (const frame of getFrames()) {
      const frameInstanceId = frame?.dataset?.packageInstanceId || "";
      if (frameInstanceId === eventMessage.payload.sourceInstanceId) {
        continue;
      }
      if (!frameListensTo(frameInstanceId, topic)) {
        continue;
      }
      if (!inEventScope(eventMessage.payload.sourceInstanceId, frameInstanceId)) {
        continue;
      }
      frame.contentWindow?.postMessage(eventMessage, "*");
    }
  }

  function emitWidgetEvent(sourceInstanceId, topic, data) {
    // Same-board siblings: in-page fan-out (the board is one connection, so the
    // transport would never echo this back to us).
    deliverEventToFrames(topic, data, sourceInstanceId);
    // Other sessions/devices: publish on the topic's ephemeral lane.
    const room = abiRoom(topic);
    ensureRoomJoined(room);
    void connectTransport()
      .then(() =>
        sendTransport({
          type: "lane_send",
          room,
          payload: {
            topic,
            data: cloneJsonValue(data, null),
            sourceInstanceId: sourceInstanceId || "",
          },
        }),
      )
      .catch(() => {});
  }

  function render(state) {
    bridgeState = normalizeBridgeState(state);
    dispatchBridgeState(statusNode, {
      bridge: bridgeState,
      meta: normalizeBridgeMeta(null),
    });

    for (const frame of getFrames()) {
      const instanceId = frame?.dataset?.packageInstanceId || "";
      const meta = normalizeBridgeMeta(
        typeof getCardMeta === "function" ? getCardMeta(instanceId) : null,
        instanceId,
      );
      postBridgeState(frame, bridgeState, meta);
    }

    // Frames or their listen config may have changed — reconcile lane rooms.
    refreshLaneRooms();
  }

  async function handleAction(message) {
    const action = message?.payload?.action;
    if (!action) {
      return;
    }

    try {
      if (action === "emit-event") {
        const topic = String(message.payload?.topic || "").trim();
        if (topic) {
          emitWidgetEvent(message.instanceId || "", topic, message.payload?.data);
        }
        return;
      }

      if (action === "print") {
        const nextState = await requestBridgePrint(
          message.instanceId || "widget-desconhecido",
          message.payload?.label || "print",
        );
        render(nextState);
        return;
      }

      if (action === "set-card-state" && typeof setCardState === "function") {
        setCardState(
          message.instanceId || "",
          cloneJsonValue(message.payload?.state, {}),
        );
        render(bridgeState);
        return;
      }

      if (
        action === "patch-card-state" &&
        typeof patchCardState === "function"
      ) {
        patchCardState(
          message.instanceId || "",
          cloneJsonValue(message.payload?.patch, {}),
        );
        render(bridgeState);
        return;
      }

      if (
        action === "set-card-streams-enabled" &&
        typeof setCardStreamsEnabled === "function"
      ) {
        setCardStreamsEnabled(
          message.instanceId || "",
          message.payload?.enabled !== false,
        );
        render(bridgeState);
        return;
      }

      if (action === "shell-action" && typeof handleShellAction === "function") {
        handleShellAction(
          message.instanceId || "",
          String(message.payload?.command || ""),
          cloneJsonValue(message.payload?.payload, {}),
        );
        render(bridgeState);
        return;
      }

      if (
        action === "invalidate-server-auth" &&
        typeof invalidateServerAuth === "function"
      ) {
        await invalidateServerAuth(String(message.payload?.serverId || ""));
        render(bridgeState);
      }
    } catch (error) {
      const payload = {
        message:
          error instanceof Error
            ? error.message
            : "Falha na ponte dos widgets.",
      };

      if (typeof onError === "function") {
        onError(payload.message);
      }

      for (const frame of getFrames()) {
        frame?.contentWindow?.postMessage(
          {
            type: WIDGET_ERROR,
            payload,
          },
          "*",
        );
      }
    }
  }

  // Both protocols reach these handlers. The new-way (flat) sand posts its
  // fields at the top level (`data.subId`); legacy chrome nests them under
  // `data.payload`. Presence of `data.payload` is the discriminator.
  function frameFields(data) {
    const nested = data.payload !== undefined && data.payload !== null;
    const source = nested ? data.payload : data;
    return {
      protocol: nested ? "nested" : "flat",
      instanceId: String(data.instanceId || ""),
      subId: String(source?.subId || ""),
      protein: source?.protein,
      name: source?.name,
      reqId: String(source?.reqId || ""),
      action: source?.action,
    };
  }

  function handleProteinSubscribe(data, saved = false) {
    const fields = frameFields(data);
    if (!fields.instanceId || !fields.subId) {
      return;
    }
    if (fields.protocol === "flat") {
      flatFrames.add(fields.instanceId);
    }

    const id = namespacedId(fields.instanceId, fields.subId);
    const transportMessage = saved
      ? { type: "subscribe_saved", id, name: String(fields.name || "") }
      : { type: "subscribe", id, protein: cloneJsonValue(fields.protein, {}) };

    subscriptions.set(id, {
      instanceId: fields.instanceId,
      subId: fields.subId,
      message: transportMessage,
      protocol: fields.protocol,
    });
    sendTransport(transportMessage);
  }

  function handleProteinUnsubscribe(data) {
    const fields = frameFields(data);
    const id = namespacedId(fields.instanceId, fields.subId);
    subscriptions.delete(id);
    sendTransport({ type: "unsubscribe", id });
  }

  function handleProteinAction(data) {
    const fields = frameFields(data);
    const id = `act:${fields.instanceId}:${nextActionRequestId++}`;
    pendingActions.set(id, {
      instanceId: fields.instanceId,
      reqId: fields.reqId,
      protocol: fields.protocol,
    });
    sendTransport({ type: "act", id, action: cloneJsonValue(fields.action, {}) });
  }

  // A new-way sand announced itself (`frame.js` posts `lince:ready`). Register
  // it for the reconnect `lince:live` signal and reply with the current
  // liveness so it can render immediately.
  function handleFlatReady(data) {
    const instanceId = String(data.instanceId || "");
    if (!instanceId) {
      return;
    }
    flatFrames.add(instanceId);
    postFrame(instanceId, { type: FLAT_LIVE, live: transport.isReady() });
  }

  function handleFlatLaneJoin(data) {
    const instanceId = String(data.instanceId || "");
    const room = String(data.room || "");
    if (!instanceId || !room) {
      return;
    }
    if (!roomMembers.has(room)) {
      roomMembers.set(room, new Set());
    }
    roomMembers.get(room).add(instanceId);
    // Join the server lane too so this room's events reach OTHER sessions.
    ensureRoomJoined(room);
  }

  function handleFlatLaneSend(data) {
    const sourceInstanceId = String(data.instanceId || "");
    const room = String(data.room || "");
    if (!room) {
      return;
    }
    const payload = data.payload;

    // In-page, group-scoped fan-out to sibling sands that joined this room on
    // this board (the server suppresses self-echo, so same-session siblings
    // would never get it back over the wire).
    const members = roomMembers.get(room);
    if (members) {
      for (const instanceId of [...members]) {
        if (instanceId === sourceInstanceId) {
          continue;
        }
        if (!frameForInstance(instanceId)) {
          members.delete(instanceId);
          continue;
        }
        if (!inEventScope(sourceInstanceId, instanceId)) {
          continue;
        }
        postFrame(instanceId, {
          type: FLAT_LANE_EVENT,
          room,
          from: sourceInstanceId,
          payload: cloneJsonValue(payload, null),
        });
      }
    }

    // Mirror to the server lane for OTHER sessions/devices.
    ensureRoomJoined(room);
    sendTransport({
      type: "lane_send",
      room,
      payload: cloneJsonValue(payload, null),
    });
  }

  function handleMessage(event) {
    const data = event.data;
    if (!data || typeof data !== "object" || typeof data.type !== "string") {
      return;
    }

    if (data.type === WIDGET_READY) {
      const frame = getFrames().find(
        (currentFrame) =>
          currentFrame.dataset.packageInstanceId === data.instanceId,
      );
      if (!frame) {
        return;
      }

      const meta = normalizeBridgeMeta(
        typeof getCardMeta === "function" ? getCardMeta(data.instanceId) : null,
        data.instanceId,
      );
      postBridgeState(frame, bridgeState, meta);
      return;
    }

    // New-way sand announced itself (frame.js). Distinct from the legacy
    // WIDGET_READY above, which is keyed off an existing board frame.
    if (data.type === FLAT_READY) {
      handleFlatReady(data);
      return;
    }

    if (data.type === WIDGET_ACTION) {
      void handleAction(data);
      return;
    }

    // Shared by both protocols; `frameFields` disambiguates flat vs nested.
    if (data.type === PROTEIN_SUBSCRIBE) {
      handleProteinSubscribe(data, false);
      return;
    }

    if (data.type === PROTEIN_SUBSCRIBE_SAVED) {
      handleProteinSubscribe(data, true);
      return;
    }

    if (data.type === PROTEIN_UNSUBSCRIBE) {
      handleProteinUnsubscribe(data);
      return;
    }

    // Legacy chrome sends PROTEIN_ACTION (nested); new-way sands send
    // FLAT_ACTION. Both route through the one Action handler.
    if (data.type === PROTEIN_ACTION || data.type === FLAT_ACTION) {
      handleProteinAction(data);
      return;
    }

    if (data.type === FLAT_LANE_JOIN) {
      handleFlatLaneJoin(data);
      return;
    }

    if (data.type === FLAT_LANE_SEND) {
      handleFlatLaneSend(data);
    }
  }

  window.addEventListener("message", handleMessage);
  render(bridgeState);

  return {
    getState() {
      return bridgeState;
    },
    setState(nextState) {
      render(nextState);
    },
    syncFrames() {
      render(bridgeState);
    },
    destroy() {
      window.removeEventListener("message", handleMessage);
    },
  };
}
