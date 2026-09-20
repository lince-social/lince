export { default } from "./loro_wasm.js";
import { LoroDoc, EphemeralStoreWasm, UndoManager, AwarenessWasm, callPendingEvents } from "./loro_wasm.js";
export * from "./loro_wasm.js";

/**
 * @deprecated Please use LoroDoc
 */
class Loro extends LoroDoc {
}
const CONTAINER_TYPES = [
    "Map",
    "Text",
    "List",
    "Tree",
    "MovableList",
    "Counter",
];
function isContainerId(s) {
    return s.startsWith("cid:");
}
/**  Whether the value is a container.
 *
 * # Example
 *
 * ```ts
 * const doc = new LoroDoc();
 * const map = doc.getMap("map");
 * const list = doc.getList("list");
 * const text = doc.getText("text");
 * isContainer(map); // true
 * isContainer(list); // true
 * isContainer(text); // true
 * isContainer(123); // false
 * isContainer("123"); // false
 * isContainer({}); // false
 * ```
 */
function isContainer(value) {
    if (typeof value !== "object" || value == null) {
        return false;
    }
    const p = Object.getPrototypeOf(value);
    if (p == null || typeof p !== "object" || typeof p["kind"] !== "function") {
        return false;
    }
    return CONTAINER_TYPES.includes(value.kind());
}
/**  Get the type of a value that may be a container.
 *
 * # Example
 *
 * ```ts
 * const doc = new LoroDoc();
 * const map = doc.getMap("map");
 * const list = doc.getList("list");
 * const text = doc.getText("text");
 * getType(map); // "Map"
 * getType(list); // "List"
 * getType(text); // "Text"
 * getType(123); // "Json"
 * getType("123"); // "Json"
 * getType({}); // "Json"
 * ```
 */
function getType(value) {
    if (isContainer(value)) {
        return value.kind();
    }
    return "Json";
}
function newContainerID(id, type) {
    return `cid:${id.counter}@${id.peer}:${type}`;
}
function newRootContainerID(name, type) {
    return `cid:root-${name}:${type}`;
}
/**
 * @deprecated Please use `EphemeralStore` instead.
 *
 * Awareness is a structure that allows to track the ephemeral state of the peers.
 *
 * If we don't receive a state update from a peer within the timeout, we will remove their state.
 * The timeout is in milliseconds. This can be used to handle the offline state of a peer.
 */
class Awareness {
    constructor(peer, timeout = 30000) {
        this.listeners = new Set();
        this.inner = new AwarenessWasm(peer, timeout);
        this.peer = peer;
        this.timeout = timeout;
    }
    apply(bytes, origin = "remote") {
        const { updated, added } = this.inner.apply(bytes);
        this.listeners.forEach((listener) => {
            listener({ updated, added, removed: [] }, origin);
        });
        this.startTimerIfNotEmpty();
    }
    setLocalState(state) {
        const wasEmpty = this.inner.getState(this.peer) == null;
        this.inner.setLocalState(state);
        if (wasEmpty) {
            this.listeners.forEach((listener) => {
                listener({ updated: [], added: [this.inner.peer()], removed: [] }, "local");
            });
        }
        else {
            this.listeners.forEach((listener) => {
                listener({ updated: [this.inner.peer()], added: [], removed: [] }, "local");
            });
        }
        this.startTimerIfNotEmpty();
    }
    getLocalState() {
        return this.inner.getState(this.peer);
    }
    getAllStates() {
        return this.inner.getAllStates();
    }
    encode(peers) {
        return this.inner.encode(peers);
    }
    encodeAll() {
        return this.inner.encodeAll();
    }
    addListener(listener) {
        this.listeners.add(listener);
    }
    removeListener(listener) {
        this.listeners.delete(listener);
    }
    peers() {
        return this.inner.peers();
    }
    destroy() {
        clearInterval(this.timer);
        this.listeners.clear();
    }
    startTimerIfNotEmpty() {
        if (this.inner.isEmpty() || this.timer != null) {
            return;
        }
        this.timer = setInterval(() => {
            const removed = this.inner.removeOutdated();
            if (removed.length > 0) {
                this.listeners.forEach((listener) => {
                    listener({ updated: [], added: [], removed }, "timeout");
                });
            }
            if (this.inner.isEmpty()) {
                clearInterval(this.timer);
                this.timer = undefined;
            }
        }, this.timeout / 2);
    }
}
/**
 * EphemeralStore tracks ephemeral key-value state across peers.
 *
 * - Use it for lightweight presence/state like cursors, selections, and UI hints.
 * - Conflict resolution is timestamp-based LWW (Last-Write-Wins) per key.
 * - Timeout unit: milliseconds.
 * - After timeout: keys are considered expired. They are omitted from
 *   `encode(key)`, `encodeAll()` and `getAllStates()`. A periodic cleanup runs
 *   while the store is non-empty and removes expired keys; when removals happen
 *   subscribers receive an event with `by: "timeout"` and the `removed` keys.
 *
 * See: https://loro.dev/docs/tutorial/ephemeral
 *
 * @param timeout Inactivity timeout in milliseconds (default: 30000). If a key
 * doesn't receive updates within this duration, it will expire and be removed
 * on the next cleanup tick.
 *
 * @example
 * ```ts
 * const store = new EphemeralStore();
 * const store2 = new EphemeralStore();
 * // Subscribe to local updates and forward over the wire
 * store.subscribeLocalUpdates((data) => {
 *   store2.apply(data);
 * });
 * // Subscribe to all updates (including removals by timeout)
 * store2.subscribe((event) => {
 *   console.log("event:", event);
 * });
 * // Set a value
 * store.set("key", "value");
 * // Encode the value
 * const encoded = store.encode("key");
 * // Apply the encoded value
 * store2.apply(encoded);
 * ```
 */
