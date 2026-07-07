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

function transportWsUrl() {
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${window.location.host}/host/transport/ws`;
}

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
    html.includes("widget-frame-bootstrap.js")
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
  setCardState,
  patchCardState,
  setCardStreamsEnabled,
  handleShellAction,
  invalidateServerAuth,
  onError,
}) {
  let bridgeState = normalizeBridgeState(initialState);
  let socket = null;
  let socketReady = null;
  let reconnectTimer = null;
  let nextActionRequestId = 1;
  const subscriptions = new Map();
  const pendingActions = new Map();

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
    if (!socket || socket.readyState !== WebSocket.OPEN) {
      return false;
    }
    socket.send(JSON.stringify(payload));
    return true;
  }

  function connectTransport() {
    if (socket && socket.readyState !== WebSocket.CLOSED) {
      return socketReady || Promise.resolve();
    }

    socketReady = new Promise((resolve, reject) => {
      try {
        socket = new WebSocket(transportWsUrl());
      } catch (error) {
        socketReady = null;
        reject(error);
        return;
      }

      socket.addEventListener("open", () => {
        for (const entry of subscriptions.values()) {
          sendTransport(entry.message);
        }
        resolve();
      }, { once: true });

      socket.addEventListener("message", (event) => {
        let message = null;
        try {
          message = JSON.parse(event.data);
        } catch {
          return;
        }
        handleTransportMessage(message);
      });

      socket.addEventListener("close", () => {
        socket = null;
        socketReady = null;
        for (const { instanceId, subId } of subscriptions.values()) {
          postFrame(instanceId, {
            type: PROTEIN_ROWS,
            payload: { subId, rows: [], live: false },
          });
        }
        if (!reconnectTimer && subscriptions.size > 0) {
          reconnectTimer = window.setTimeout(() => {
            reconnectTimer = null;
            void connectTransport().catch(() => {});
          }, 1000);
        }
      });

      socket.addEventListener("error", () => {
        reject(new Error("Unable to connect to Lince transport."));
      }, { once: true });
    });

    return socketReady;
  }

  function handleTransportMessage(message) {
    const type = String(message?.type || "");
    if (type === "snapshot" || type === "update") {
      const entry = subscriptions.get(String(message.id || ""));
      if (!entry) {
        return;
      }
      postFrame(entry.instanceId, {
        type: PROTEIN_ROWS,
        payload: {
          subId: entry.subId,
          rows: cloneJsonValue(message.rows, []),
          live: true,
        },
      });
      return;
    }

    if (type === "action_ok" || type === "error") {
      const req = pendingActions.get(String(message.id || ""));
      if (!req) {
        return;
      }
      pendingActions.delete(String(message.id || ""));
      postFrame(req.instanceId, {
        type: PROTEIN_ACTION_RESULT,
        payload: {
          reqId: req.reqId,
          ok: type === "action_ok",
          created: message.created || null,
          facts: Number(message.facts) || 0,
          message: String(message.message || ""),
        },
      });
    }
  }

  function frameListensTo(instanceId, topic) {
    if (!instanceId || typeof getCardAbiListen !== "function") {
      return false;
    }

    // Enforcement, not convention: a sand only ever sees the topics its card
    // was configured to listen to (default: nothing).
    const listen = getCardAbiListen(instanceId);
    return Array.isArray(listen) && listen.includes(topic);
  }

  function emitWidgetEvent(sourceInstanceId, topic, data) {
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
      frame.contentWindow?.postMessage(eventMessage, "*");
    }
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

  async function handleProteinSubscribe(data, saved = false) {
    const instanceId = data.instanceId || "";
    const subId = String(data.payload?.subId || "");
    if (!instanceId || !subId) {
      return;
    }

    const id = namespacedId(instanceId, subId);
    const transportMessage = saved
      ? {
          type: "subscribe_saved",
          id,
          name: String(data.payload?.name || ""),
        }
      : {
          type: "subscribe",
          id,
          protein: cloneJsonValue(data.payload?.protein, {}),
        };

    subscriptions.set(id, { instanceId, subId, message: transportMessage });
    try {
      await connectTransport();
      sendTransport(transportMessage);
    } catch (error) {
      postFrame(instanceId, {
        type: WIDGET_ERROR,
        payload: {
          message: error instanceof Error ? error.message : "Unable to subscribe.",
        },
      });
    }
  }

  function handleProteinUnsubscribe(data) {
    const id = namespacedId(data.instanceId || "", data.payload?.subId || "");
    subscriptions.delete(id);
    sendTransport({ type: "unsubscribe", id });
  }

  async function handleProteinAction(data) {
    const instanceId = data.instanceId || "";
    const reqId = String(data.payload?.reqId || "");
    const id = `act:${instanceId}:${nextActionRequestId++}`;
    pendingActions.set(id, { instanceId, reqId });
    try {
      await connectTransport();
      sendTransport({
        type: "act",
        id,
        action: cloneJsonValue(data.payload?.action, {}),
      });
    } catch (error) {
      pendingActions.delete(id);
      postFrame(instanceId, {
        type: PROTEIN_ACTION_RESULT,
        payload: {
          reqId,
          ok: false,
          message: error instanceof Error ? error.message : "Unable to send Action.",
        },
      });
    }
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

    if (data.type === WIDGET_ACTION) {
      void handleAction(data);
      return;
    }

    if (data.type === PROTEIN_SUBSCRIBE) {
      void handleProteinSubscribe(data, false);
      return;
    }

    if (data.type === PROTEIN_SUBSCRIBE_SAVED) {
      void handleProteinSubscribe(data, true);
      return;
    }

    if (data.type === PROTEIN_UNSUBSCRIBE) {
      handleProteinUnsubscribe(data);
      return;
    }

    if (data.type === PROTEIN_ACTION) {
      void handleProteinAction(data);
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
