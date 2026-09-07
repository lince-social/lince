import {
  getSharedTransport,
  getTransportFor,
  releaseTransport,
} from "./transport.js";

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
const FLAT_SIGNING_STATE = "lince:signing-state";
const FLAT_PROTEIN_ERROR = "lince:protein-error";
const FLAT_LANE_JOIN = "lince:lane-join";
const FLAT_LANE_SEND = "lince:lane-send";
const FLAT_LANE_EVENT = "lince:lane-event";
const FLAT_COLLAB_JOIN = "lince:collab-join";
const FLAT_COLLAB_LEAVE = "lince:collab-leave";
const FLAT_COLLAB_UPDATE = "lince:collab-update";
const FLAT_COLLAB_STATE = "lince:collab-state";
const FLAT_COLLAB_ACK = "lince:collab-ack";
const FLAT_COLLAB_RESET = "lince:collab-reset";
const FLAT_LIVE = "lince:live";
const FLAT_ENTER_LIVE = "lince:enter-live";
const FLAT_LIVE_ORGAN = "lince:live-organ";
const FLAT_PATCH_CARD_STATE = "lince:patch-card-state";
// Only one tooltip may be up at a time, and each sand is its own document that
// cannot see the others. lynx-ui.js announces the one it just showed; the board
// passes the announcement on so every other sand hides its own.
const TOOLTIP_SHOWN = "lince:tooltip-shown";
const FLAT_ARCHIVE_WORKSPACE = "lince:archive-workspace";
const FLAT_TERMINAL_OPEN = "lince:terminal-open";
const FLAT_SCAN_CODE = "lince:scan-code";
const FLAT_TERMINAL_INPUT = "lince:terminal-input";
const FLAT_TERMINAL_RESIZE = "lince:terminal-resize";
const FLAT_TERMINAL_CLOSE = "lince:terminal-close";

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
    viewer: cloneJsonValue(rawMeta?.viewer, null),
    shell: cloneJsonValue(rawMeta?.shell, {}),
    permissions: Array.isArray(rawMeta?.permissions)
      ? rawMeta.permissions.map((value) => String(value))
      : [],
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

  return injectHostBootstrap(html, createBridgeBootstrapScript());
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
  bindAllHosts,
  archiveWorkspace,
  scanCode,
  onError,
}) {
  let bridgeState = normalizeBridgeState(initialState);
  // Our own Cell. Board-scoped traffic rides this one no matter what any sand
  // is bound to: lane rooms are how the sands ON THIS BOARD talk to each other
  // and to our own other sessions, and a PTY is a process on THIS machine.
  // Routing either to a contact's Cell would be a different feature wearing the
  // same frame name.
  const transport = getSharedTransport();
  // host -> signing state. One per Cell: each binds its own Person to its own
  // challenge, and a sand must be told about ITS host, never another one's.
  const signingStates = new Map();
  signingStates.set("", transport.getSigningState());
  // Connections we have already wired handlers onto, so attaching is idempotent
  // when a second sand binds a host that is already open.
  const wiredHosts = new Map(); // host -> the connection its handlers are on
  let nextActionRequestId = 1;
  // subscription id -> { instanceId, subId, message, protocol } where protocol
  // is "flat" (new-way frame.js sand) or "nested" (legacy chrome). The protocol
  // decides the shape of the rows/error message posted back to the frame.
  const subscriptions = new Map();
  const pendingActions = new Map();
  // transport terminal id -> owning sand/session. PTYs are scoped to this
  // board socket and are never replayed after reconnect.
  const terminalSessions = new Map();
  // instanceId of new-way (flat-protocol) frames, learned when they announce
  // `lince:ready`. Lets the reconnect signal reach them as `lince:live`.
  const flatFrames = new Set();
  // New-way ABI lane rooms: room -> Set<instanceId> that joined it. Used for
  // in-page, group-scoped fan-out of `lince:lane-send` between sibling sands on
  // this board (the server suppresses self-echo, so same-session siblings must
  // be delivered in-page — this is exactly what kanban's scoped `recordClicked`
  // to its packaged Record needs).
  const roomMembers = new Map();
  // ABI sand-to-sand events ride transport ephemeral lanes (blueprint VII.3):
  // one room per topic (`abi:<topic>`). The board joins a room for every topic
  // its cards listen to AND every topic it emits, so events reach OTHER
  // sessions/devices. Same-board siblings are still fanned out in-page because
  // the whole board is a single connection and the transport suppresses
  // self-echo — a server round-trip would never come back to this board.
  const joinedRooms = new Set();
  // In-flight collab updates awaiting a `collab_ack`: id -> {instanceId,
  // recordUid, token}. Cleared on ack; a dropped socket simply leaves entries
  // unacked, which is exactly the signal the sand needs to re-export.
  const collabPending = new Map();
  let collabSendCounter = 0;
  // Live collab docs (Ontology §11 "Collab"): recordUid -> Set<instanceId>
  // that joined it. One server-side join per record regardless of how many
  // sands on this board edit it; snapshots fan out to every member frame.
  const collabMembers = new Map();
  // record uid -> the host its doc lives on. A collab doc belongs to ONE Cell:
  // the record it mirrors is stored there, and sending an update anywhere else
  // would write into a different Organ's document of the same name. Captured on
  // join and used for the leave, which happens after the last member is gone
  // and there is no instance left to ask.
  const collabHosts = new Map();

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

  // Which Cell a sand's data and Actions belong to. "" is our own.
  function hostOf(instanceId) {
    return String(getCardMeta(instanceId)?.serverId || "");
  }

  // The connection for a sand, wiring the inbound handlers the first time we
  // reach a given host. Frames from every open Cell land in the same
  // `handleTransportMessage`, which routes by subscription id — and every
  // subscription id is namespaced by instance, so a reply can only ever reach
  // the sand that asked for it.
  function hostTransport(instanceId) {
    return connectionForHost(hostOf(instanceId));
  }

  // The connection to a named Cell, wiring its inbound handlers on first reach.
  //
  // Taken by HOST rather than by sand because an event-driven sand does not
  // read its own binding: the Record pin is handed a record that lives on
  // whichever Lince emitted the event, and has to fetch it from there.
  function connectionForHost(host) {
    const connection = getTransportFor(host);
    // Keyed by the CONNECTION, not just the host: a released host that is bound
    // again gets a brand new connection object with empty listener sets, and
    // remembering only the host name would leave that one unwired — the sand
    // would sit there receiving nothing, with no error to show for it.
    if (host && wiredHosts.get(host) !== connection) {
      wiredHosts.set(host, connection);
      attachHostHandlers(connection, host);
    }
    return connection;
  }

  // Sand-scoped traffic: Protein subscriptions, Actions, collab docs. Goes to
  // the Cell that sand is bound to.
  function sendFor(instanceId, payload) {
    hostTransport(instanceId).send(payload);
    return true;
  }

  // Board-scoped traffic: lane rooms and terminals. Always our own Cell.
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
  // Warnings are non-fatal advisories (link cycles, Proof loops) — they ride
  // alongside ok, never turn a success into an error.
  function postActionResult(req, ok, created, facts, message, warnings, code, data) {
    const safeWarnings = Array.isArray(warnings) ? warnings : [];
    // Structured result for surfaces that need more than a uid to render what
    // happened — an enrolment code and its QR, a front door queue. Null when
    // the Action had nothing extra to say, which is almost all of them.
    const safeData = data === undefined ? null : data;
    if (req.protocol === "flat") {
      postFrame(req.instanceId, {
        type: FLAT_ACTION_RESULT,
        reqId: req.reqId,
        ok,
        created: created || null,
        facts: Number(facts) || 0,
        message: String(message || ""),
        code: String(code || ""),
        warnings: safeWarnings,
        data: safeData,
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
        code: String(code || ""),
        warnings: safeWarnings,
        data: safeData,
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
            // Taken from the wire, never recomputed: the source card lives on
            // the board that sent this, so there is nothing here to ask.
            payload.organ,
          );
        }
        return;
      }
      // New-way room-based ABI from another session/device. Deliver flat to the
      // sibling sands that joined this room here.
      deliverLaneEventToRoom(
        room,
        message.payload,
        message.from || "",
        message.identity || "",
        message.organ || "",
      );
      return;
    }

    if (type === "collab_ack") {
      const id = String(message.id || "");
      const pending = collabPending.get(id);
      if (!pending) {
        return;
      }
      collabPending.delete(id);
      if (frameForInstance(pending.instanceId)) {
        postFrame(pending.instanceId, {
          type: FLAT_COLLAB_ACK,
          recordUid: pending.recordUid,
          token: pending.token,
        });
      }
      return;
    }

    if (type === "collab_state" || type === "collab_change") {
      const recordUid = String(message.record_uid || "");
      const members = collabMembers.get(recordUid);
      if (!members) {
        return;
      }
      for (const instanceId of [...members]) {
        if (!frameForInstance(instanceId)) {
          members.delete(instanceId);
          continue;
        }
        postFrame(instanceId, {
          type: FLAT_COLLAB_STATE,
          recordUid,
          snapshotBase64: String(message.snapshot_base64 || ""),
        });
      }
      if (members.size === 0) {
        collabMembers.delete(recordUid);
        getTransportFor(collabHosts.get(recordUid) || "").send({
          type: "collab_leave",
          record_uid: recordUid,
        });
        collabHosts.delete(recordUid);
      }
      return;
    }

    if (type === "terminal_opened" || type === "terminal_data" || type === "terminal_exit") {
      const entry = terminalSessions.get(String(message.id || ""));
      if (!entry) {
        return;
      }
      if (type === "terminal_opened") {
        entry.opened = true;
        postFrame(entry.instanceId, {
          type: "lince:terminal-opened",
          sessionId: entry.sessionId,
          shell: String(message.shell || ""),
          cwd: String(message.cwd || ""),
        });
      } else if (type === "terminal_data") {
        postFrame(entry.instanceId, {
          type: "lince:terminal-data",
          sessionId: entry.sessionId,
          dataBase64: String(message.data_base64 || ""),
        });
      } else {
        terminalSessions.delete(String(message.id || ""));
        postFrame(entry.instanceId, {
          type: "lince:terminal-exit",
          sessionId: entry.sessionId,
          exitCode: message.exit_code ?? null,
        });
      }
      return;
    }

    if (type === "error") {
      const terminal = terminalSessions.get(String(message.id || ""));
      if (terminal) {
        if (!terminal.opened) {
          terminalSessions.delete(String(message.id || ""));
        }
        postFrame(terminal.instanceId, {
          type: "lince:terminal-error",
          sessionId: terminal.sessionId,
          message: String(message.message || "terminal session failed"),
        });
        return;
      }
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
        message.warnings,
        message.code,
        message.data,
      );
    }
  }

  // Deliver a new-way ABI lane event to the sibling sands that joined `room` on
  // this board. Prunes members whose frame has gone away.
  //
  // `identity` is carried separately from `from` and must survive this hop:
  // `from` is a CONNECTION id, while `identity` is the sender's subject and is
  // present only when the host resolved that this viewer may know it
  // (Ontology §11 presence). Dropping it here is what made named cursors
  // impossible — a sand that never receives it can only ever render "someone".
  function deliverLaneEventToRoom(room, payload, from, identity, organ) {
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
        identity: identity || "",
        // Taken from the wire, never recomputed: the source card lives on the
        // board that sent this, so there is nothing here to ask. Absent means
        // this Cell, which is what "" already means to a sand.
        organ: organ || "",
        payload: cloneJsonValue(payload, null),
      });
    }
    if (members.size === 0) {
      roomMembers.delete(room);
    }
  }

  // Sands currently bound to a host, so a reconnect replays only what belongs
  // to the Cell that reconnected.
  function instancesOn(host) {
    return [...flatFrames].filter((instanceId) => hostOf(instanceId) === host);
  }

  // Filtered on the host the subscription was SENT to, not on where its card
  // points now. Those differ exactly when a binding has just changed, and
  // resolving live would replay the old connection's subscription onto the new
  // Cell while leaving the old one still feeding rows nobody asked for.
  function subscriptionsOn(host) {
    return [...subscriptions.values()].filter((entry) => entry.host === host);
  }

  // A reconnect replays the joins for the docs that live on THAT Cell, and
  // tells their frames to re-export from their last acked version. The snapshot
  // that comes back heals Cell -> sand; nothing heals sand -> Cell, which is
  // why the pending set is declared lost rather than retried.
  function rejoinCollabDocsOn(host, send) {
    for (const [recordUid, members] of collabMembers) {
      if ((collabHosts.get(recordUid) || "") !== host) {
        continue;
      }
      send({
        type: "collab_join",
        id: `collab:${recordUid}`,
        record_uid: recordUid,
      });
      for (const instanceId of members) {
        if (frameForInstance(instanceId)) {
          postFrame(instanceId, { type: FLAT_COLLAB_RESET, recordUid });
        }
      }
    }
  }

  // Everything a remote host needs. Deliberately narrower than the local block
  // below: no lane rooms, no terminals, no collab rejoin — those are board- and
  // machine-scoped and stay on our own Cell.
  function attachHostHandlers(connection, host) {
    connection.onMessage(handleTransportMessage);
    connection.onOpen(() => {
      for (const entry of subscriptionsOn(host)) {
        connection.send(entry.message);
      }
      collabPending.clear();
      rejoinCollabDocsOn(host, (payload) => connection.send(payload));
      for (const instanceId of instancesOn(host)) {
        postFrame(instanceId, { type: FLAT_LIVE, live: true });
        postFrame(instanceId, { type: FLAT_LIVE_ORGAN, organ: host });
      }
    });
    connection.onLive((live) => {
      if (live) return;
      for (const instanceId of instancesOn(host)) {
        postFrame(instanceId, { type: FLAT_LIVE, live: false });
      }
      for (const entry of subscriptionsOn(host)) {
        if (entry.protocol !== "flat") {
          postRows(entry, [], false);
        }
      }
    });
    connection.onSigningState((state) => {
      const previous = signingStates.get(host);
      const becameAvailable = !previous?.available && Boolean(state?.available);
      signingStates.set(host, cloneJsonValue(state, {}));
      for (const instanceId of instancesOn(host)) {
        postFrame(instanceId, {
          type: FLAT_SIGNING_STATE,
          ...signingStates.get(host),
        });
      }
      if (becameAvailable) {
        for (const entry of subscriptionsOn(host)) {
          connection.send(entry.message);
        }
      }
    });
  }

  // Wire our own Cell: one message handler for every inbound frame, one replay
  // on (re)open, and a connection up/down signal that reaches both protocols.
  transport.onMessage(handleTransportMessage);
  transport.onOpen(() => {
    for (const entry of subscriptionsOn("")) {
      sendTransport(entry.message);
    }
    // The Cell session is fresh on every (re)connect — re-join our lane rooms so
    // ABI events keep flowing.
    for (const room of joinedRooms) {
      sendTransport({ type: "lane_join", room });
    }
    // Re-join collab docs too: the fresh join replays a full snapshot, which
    // heals whatever the sand's doc missed while the socket was down.
    //
    // That heals ONE direction. The snapshot flows Cell -> sand, so anything
    // this sand sent and never got acked is still missing at the Cell and no
    // amount of rejoining will carry it there. Any update still pending when
    // the socket dropped is therefore declared lost, and each member frame is
    // told to re-export from its last ACKED version.
    collabPending.clear();
    rejoinCollabDocsOn("", (payload) => sendTransport(payload));
    for (const instanceId of instancesOn("")) {
      postFrame(instanceId, { type: FLAT_LIVE, live: true });
    }
  });
  transport.onLive((live) => {
    if (live) {
      return;
    }
    // Socket down: tell new-way sands, and blank legacy subscriptions' rows.
    for (const instanceId of instancesOn("")) {
      postFrame(instanceId, { type: FLAT_LIVE, live: false });
    }
    for (const entry of terminalSessions.values()) {
      postFrame(entry.instanceId, {
        type: "lince:terminal-error",
        sessionId: entry.sessionId,
        message: "terminal transport disconnected",
      });
    }
    terminalSessions.clear();
    if (!live) {
      for (const req of pendingActions.values()) {
        postActionResult(
          req,
          false,
          null,
          0,
          "The Cell connection closed before the Action completed.",
          [],
          "session_disconnected",
        );
      }
      pendingActions.clear();
    }
    for (const entry of subscriptionsOn("")) {
      if (entry.protocol !== "flat") {
        postRows(entry, [], false);
      }
    }
  });
  transport.onSigningState((state) => {
    const previous = signingStates.get("");
    const becameAvailable = !previous?.available && Boolean(state?.available);
    signingStates.set("", cloneJsonValue(state, {}));
    for (const instanceId of instancesOn("")) {
      postFrame(instanceId, { type: FLAT_SIGNING_STATE, ...signingStates.get("") });
    }
    // Capability-bearing Protein rows may have been projected while the
    // session was still proving its key. Refresh them once that signer becomes
    // usable so controls do not remain falsely disabled until another Fact.
    if (becameAvailable) {
      for (const entry of subscriptionsOn("")) {
        sendTransport(entry.message);
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
  // card's `recordClicked` reaches only the Record sand packaged with it,
  // not a Record in another group nor an unrelated sand that merely shares
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
  function deliverEventToFrames(topic, data, sourceInstanceId, organ) {
    for (const frame of getFrames()) {
      const frameInstanceId = frame?.dataset?.packageInstanceId || "";
      if (frameInstanceId === (sourceInstanceId || "")) {
        continue;
      }
      if (!frameListensTo(frameInstanceId, topic)) {
        continue;
      }
      if (!inEventScope(sourceInstanceId || "", frameInstanceId)) {
        continue;
      }
      postLaneEvent(frame, frameInstanceId, topic, data, sourceInstanceId, organ);
    }
  }

  // `organ` is which Cell the thing this event refers to actually lives on.
  //
  // A record uid alone is not enough to fetch a record: the same uid names
  // nothing, or something else entirely, on a different Lince. Without it an
  // event-driven sand can only look in its own binding, so a click in a kanban
  // reading someone else's Lince opened a blank Record panel — the record was
  // never missing, we were asking the wrong Cell for it.
  function postLaneEvent(
    frame,
    frameInstanceId,
    topic,
    data,
    sourceInstanceId,
    organ,
  ) {
    const cloned = cloneJsonValue(data, null);
    const host = String(organ || "");
    const message = flatFrames.has(frameInstanceId)
      ? {
          type: "lince:lane-event",
          room: topic,
          payload: cloned,
          from: sourceInstanceId || "",
          organ: host,
        }
      : {
          type: WIDGET_EVENT,
          payload: {
            topic,
            data: cloned,
            sourceInstanceId: sourceInstanceId || "",
            organ: host,
          },
        };
    frame.contentWindow?.postMessage(message, "*");
  }

  function deliverEventToCard(cardId, topic, data, organ) {
    for (const frame of getFrames()) {
      const frameInstanceId = frame?.dataset?.packageInstanceId || "";
      if (frameInstanceId === cardId) {
        postLaneEvent(frame, frameInstanceId, topic, data, "", organ);
      }
    }
  }

  function emitWidgetEvent(sourceInstanceId, topic, data) {
    // The Cell the emitting sand is reading. Resolved HERE, where the source
    // card is known, and carried on the wire — a receiver cannot recompute it,
    // least of all one on another device where the source card does not exist.
    const organ = hostOf(sourceInstanceId);
    // Same-board siblings: in-page fan-out (the board is one connection, so the
    // transport would never echo this back to us).
    deliverEventToFrames(topic, data, sourceInstanceId, organ);
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
            organ,
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
    for (const [id, entry] of [...terminalSessions]) {
      if (!frameForInstance(entry.instanceId)) {
        sendTransport({ type: "terminal_close", id });
        terminalSessions.delete(id);
      }
    }
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
      // An explicit Cell for THIS request, overriding the card's binding.
      // `undefined` means "wherever this sand is bound"; "" is a real value
      // meaning our own Cell, so the two must stay distinguishable.
      organ: source?.organ,
    };
  }

  // Where one request goes: the Cell the sand named, or the card's binding.
  function requestHost(fields) {
    return fields.organ === undefined || fields.organ === null
      ? hostOf(fields.instanceId)
      : String(fields.organ || "");
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

    const host = requestHost(fields);
    subscriptions.set(id, {
      instanceId: fields.instanceId,
      subId: fields.subId,
      message: transportMessage,
      protocol: fields.protocol,
      host,
    });
    connectionForHost(host).send(transportMessage);
  }

  function handleProteinUnsubscribe(data) {
    const fields = frameFields(data);
    const id = namespacedId(fields.instanceId, fields.subId);
    const entry = subscriptions.get(id);
    subscriptions.delete(id);
    // Unsubscribe where it was subscribed.
    getTransportFor(entry ? entry.host : hostOf(fields.instanceId)).send({
      type: "unsubscribe",
      id,
    });
  }

  function handleProteinAction(data) {
    const fields = frameFields(data);
    const id = `act:${fields.instanceId}:${nextActionRequestId++}`;
    pendingActions.set(id, {
      instanceId: fields.instanceId,
      reqId: fields.reqId,
      protocol: fields.protocol,
    });
    // An Action goes to the Cell the sand named, exactly as its reads do. They
    // have to agree: an event-driven sand READING a record from another Lince
    // and WRITING back to its own binding would create or clobber a local
    // record carrying that uid — a silent cross-Cell write, wrong Organ in the
    // Ledger, and no error anywhere.
    void connectionForHost(requestHost(fields))
      .sendAction(id, cloneJsonValue(fields.action, {}))
      .catch((error) => {
        const request = pendingActions.get(id);
        if (!request) return;
        pendingActions.delete(id);
        postActionResult(
          request,
          false,
          null,
          0,
          error?.message || "Session signing is unavailable.",
          [],
          error?.code || "session_signing_unavailable",
        );
      });
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
    // Everything this sand is told is about ITS host, which may be a contact's
    // Cell while the sand beside it is showing our own.
    const host = hostOf(instanceId);
    const connection = hostTransport(instanceId);
    postFrame(instanceId, { type: FLAT_LIVE, live: connection.isReady() });
    postFrame(instanceId, {
      type: FLAT_SIGNING_STATE,
      ...(signingStates.get(host) || connection.getSigningState()),
    });
    // A sand must know whose Cell it is showing. Presenting someone else's
    // Organ as yours is the one mistake here that cannot be walked back.
    postFrame(instanceId, { type: FLAT_LIVE_ORGAN, organ: host });
    // Also re-push THIS frame's current bridge-state/cardState now (2026-07-18).
    // A cold page load creates the iframe and calls the bridge's initial
    // render() essentially back-to-back — postMessage to a still-loading
    // iframe (frame.js hasn't attached its listener yet) is silently
    // dropped, so that first cardState push can vanish. `lince:ready` only
    // fires once frame.js IS listening, so replying here guarantees every
    // sand eventually sees its real cardState (a Data-panel Protein, saved
    // UI prefs, ...) instead of only picking it up on some LATER unrelated
    // board change. This gap existed before too; it just stayed invisible
    // while kanban/relations still had an auto-applied default Protein to
    // fall back on.
    const frame = frameForInstance(instanceId);
    if (frame) {
      const meta =
        typeof getCardMeta === "function" ? getCardMeta(instanceId) : null;
      postBridgeState(frame, bridgeState, meta);
    }
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
    // The Cell the emitting sand is reading, resolved here where the source
    // card is known. A receiver cannot work this out — least of all one on
    // another device, where the source card does not exist — so it travels.
    const organ = hostOf(sourceInstanceId);

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
          organ,
          payload: cloneJsonValue(payload, null),
        });
      }
    }

    // Mirror to the server lane for OTHER sessions/devices, with the same host
    // the in-page siblings got — as a SIBLING of `payload`, never inside it, so
    // the payload every existing sand parses keeps its exact shape and a board
    // that predates the field reads straight past it.
    //
    // Omitted when it is our own Cell, which is also what a lane means by
    // saying nothing. Lane traffic always rides our own transport even when
    // every card on the board is bound elsewhere, so a room has exactly one
    // Cell and "ours" is the same Cell for everyone in it — there is no uid to
    // translate on the way out or back.
    ensureRoomJoined(room);
    sendTransport({
      type: "lane_send",
      room,
      payload: cloneJsonValue(payload, null),
      ...(organ ? { organ } : {}),
    });
  }

  function terminalTransportId(instanceId, sessionId) {
    return `terminal:${instanceId}:${sessionId}`;
  }

  function terminalGeometry(raw) {
    const bounded = (value, fallback, min, max) =>
      Math.max(min, Math.min(max, Math.trunc(Number(value) || fallback)));
    return {
      cols: bounded(raw?.cols, 80, 1, 1000),
      rows: bounded(raw?.rows, 24, 1, 1000),
      pixel_width: bounded(raw?.pixelWidth ?? raw?.pixel_width, 0, 0, 65535),
      pixel_height: bounded(raw?.pixelHeight ?? raw?.pixel_height, 0, 0, 65535),
    };
  }

  function terminalEntry(data) {
    const instanceId = String(data.instanceId || "");
    const sessionId = String(data.sessionId || "");
    const id = terminalTransportId(instanceId, sessionId);
    return { id, instanceId, sessionId, current: terminalSessions.get(id) };
  }

  function isCurrentFrameSource(instanceId, source) {
    return frameForInstance(instanceId)?.contentWindow === source;
  }

  function handleTerminalOpen(data, source) {
    const entry = terminalEntry(data);
    const permissions =
      typeof getCardMeta === "function"
        ? getCardMeta(entry.instanceId)?.permissions
        : [];
    if (
      !entry.instanceId ||
      !entry.sessionId ||
      !isCurrentFrameSource(entry.instanceId, source) ||
      !Array.isArray(permissions) ||
      !permissions.includes("terminal_session")
    ) {
      postFrame(entry.instanceId, {
        type: "lince:terminal-error",
        sessionId: entry.sessionId,
        message: "terminal_session permission is required",
      });
      return;
    }
    if (entry.current) {
      return;
    }
    terminalSessions.set(entry.id, { ...entry, opened: false });
    sendTransport({
      type: "terminal_open",
      id: entry.id,
      ...terminalGeometry(data.geometry),
    });
  }

  /// Camera scan, gated the same way a terminal is: the card must hold the
  /// permission, and the request must come from that card's REAL iframe — a
  /// page-level impostor posting a stolen instanceId is ignored.
  ///
  /// The camera itself never belongs to the sand. `scanCode` is a chrome
  /// callback: the chrome owns the stream and the preview, and only the
  /// decoded text is posted back.
  function handleScanCode(data, source) {
    const instanceId = String(data.instanceId || "");
    const scanId = String(data.scanId || "");
    const permissions =
      typeof getCardMeta === "function"
        ? getCardMeta(instanceId)?.permissions
        : [];
    const refuse = (message) => {
      postFrame(instanceId, {
        type: "lince:scan-result",
        scanId,
        ok: false,
        message,
      });
    };
    if (
      !instanceId ||
      !scanId ||
      !isCurrentFrameSource(instanceId, source) ||
      !Array.isArray(permissions) ||
      !permissions.includes("media_capture")
    ) {
      refuse("media_capture permission is required");
      return;
    }
    if (typeof scanCode !== "function") {
      refuse("this host cannot open a camera");
      return;
    }
    void Promise.resolve(scanCode())
      .then((text) => {
        postFrame(instanceId, {
          type: "lince:scan-result",
          scanId,
          ok: true,
          // null is the CANCEL answer, distinct from a failure: the user
          // closing the camera is not an error a sand should show as one.
          text: text == null ? null : String(text),
        });
      })
      .catch((error) => refuse(error?.message || "scan failed"));
  }

  function handleTerminalCommand(data, source) {
    const entry = terminalEntry(data);
    if (
      !entry.current ||
      entry.current.instanceId !== entry.instanceId ||
      !isCurrentFrameSource(entry.instanceId, source)
    ) {
      return;
    }
    if (data.type === FLAT_TERMINAL_INPUT) {
      sendTransport({
        type: "terminal_input",
        id: entry.id,
        data_base64: String(data.dataBase64 || ""),
      });
    } else if (data.type === FLAT_TERMINAL_RESIZE) {
      sendTransport({
        type: "terminal_resize",
        id: entry.id,
        ...terminalGeometry(data.geometry),
      });
    } else {
      sendTransport({ type: "terminal_close", id: entry.id });
    }
  }

  function handleMessage(event) {
    const data = event.data;
    if (!data || typeof data !== "object" || typeof data.type !== "string") {
      return;
    }

    // Pure chrome: no card, no host, no permission to consider — it carries
    // nothing but "somebody is showing a tooltip now."
    if (data.type === TOOLTIP_SHOWN) {
      for (const frame of getFrames()) {
        frame.contentWindow?.postMessage(data, "*");
      }
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

    // "Enter their Lince" from the Organ sand: bind EVERY sand on this board to
    // that Organ at once, or `organ: null` to bring them all home.
    //
    // It used to repoint one board-wide socket, which is no longer what a host
    // binding is — each sand carries its own. So this is now a bulk edit of the
    // same per-sand setting the config modal writes one card at a time, which
    // means it persists, survives reload, and can be undone per sand instead of
    // being a hidden mode the board is secretly in.
    if (data.type === FLAT_ENTER_LIVE) {
      if (typeof bindAllHosts === "function") {
        bindAllHosts(data.organ || "");
      }
      return;
    }

    if (data.type === FLAT_LANE_JOIN) {
      handleFlatLaneJoin(data);
      return;
    }

    if (data.type === FLAT_LANE_SEND) {
      handleFlatLaneSend(data);
      return;
    }

    if (data.type === FLAT_COLLAB_JOIN) {
      const recordUid = String(data.recordUid || "");
      const instanceId = String(data.instanceId || "");
      if (!recordUid || !instanceId) {
        return;
      }
      let members = collabMembers.get(recordUid);
      if (!members) {
        members = new Set();
        collabMembers.set(recordUid, members);
      }
      members.add(instanceId);
      // A collab doc belongs to the Cell the RECORD lives on, which for an
      // event-driven sand is the Cell that emitted the event — not the one the
      // sand happens to be bound to.
      const collabHost = requestHost({
        instanceId,
        organ: data.organ,
      });
      collabHosts.set(recordUid, collabHost);
      // Always (re)send the join — the reply's snapshot is what a NEW member
      // frame needs even when this board already joined server-side.
      connectionForHost(collabHost).send({
        type: "collab_join",
        id: `collab:${recordUid}`,
        record_uid: recordUid,
      });
      return;
    }

    if (data.type === FLAT_COLLAB_LEAVE) {
      const recordUid = String(data.recordUid || "");
      const members = collabMembers.get(recordUid);
      if (members) {
        members.delete(String(data.instanceId || ""));
        if (members.size === 0) {
          collabMembers.delete(recordUid);
          getTransportFor(collabHosts.get(recordUid) || "").send({
            type: "collab_leave",
            record_uid: recordUid,
          });
          collabHosts.delete(recordUid);
        }
      }
      return;
    }

    if (data.type === FLAT_COLLAB_UPDATE) {
      const recordUid = String(data.recordUid || "");
      const instanceId = String(data.instanceId || "");
      const members = collabMembers.get(recordUid);
      // Only frames that joined the doc may write to it.
      if (!recordUid || !members || !members.has(instanceId)) {
        return;
      }
      // A UNIQUE id per update, not one per record: the id is what routes the
      // ack back to the frame that sent this particular delta, and a shared id
      // would make two in-flight updates indistinguishable — the first ack
      // would confirm work the Cell had not yet seen.
      collabSendCounter += 1;
      const id = `collab-up:${collabSendCounter}`;
      collabPending.set(id, {
        instanceId,
        recordUid,
        token: String(data.token || ""),
      });
      sendFor(instanceId, {
        type: "collab_update",
        id,
        record_uid: recordUid,
        update_base64: String(data.updateBase64 || ""),
      });
      return;
    }

    if (data.type === FLAT_TERMINAL_OPEN) {
      handleTerminalOpen(data, event.source);
      return;
    }

    if (data.type === FLAT_SCAN_CODE) {
      handleScanCode(data, event.source);
      return;
    }

    if (
      data.type === FLAT_TERMINAL_INPUT ||
      data.type === FLAT_TERMINAL_RESIZE ||
      data.type === FLAT_TERMINAL_CLOSE
    ) {
      handleTerminalCommand(data, event.source);
      return;
    }

    // Archive sand requesting a static export of the current workspace. Only
    // the card's REAL iframe may trigger it (same guard as terminals) — a
    // page-level impostor posting a stolen instanceId is ignored. The chrome
    // callback does the capture and the download; the disclosure decision is
    // the user's click inside that sand.
    if (data.type === FLAT_ARCHIVE_WORKSPACE) {
      const requestingInstanceId = String(data.instanceId || "");
      if (
        typeof archiveWorkspace === "function" &&
        requestingInstanceId &&
        isCurrentFrameSource(requestingInstanceId, event.source)
      ) {
        void archiveWorkspace(
          requestingInstanceId,
          cloneJsonValue(data.options, {}),
        );
      }
      return;
    }

    // New-way sand persisting its UI prefs into the card's widgetState (host
    // state). Mirrors the legacy "patch-card-state" action; re-render pushes
    // the updated cardState back down to every frame.
    if (data.type === FLAT_PATCH_CARD_STATE) {
      if (typeof patchCardState === "function") {
        patchCardState(
          String(data.instanceId || ""),
          cloneJsonValue(data.patch, {}),
        );
        render(bridgeState);
      }
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
    // A sand's host binding changed. Tear its work down on the Cell it was
    // pointed at BEFORE the new binding takes effect.
    //
    // Without this the old connection keeps computing and pushing rows for a
    // subscription nobody will unsubscribe — and, worse, keeps feeding a sand
    // that is now supposed to be showing a different Lince entirely. The frame
    // re-subscribes against its new host when it reloads.
    rebindHost(instanceId) {
      const id = String(instanceId || "");
      if (!id) return;
      for (const [key, entry] of [...subscriptions]) {
        if (entry.instanceId !== id) continue;
        subscriptions.delete(key);
        getTransportFor(entry.host).send({ type: "unsubscribe", id: key });
      }
      for (const [recordUid, members] of [...collabMembers]) {
        if (!members.has(id)) continue;
        members.delete(id);
        if (members.size === 0) {
          collabMembers.delete(recordUid);
          getTransportFor(collabHosts.get(recordUid) || "").send({
            type: "collab_leave",
            record_uid: recordUid,
          });
          collabHosts.delete(recordUid);
        }
      }
    },
    // Every Cell this board is actually reading right now.
    //
    // Card bindings alone no longer answer this. An event-driven sand holds its
    // host in the event it was given, not in any card, so a board can have a
    // live subscription on a Lince that appears in nobody's `serverId` — and
    // closing that connection for looking unused would blank the sand with no
    // message and nothing to re-dial it.
    hostsInUse() {
      const hosts = new Set();
      for (const entry of subscriptions.values()) {
        if (entry.host) hosts.add(entry.host);
      }
      for (const host of collabHosts.values()) {
        if (host) hosts.add(host);
      }
      return hosts;
    },
    emitLocal(topic, data) {
      // Chrome-originated navigation (for example accepting a conversation
      // notification) should drive this board's Record sand without being
      // rebroadcast to every other open board session.
      deliverEventToFrames(String(topic || ""), data, "");
    },
    emitLocalToCard(cardId, topic, data) {
      // Chrome owns shell-card navigation. Targeting the built-in Record pin
      // here keeps notification acceptance independent from a user's ABI
      // routing configuration while ordinary sand events remain filtered.
      deliverEventToCard(String(cardId || ""), String(topic || ""), data);
    },
    destroy() {
      for (const id of terminalSessions.keys()) {
        sendTransport({ type: "terminal_close", id });
      }
      terminalSessions.clear();
      window.removeEventListener("message", handleMessage);
    },
  };
}
