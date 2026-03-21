const BRIDGE_STATE_EVENT = "widget-bridge-state";
const HOST_TO_WIDGET_STATE = "lince:bridge-state";
const WIDGET_READY = "lince:widget-ready";
const WIDGET_ACTION = "lince:widget-action";
const WIDGET_ERROR = "lince:bridge-error";

function apiPath(path) {
  return path;
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

function dispatchBridgeState(node, state) {
  if (!node) {
    return;
  }

  node.dispatchEvent(
    new CustomEvent(BRIDGE_STATE_EVENT, {
      bubbles: true,
      detail: normalizeBridgeState(state),
    }),
  );
}

function postBridgeState(frame, state) {
  if (!frame?.contentWindow) {
    return;
  }

  frame.contentWindow.postMessage(
    {
      type: HOST_TO_WIDGET_STATE,
      payload: normalizeBridgeState(state),
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

  window.addEventListener("message", (event) => {
    if (!event.data || typeof event.data !== "object") {
      return;
    }

    if (event.data.type === "${HOST_TO_WIDGET_STATE}") {
      const detail = {
        bridge: event.data.payload || {},
        meta: {
          instanceId,
          source: "host",
        },
      };

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
      return () => listeners.delete(handler);
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

  if (html.includes("</body>")) {
    return html.replace("</body>", `${bridgeScript}\n</body>`);
  }

  if (html.includes("</html>")) {
    return html.replace("</html>", `${bridgeScript}\n</html>`);
  }

  return `${html}\n${bridgeScript}`;
}

export function createWidgetBridge({
  statusNode,
  getFrames,
  initialState,
  onError,
}) {
  let bridgeState = normalizeBridgeState(initialState);

  function render(state) {
    bridgeState = normalizeBridgeState(state);
    dispatchBridgeState(statusNode, bridgeState);

    for (const frame of getFrames()) {
      postBridgeState(frame, bridgeState);
    }
  }

  async function handleAction(message) {
    if (message?.payload?.action !== "print") {
      return;
    }

    try {
      const nextState = await requestBridgePrint(
        message.instanceId || "widget-desconhecido",
        message.payload?.label || "print",
      );
      render(nextState);
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
      postBridgeState(frame, bridgeState);
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
