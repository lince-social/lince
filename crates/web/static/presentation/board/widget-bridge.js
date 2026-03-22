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
  return `
<script>
(() => {
  if (window.__LINCE_WIDGET_HOST__) {
    return;
  }

  window.__LINCE_WIDGET_HOST__ = true;
  const instanceId = window.frameElement?.dataset?.packageInstanceId || "preview";
  const listeners = new Set();
  let lastDetail = {
    bridge: {},
    meta: {
      instanceId,
      source: "host",
      mode: "view",
      serverId: "",
      viewId: null,
      cardState: {},
      streams: {
        globalEnabled: true,
        cardEnabled: true,
        enabled: true,
      },
    },
  };

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

  function bridgeTargets() {
    const targets = Array.from(document.querySelectorAll("[data-lince-bridge-root]"));
    return targets.length ? targets : [document.body];
  }

  function emit(type, detail) {
    for (const target of bridgeTargets()) {
      target.dispatchEvent(new CustomEvent(type, {
        bubbles: true,
        detail,
      }));
    }
  }

  function send(type, payload) {
    window.parent.postMessage({
      type,
      instanceId,
      payload,
    }, "*");
  }

  function assignDetail(detail) {
    const nextDetail = detail && typeof detail === "object" ? detail : {};
    const nextMeta = nextDetail.meta && typeof nextDetail.meta === "object"
      ? nextDetail.meta
      : {};

    lastDetail = {
      bridge: nextDetail.bridge && typeof nextDetail.bridge === "object"
        ? nextDetail.bridge
        : {},
      meta: {
        instanceId,
        source: String(nextMeta.source || "host"),
        mode: nextMeta.mode === "edit" ? "edit" : "view",
        serverId: String(nextMeta.serverId || ""),
        viewId:
          nextMeta.viewId == null ? null : Number(nextMeta.viewId) || null,
        cardState: cloneJsonValue(nextMeta.cardState, {}),
        streams: {
          globalEnabled: nextMeta.streams?.globalEnabled !== false,
          cardEnabled: nextMeta.streams?.cardEnabled !== false,
          enabled: nextMeta.streams?.enabled !== false,
        },
      },
    };

    return lastDetail;
  }

  window.addEventListener("message", (event) => {
    if (!event.data || typeof event.data !== "object") {
      return;
    }

    if (event.data.type === "${HOST_TO_WIDGET_STATE}") {
      const detail = assignDetail(event.data.payload);
      emit("lince-bridge-state", detail);

      for (const listener of listeners) {
        listener(detail);
      }
      return;
    }

    if (event.data.type === "${WIDGET_ERROR}") {
      emit("lince-bridge-error", event.data.payload || {});
    }
  });

  window.LinceWidgetHost = {
    instanceId,
    print(label) {
      send("${WIDGET_ACTION}", {
        action: "print",
        label: String(label || "print"),
      });
    },
    requestState() {
      send("${WIDGET_READY}", {});
    },
    subscribe(handler) {
      if (typeof handler !== "function") {
        return () => {};
      }

      listeners.add(handler);
      handler(cloneJsonValue(lastDetail, {}));
      return () => listeners.delete(handler);
    },
    getState() {
      return cloneJsonValue(lastDetail, {});
    },
    getMeta() {
      return cloneJsonValue(lastDetail.meta, {});
    },
    getCardState() {
      return cloneJsonValue(lastDetail.meta?.cardState, {});
    },
    setCardState(nextState) {
      send("${WIDGET_ACTION}", {
        action: "set-card-state",
        state: cloneJsonValue(nextState, {}),
      });
    },
    patchCardState(patch) {
      send("${WIDGET_ACTION}", {
        action: "patch-card-state",
        patch: cloneJsonValue(patch, {}),
      });
    },
    setStreamsEnabled(enabled) {
      send("${WIDGET_ACTION}", {
        action: "set-card-streams-enabled",
        enabled: Boolean(enabled),
      });
    },
  };

  send("${WIDGET_READY}", {});
})();
</script>`.trim();
}

export function enhancePackageHtml(rawHtml) {
  const html = String(rawHtml || "");
  if (!html) {
    return "";
  }

  if (html.includes("window.__LINCE_WIDGET_HOST__")) {
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