class EphemeralStore {
    constructor(timeout = 30000) {
        this.inner = new EphemeralStoreWasm(timeout);
        this.timeout = timeout;
    }
    apply(bytes) {
        this.inner.apply(bytes);
        this.startTimerIfNotEmpty();
    }
    set(key, value) {
        this.inner.set(key, value);
        this.startTimerIfNotEmpty();
    }
    delete(key) {
        this.inner.delete(key);
    }
    get(key) {
        return this.inner.get(key);
    }
    getAllStates() {
        return this.inner.getAllStates();
    }
    encode(key) {
        return this.inner.encode(key);
    }
    encodeAll() {
        return this.inner.encodeAll();
    }
    keys() {
        return this.inner.keys();
    }
    destroy() {
        clearInterval(this.timer);
    }
    subscribe(listener) {
        return this.inner.subscribe(listener);
    }
    subscribeLocalUpdates(listener) {
        return this.inner.subscribeLocalUpdates(listener);
    }
    startTimerIfNotEmpty() {
        if (this.inner.isEmpty() || this.timer != null) {
            return;
        }
        this.timer = setInterval(() => {
            this.inner.removeOutdated();
            if (this.inner.isEmpty()) {
                clearInterval(this.timer);
                this.timer = undefined;
            }
        }, this.timeout / 2);
    }
}
LoroDoc.prototype.toJsonWithReplacer = function (replacer) {
    const processed = new Set();
    const doc = this;
    const m = (key, value) => {
        if (typeof value === "string") {
            if (isContainerId(value) && !processed.has(value)) {
                processed.add(value);
                const container = doc.getContainerById(value);
                if (container == null) {
                    throw new Error(`ContainerID not found: ${value}`);
                }
                const ans = replacer(key, container);
                if (ans === container) {
                    const ans = container.getShallowValue();
                    if (typeof ans === "object") {
                        return run(ans);
                    }
                    return ans;
                }
                if (isContainer(ans)) {
                    throw new Error("Using new container is not allowed in toJsonWithReplacer");
                }
                if (typeof ans === "object" && ans != null) {
                    return run(ans);
                }
                return ans;
            }
        }
        if (typeof value === "object" && value != null) {
            return run(value);
        }
        const ans = replacer(key, value);
        if (isContainer(ans)) {
            throw new Error("Using new container is not allowed in toJsonWithReplacer");
        }
        return ans;
    };
    const run = (layer) => {
        if (Array.isArray(layer)) {
            return layer
                .map((item, index) => {
                return m(index, item);
            })
                .filter((item) => item !== undefined);
        }
        const result = {};
        for (const [key, value] of Object.entries(layer)) {
            const ans = m(key, value);
            if (ans !== undefined) {
                result[key] = ans;
            }
        }
        return result;
    };
    const layer = doc.getShallowValue();
    return run(layer);
};
function idStrToId(idStr) {
    const [counter, peer] = idStr.split("@");
    return {
        counter: parseInt(counter),
        peer: peer,
    };
}
const CALL_PENDING_EVENTS_WRAPPED = Symbol("loro.callPendingEventsWrapped");
function decorateMethod(prototype, method) {
    const descriptor = Object.getOwnPropertyDescriptor(prototype, method);
    if (!descriptor || typeof descriptor.value !== "function") {
        return;
    }
    const original = descriptor.value;
    if (original[CALL_PENDING_EVENTS_WRAPPED]) {
        return;
    }
    const wrapped = function (...args) {
        let result;
        try {
            result = original.apply(this, args);
            return result;
        }
        finally {
            if (result && typeof result.then === "function") {
                result.finally(() => {
                    callPendingEvents();
                });
            }
            else {
                callPendingEvents();
            }
        }
    };
    wrapped[CALL_PENDING_EVENTS_WRAPPED] = true;
    Object.defineProperty(prototype, method, {
        ...descriptor,
        value: wrapped,
    });
}
function decorateMethods(prototype, methods) {
    for (const method of methods) {
        decorateMethod(prototype, method);
    }
}
decorateMethods(LoroDoc.prototype, [
    "setDetachedEditing",
    "attach",
    "detach",
    "fork",
    "forkAt",
    "checkoutToLatest",
    "checkout",
    "commit",
    "getCursorPos",
    "revertTo",
    "export",
    "exportJsonUpdates",
    "exportJsonInIdSpan",
    "importJsonUpdates",
    "import",
    "importUpdateBatch",
    "importBatch",
    "travelChangeAncestors",
    "getChangedContainersIn",
    "diff",
    "applyDiff",
    "setPeerId",
]);
decorateMethods(EphemeralStoreWasm.prototype, [
    "set",
    "delete",
    "apply",
    "removeOutdated",
]);
decorateMethods(UndoManager.prototype, ["undo", "redo"]);

export { Awareness, EphemeralStore, Loro, getType, idStrToId, isContainer, isContainerId, newContainerID, newRootContainerID };
// loro-explicit-wasm-reexports
export {
  AwarenessWasm,
  ChangeModifier,
  Cursor,
  EphemeralStoreWasm,
  LORO_VERSION,
  LoroCounter,
  LoroDoc,
  LoroList,
  LoroMap,
  LoroMovableList,
  LoroText,
  LoroTree,
  LoroTreeNode,
  UndoManager,
  VersionVector,
  callPendingEvents,
  decodeFrontiers,
  decodeImportBlobMeta,
  encodeFrontiers,
  initSync,
  redactJsonUpdates,
  run,
  setDebug,
} from "./loro_wasm.js";
//# sourceMappingURL=index.js.map
