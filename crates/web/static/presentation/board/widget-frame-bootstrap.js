const HOST_TO_WIDGET_STATE = "lince:bridge-state";
const WIDGET_READY = "lince:widget-ready";
const WIDGET_ACTION = "lince:widget-action";
const WIDGET_ERROR = "lince:bridge-error";
const WIDGET_EVENT = "lince:bridge-event";
const WIDGET_SPACE_PAN = "lince:widget-space-pan";
const PROTEIN_SUBSCRIBE = "lince:protein-subscribe";
const PROTEIN_SUBSCRIBE_SAVED = "lince:protein-subscribe-saved";
const PROTEIN_UNSUBSCRIBE = "lince:protein-unsubscribe";
const PROTEIN_ROWS = "lince:protein-rows";
const PROTEIN_ACTION = "lince:protein-action";
const PROTEIN_ACTION_RESULT = "lince:protein-action-result";

(() => {
  if (window.__LINCE_WIDGET_HOST__) {
    return;
  }

  window.__LINCE_WIDGET_HOST__ = true;

  document.documentElement.style.overscrollBehavior = 'contain';
  document.addEventListener('DOMContentLoaded', () => {
    if (document.body) document.body.style.overscrollBehavior = 'contain';
  });

  const instanceId =
    window.frameElement?.dataset?.packageInstanceId ||
    window.frameElement?.dataset?.packagePreviewId ||
    "preview";
  const listeners = new Set();
  const eventHandlers = new Set();
  const proteinHandlers = new Map();
  const pendingActions = new Map();
  let nextActionRequestId = 1;
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

  function isTypingTarget(target) {
    return Boolean(
      target?.closest?.("input, textarea, select, [contenteditable='true']"),
    );
  }

  function setHostSpacePan(enabled) {
    send(WIDGET_SPACE_PAN, {
      enabled: Boolean(enabled),
    });
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
        viewName: String(nextMeta.viewName || ""),
        cardState: cloneJsonValue(nextMeta.cardState, {}),
        shell: cloneJsonValue(nextMeta.shell, {}),
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

    if (event.data.type === WIDGET_EVENT) {
      const payload = event.data.payload || {};
      emit("lince-bridge-event", payload);

      for (const entry of eventHandlers) {
        if (!entry.topic || entry.topic === payload.topic) {
          entry.handler(cloneJsonValue(payload, {}));
        }
      }
      return;
    }

    if (event.data.type === WIDGET_ERROR) {
      emit("lince-bridge-error", event.data.payload || {});
      for (const pending of pendingActions.values()) {
        pending.reject(
          new Error(event.data.payload?.message || "Lince transport error."),
        );
      }
      pendingActions.clear();
      return;
    }

    if (event.data.type === PROTEIN_ROWS) {
      const payload = event.data.payload || {};
      const subId = String(payload.subId || "");
      const handler = proteinHandlers.get(subId);
      if (handler) {
        handler({
          rows: cloneJsonValue(payload.rows, []),
          live: payload.live !== false,
        });
      }
      return;
    }

    if (event.data.type === PROTEIN_ACTION_RESULT) {
      const payload = event.data.payload || {};
      const reqId = String(payload.reqId || "");
      const pending = pendingActions.get(reqId);
      if (!pending) {
        return;
      }
      pendingActions.delete(reqId);
      if (payload.ok) {
        pending.resolve({
          created: payload.created || null,
          facts: Number(payload.facts) || 0,
        });
      } else {
        pending.reject(new Error(payload.message || "Action failed."));
      }
    }
  });

  window.addEventListener("keydown", (event) => {
    if (event.code !== "Space" || isTypingTarget(event.target)) {
      return;
    }

    event.preventDefault();
    setHostSpacePan(true);
  });

  window.addEventListener("keyup", (event) => {
    if (event.code === "Space") {
      setHostSpacePan(false);
    }
  });

  window.addEventListener("blur", () => {
    setHostSpacePan(false);
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
    subscribeProtein(subId, protein, handler) {
      const id = String(subId || "default");
      if (typeof handler !== "function") {
        return () => {};
      }
      proteinHandlers.set(id, handler);
      send(PROTEIN_SUBSCRIBE, {
        subId: id,
        protein: cloneJsonValue(protein, {}),
      });
      return () => {
        proteinHandlers.delete(id);
        send(PROTEIN_UNSUBSCRIBE, { subId: id });
      };
    },
    subscribeSaved(subId, name, handler) {
      const id = String(subId || "default");
      if (typeof handler !== "function") {
        return () => {};
      }
      proteinHandlers.set(id, handler);
      send(PROTEIN_SUBSCRIBE_SAVED, {
        subId: id,
        name: String(name || ""),
      });
      return () => {
        proteinHandlers.delete(id);
        send(PROTEIN_UNSUBSCRIBE, { subId: id });
      };
    },
    act(action) {
      const reqId = String(nextActionRequestId++);
      return new Promise((resolve, reject) => {
        pendingActions.set(reqId, { resolve, reject });
        window.setTimeout(() => {
          if (!pendingActions.has(reqId)) {
            return;
          }
          pendingActions.delete(reqId);
          reject(new Error("Action timed out."));
        }, 15000);
        send(PROTEIN_ACTION, {
          reqId,
          action: cloneJsonValue(action, {}),
        });
      });
    },
    invalidateServerAuth(serverId) {
      send(WIDGET_ACTION, {
        action: "invalidate-server-auth",
        serverId: String(serverId || ""),
      });
    },
    shell(command, payload = {}) {
      send(WIDGET_ACTION, {
        action: "shell-action",
        command: String(command || ""),
        payload: cloneJsonValue(payload, {}),
      });
    },
    emit(topic, data) {
      send(WIDGET_ACTION, {
        action: "emit-event",
        topic: String(topic || ""),
        data: cloneJsonValue(data, null),
      });
    },
    onEvent(topic, handler) {
      if (typeof handler !== "function") {
        return () => {};
      }

      const entry = { topic: String(topic || ""), handler };
      eventHandlers.add(entry);
      return () => eventHandlers.delete(entry);
    },
  };

  send(WIDGET_READY, {});
})();
