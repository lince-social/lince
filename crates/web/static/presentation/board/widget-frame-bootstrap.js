const HOST_TO_WIDGET_STATE = "lince:bridge-state";
const WIDGET_READY = "lince:widget-ready";
const WIDGET_ACTION = "lince:widget-action";
const WIDGET_ERROR = "lince:bridge-error";

(() => {
  if (window.__LINCE_WIDGET_HOST__) {
    return;
  }

  window.__LINCE_WIDGET_HOST__ = true;
  const instanceId =
    window.frameElement?.dataset?.packageInstanceId ||
    window.frameElement?.dataset?.packagePreviewId ||
    "preview";
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
    const targets = Array.from(
      document.querySelectorAll("[data-lince-bridge-root]"),
    );
    return targets.length ? targets : [document.body];
  }

  function emit(type, detail) {
    for (const target of bridgeTargets()) {
      target.dispatchEvent(
        new CustomEvent(type, {
          bubbles: true,
          detail,
        }),
      );
    }
  }

  function send(type, payload) {
    window.parent.postMessage(
      {
        type,
        instanceId,
        payload,
      },
      "*",
    );
  }

  function assignDetail(detail) {
    const nextDetail = detail && typeof detail === "object" ? detail : {};
    const nextMeta =
      nextDetail.meta && typeof nextDetail.meta === "object"
        ? nextDetail.meta
        : {};

    lastDetail = {
      bridge:
        nextDetail.bridge && typeof nextDetail.bridge === "object"
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

    if (event.data.type === HOST_TO_WIDGET_STATE) {
      const detail = assignDetail(event.data.payload);
      emit("lince-bridge-state", detail);

      for (const listener of listeners) {
        listener(detail);
      }
      return;
    }

    if (event.data.type === WIDGET_ERROR) {
      emit("lince-bridge-error", event.data.payload || {});
    }
  });

  window.LinceWidgetHost = {
    instanceId,
    print(label) {
      send(WIDGET_ACTION, {
        action: "print",
        label: String(label || "print"),
      });
    },
    requestState() {
      send(WIDGET_READY, {});
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
      send(WIDGET_ACTION, {
        action: "set-card-state",
        state: cloneJsonValue(nextState, {}),
      });
    },
    patchCardState(patch) {
      send(WIDGET_ACTION, {
        action: "patch-card-state",
        patch: cloneJsonValue(patch, {}),
      });
    },
    setStreamsEnabled(enabled) {
      send(WIDGET_ACTION, {
        action: "set-card-streams-enabled",
        enabled: Boolean(enabled),
      });
    },
    invalidateServerAuth(serverId) {
      send(WIDGET_ACTION, {
        action: "invalidate-server-auth",
        serverId: String(serverId || ""),
      });
    },
  };

  send(WIDGET_READY, {});
})();
