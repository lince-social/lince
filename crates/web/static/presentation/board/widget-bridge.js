const BRIDGE_STATE_EVENT = "widget-bridge-state";
const HOST_TO_WIDGET_STATE = "lince:bridge-state";
const WIDGET_READY = "lince:widget-ready";
const WIDGET_ACTION = "lince:widget-action";
const WIDGET_ERROR = "lince:bridge-error";

function apiPath(path) {
  if (path.startsWith("/api/")) {
    return `/host/${path.slice("/api/".length)}`;
  }

  return path;
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
  const response = await fetch(apiPath("/api/widget-bridge/actions/print"), {
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
  return `<script type="module" src="${apiPath("/static/presentation/board/widget-frame-bootstrap.js")}"></script>`;
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

  if (html.includes("</body>")) {
    return html.replace("</body>", `${injections}\n</body>`);
  }

  if (html.includes("</html>")) {
    return html.replace("</html>", `${injections}\n</html>`);
  }

  return `${html}\n${injections}`;
}

export function createWidgetBridge({
  statusNode,
  getFrames,
  initialState,
  getCardMeta,
  setCardState,
  patchCardState,
  setCardStreamsEnabled,
  invalidateServerAuth,
  onError,
}) {
  let bridgeState = normalizeBridgeState(initialState);

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
