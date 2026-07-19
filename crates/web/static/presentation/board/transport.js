// The board owns EXACTLY ONE WebSocket to the Cell transport
// (`/host/transport/ws`). Every board-side consumer — the unified widget bridge
// (new-way sands + legacy chrome) and the Data-panel Protein config — multiplexes
// over this single connection instead of opening its own socket (Stage 8b, base
// task 1: "merge the two WebSockets into one").
//
// The socket also owns the authenticated session signer. The server binds an
// authoritative Person to a fresh challenge; WebCrypto creates a non-
// extractable Ed25519 private key and IndexedDB preserves the CryptoKey object
// for that Person across reconnects. Private key bytes never cross this module
// or enter Cell storage. Trusted-local sessions are explicitly identified by
// the server and remain the only sessions allowed to send legacy unsigned
// `act` frames.

let socket = null;
let ready = false;
const outbox = []; // frames queued until the socket opens
const messageListeners = new Set(); // fn(message) — every consumer sees every frame
const openListeners = new Set(); // fn() — replay subscriptions / rejoin rooms on (re)open
const liveListeners = new Set(); // fn(bool) — connection up/down
const signingListeners = new Set(); // fn(state) — signer availability/identity

const textEncoder = new TextEncoder();
const SIGNING_KEY_DB = "lince-browser-identity";
const SIGNING_KEY_STORE = "person-keys";
let sessionGeneration = 0;
let session = freshSession();

function freshSession() {
  return {
    generation: sessionGeneration,
    sessionId: "",
    person: null,
    required: null,
    status: "connecting",
    code: "session_challenge_pending",
    message: "Waiting for the Cell signing challenge.",
    keyPair: null,
    keyId: "",
    challenge: "",
    sequence: 0,
    authRequestId: "",
    queuedActions: [],
    signingChain: Promise.resolve(),
  };
}

function cloneSigningState() {
  return Object.freeze({
    status: session.status,
    available:
      session.status === "authenticated" || session.status === "trusted-local",
    required: session.required,
    person: session.person,
    code: session.code,
    message: session.message,
  });
}

function publishSigningState() {
  const state = cloneSigningState();
  for (const fn of signingListeners) {
    try {
      fn(state);
    } catch (error) {
      console.warn("[transport] signing listener failed", error);
    }
  }
}

function signingError(message, code = "session_signing_unavailable") {
  const error = new Error(message);
  error.code = code;
  return error;
}

function bytesToBase64(value) {
  const bytes = value instanceof Uint8Array ? value : new Uint8Array(value);
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  }
  return window.btoa(binary);
}

function sendNow(message) {
  if (!ready || !socket || socket.readyState !== WebSocket.OPEN) {
    throw signingError(
      "The Cell connection closed before the signed action was sent.",
      "session_disconnected",
    );
  }
  socket.send(JSON.stringify(message));
}

function rejectQueuedActions(error) {
  for (const entry of session.queuedActions.splice(0)) {
    entry.reject(error);
  }
}

function resetSigningSession(message = "Waiting for the Cell signing challenge.") {
  rejectQueuedActions(signingError(message, "session_disconnected"));
  sessionGeneration += 1;
  session = freshSession();
  publishSigningState();
}

function requestResult(request) {
  return new Promise((resolve, reject) => {
    request.addEventListener("success", () => resolve(request.result), { once: true });
    request.addEventListener("error", () => reject(request.error), { once: true });
  });
}

function transactionDone(transaction) {
  return new Promise((resolve, reject) => {
    transaction.addEventListener("complete", resolve, { once: true });
    transaction.addEventListener(
      "abort",
      () => reject(transaction.error || new Error("key storage transaction aborted")),
      { once: true },
    );
    transaction.addEventListener(
      "error",
      () => reject(transaction.error || new Error("key storage transaction failed")),
      { once: true },
    );
  });
}

async function signingKeyDatabase() {
  if (!window.indexedDB) {
    throw signingError(
      "Authenticated Actions require browser protected-key storage.",
      "signing_key_storage_unavailable",
    );
  }
  const request = window.indexedDB.open(SIGNING_KEY_DB, 1);
  request.addEventListener("upgradeneeded", () => {
    const db = request.result;
    if (!db.objectStoreNames.contains(SIGNING_KEY_STORE)) {
      db.createObjectStore(SIGNING_KEY_STORE, { keyPath: "person" });
    }
  });
  try {
    return await requestResult(request);
  } catch (cause) {
    throw signingError(
      `The browser could not open protected-key storage${cause?.message ? `: ${cause.message}` : "."}`,
      "signing_key_storage_unavailable",
    );
  }
}

function validStoredKey(record, person) {
  return Boolean(
    record?.person === person &&
    record.privateKey instanceof CryptoKey &&
    record.publicKey instanceof CryptoKey &&
    record.privateKey.type === "private" &&
    record.privateKey.extractable === false &&
    record.privateKey.algorithm?.name === "Ed25519" &&
    record.privateKey.usages.includes("sign") &&
    record.publicKey.type === "public" &&
    record.publicKey.algorithm?.name === "Ed25519" &&
    record.publicKey.usages.includes("verify")
  );
}

async function loadSigningKey(person) {
  const db = await signingKeyDatabase();
  try {
    const transaction = db.transaction(SIGNING_KEY_STORE, "readonly");
    const record = await requestResult(
      transaction.objectStore(SIGNING_KEY_STORE).get(person),
    );
    return validStoredKey(record, person)
      ? { privateKey: record.privateKey, publicKey: record.publicKey }
      : null;
  } finally {
    db.close();
  }
}

async function storeSigningKey(person, keyPair) {
  const db = await signingKeyDatabase();
  try {
    const transaction = db.transaction(SIGNING_KEY_STORE, "readwrite");
    const done = transactionDone(transaction);
    await requestResult(
      transaction.objectStore(SIGNING_KEY_STORE).put({
        person,
        privateKey: keyPair.privateKey,
        publicKey: keyPair.publicKey,
      }),
    );
    await done;
  } catch (cause) {
    throw signingError(
      `The browser could not preserve the non-extractable signing key${cause?.message ? `: ${cause.message}` : "."}`,
      "signing_key_storage_unavailable",
    );
  } finally {
    db.close();
  }
}

async function sessionKeyFor(person) {
  if (!window.isSecureContext || !window.crypto?.subtle || typeof CryptoKey === "undefined") {
    throw signingError(
      "Authenticated Actions require WebCrypto in a secure browser context.",
      "webcrypto_unavailable",
    );
  }
  try {
    const stored = await loadSigningKey(person);
    if (stored) return stored;
    const created = await window.crypto.subtle.generateKey(
      { name: "Ed25519" },
      false,
      ["sign", "verify"],
    );
    await storeSigningKey(person, created);
    // Read it back once. This verifies that the browser can structured-clone
    // the non-extractable private CryptoKey instead of only accepting the put.
    const restored = await loadSigningKey(person);
    if (!restored) {
      throw signingError(
        "The browser did not preserve the non-extractable signing key.",
        "signing_key_storage_unavailable",
      );
    }
    return restored;
  } catch (cause) {
    if (cause?.code) throw cause;
    throw signingError(
      `This browser cannot use a non-extractable Ed25519 signing key${cause?.message ? `: ${cause.message}` : "."}`,
      "ed25519_unavailable",
    );
  }
}

async function signBytes(privateKey, value) {
  return bytesToBase64(
    await window.crypto.subtle.sign("Ed25519", privateKey, value),
  );
}

function canonicalBytes(...values) {
  return textEncoder.encode(values.map(String).join("\n"));
}

async function keyIdFor(publicKeyBytes) {
  const digest = new Uint8Array(
    await window.crypto.subtle.digest("SHA-256", publicKeyBytes),
  );
  return `web-ed25519-sha256:${bytesToBase64(digest)
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/, "")}`;
}

function setSigningUnavailable(error, generation) {
  if (generation !== session.generation) return;
  session.status = "unavailable";
  session.code = String(error?.code || "session_signing_unavailable");
  session.message = String(error?.message || "Session signing is unavailable.");
  rejectQueuedActions(signingError(session.message, session.code));
  publishSigningState();
}

async function authenticateSession(message) {
  const generation = session.generation;
  if (!message.signing_required) {
    session.status = "trusted-local";
    session.code = "";
    session.message = "Trusted local Actions are enabled.";
    publishSigningState();
    drainQueuedActions();
    return;
  }

  if (!session.person) {
    setSigningUnavailable(
      signingError(
        "The authenticated user is not mapped to a Person, so signed Actions are unavailable.",
        "session_person_unmapped",
      ),
      generation,
    );
    return;
  }

  session.status = "authenticating";
  session.code = "session_authenticating";
  session.message = `Preparing signed Actions for ${session.person}.`;
  publishSigningState();

  try {
    const keyPair = await sessionKeyFor(session.person);
    if (generation !== session.generation) return;
    const publicKeyBytes = await window.crypto.subtle.exportKey("raw", keyPair.publicKey);
    const publicKey = bytesToBase64(publicKeyBytes);
    const keyId = await keyIdFor(publicKeyBytes);
    const proof = canonicalBytes(
      "lince.action-intent-session.v1",
      session.sessionId,
      session.challenge,
      session.person,
      keyId,
      publicKey,
    );
    const signature = await signBytes(keyPair.privateKey, proof);
    if (generation !== session.generation) return;

    session.keyPair = keyPair;
    session.keyId = keyId;
    session.authRequestId = `session-auth:${session.sessionId}`;
    sendNow({
      type: "session_authenticate",
      id: session.authRequestId,
      session_id: session.sessionId,
      session_challenge: session.challenge,
      person_uid: session.person,
      key_id: keyId,
      public_key_base64: publicKey,
      signature,
    });
  } catch (error) {
    setSigningUnavailable(error, generation);
  }
}

function handleSessionChallenge(message) {
  rejectQueuedActions(
    signingError("The Cell replaced the signing session.", "session_replaced"),
  );
  sessionGeneration += 1;
  session = freshSession();
  session.sessionId = String(message.session_id || "");
  session.challenge = String(message.challenge || "");
  session.person = message.person == null ? null : String(message.person);
  session.required =
    typeof message.signing_required === "boolean"
      ? message.signing_required
      : null;

  if (
    !session.sessionId ||
    !session.challenge ||
    session.required == null ||
    String(message.algorithm || "").toLowerCase() !== "ed25519"
  ) {
    setSigningUnavailable(
      signingError("The Cell sent an invalid signing challenge.", "invalid_session_challenge"),
      session.generation,
    );
    return;
  }
  void authenticateSession(message);
}

function handleSessionAuthenticated(message) {
  if (
    session.status !== "authenticating" ||
    String(message.id || "") !== session.authRequestId ||
    String(message.session_id || "") !== session.sessionId ||
    String(message.person || "") !== session.person ||
    String(message.key_id || "") !== session.keyId
  ) {
    return false;
  }
  session.status = "authenticated";
  session.code = "";
  session.message = `Signed Actions are bound to ${session.person}.`;
  session.sequence = 0;
  publishSigningState();
  drainQueuedActions();
  return true;
}

async function sendSignedAction(id, action, generation) {
  if (generation !== session.generation || session.status !== "authenticated") {
    throw signingError("The signing session changed before the Action was signed.", "session_replaced");
  }
  if (/\r|\n/.test(id)) {
    throw signingError("Action request ids cannot contain line breaks.", "action_intent_invalid");
  }
  const sequence = session.sequence + 1;
  const actionJson = JSON.stringify(action);
  if (typeof actionJson !== "string") {
    throw signingError("The Action is not a JSON value.", "action_intent_invalid");
  }
  const actionBytes = textEncoder.encode(actionJson);
  if (actionBytes.byteLength > 1_048_576) {
    throw signingError("The Action exceeds the 1 MiB signed payload limit.", "action_intent_invalid");
  }
  const actionBase64 = bytesToBase64(actionBytes);
  const intent = canonicalBytes(
    "lince.action-intent.v1",
    session.sessionId,
    session.challenge,
    sequence,
    id,
    actionBase64,
  );
  const signature = await signBytes(session.keyPair.privateKey, intent);
  if (generation !== session.generation || session.status !== "authenticated") {
    throw signingError("The signing session changed before the Action was sent.", "session_replaced");
  }
  sendNow({
    type: "signed_act",
    id,
    session_id: session.sessionId,
    session_challenge: session.challenge,
    sequence,
    action_base64: actionBase64,
    signature,
  });
  session.sequence = sequence;
}

function dispatchAction(entry) {
  if (session.status === "trusted-local") {
    try {
      sendNow({ type: "act", id: entry.id, action: entry.action });
      entry.resolve();
    } catch (error) {
      entry.reject(error);
    }
    return;
  }
  if (session.status !== "authenticated") {
    entry.reject(signingError(session.message, session.code));
    return;
  }

  const generation = session.generation;
  session.signingChain = session.signingChain
    .then(() => sendSignedAction(entry.id, entry.action, generation))
    .then(entry.resolve, entry.reject);
}

function drainQueuedActions() {
  for (const entry of session.queuedActions.splice(0)) {
    dispatchAction(entry);
  }
}

function enqueueAction(id, action) {
  return new Promise((resolve, reject) => {
    const entry = { id: String(id), action, resolve, reject };
    if (session.status === "connecting" || session.status === "authenticating") {
      session.queuedActions.push(entry);
      return;
    }
    if (session.status === "unavailable") {
      reject(signingError(session.message, session.code));
      return;
    }
    dispatchAction(entry);
  });
}

function transportWsUrl() {
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${window.location.host}/host/transport/ws`;
}

function connect() {
  socket = new WebSocket(transportWsUrl());

  socket.addEventListener("open", () => {
    ready = true;
    while (outbox.length) {
      socket.send(JSON.stringify(outbox.shift()));
    }
    for (const fn of openListeners) {
      try {
        fn();
      } catch (error) {
        console.warn("[transport] open listener failed", error);
      }
    }
    for (const fn of liveListeners) {
      try {
        fn(true);
      } catch (error) {
        console.warn("[transport] live listener failed", error);
      }
    }
  });

  socket.addEventListener("message", (event) => {
    let message = null;
    try {
      message = JSON.parse(event.data);
    } catch {
      return;
    }
    if (message?.type === "session_challenge") {
      handleSessionChallenge(message);
      return;
    }
    if (message?.type === "session_authenticated") {
      if (handleSessionAuthenticated(message)) return;
    }
    if (
      message?.type === "error" &&
      session.status === "authenticating" &&
      String(message.id || "") === session.authRequestId
    ) {
      setSigningUnavailable(
        signingError(
          String(message.message || "The Cell rejected session signing."),
          String(message.code || "session_authentication_failed"),
        ),
        session.generation,
      );
      return;
    }
    for (const fn of messageListeners) {
      try {
        fn(message);
      } catch (error) {
        console.warn("[transport] message listener failed", error);
      }
    }
  });

  socket.addEventListener("close", () => {
    socket = null;
    ready = false;
    resetSigningSession("The Cell connection closed; signed Actions are paused.");
    for (const fn of liveListeners) {
      try {
        fn(false);
      } catch (error) {
        console.warn("[transport] live listener failed", error);
      }
    }
    // The Cell session is fresh on every (re)connect; consumers re-establish
    // their subscriptions and lane rooms via their open listeners.
    window.setTimeout(connect, 1000);
  });
}

function ensureConnected() {
  if (!socket) {
    connect();
  }
}

let sharedTransport = null;

// Returns the process-wide shared transport handle. Idempotent — every caller
// gets the same underlying socket.
export function getSharedTransport() {
  if (sharedTransport) {
    return sharedTransport;
  }

  ensureConnected();

  sharedTransport = {
    isReady() {
      return ready;
    },
    // Send a frame, queuing it until the socket is open.
    send(message) {
      ensureConnected();
      if (ready && socket && socket.readyState === WebSocket.OPEN) {
        socket.send(JSON.stringify(message));
      } else {
        outbox.push(message);
      }
    },
    // Actions have a stricter path than subscriptions and ephemeral messages:
    // authenticated sessions sign a canonical intent; only a server-declared
    // trusted-local session may emit the legacy unsigned frame.
    sendAction(id, action) {
      ensureConnected();
      return enqueueAction(id, action);
    },
    getSigningState() {
      return cloneSigningState();
    },
    onSigningState(handler) {
      signingListeners.add(handler);
      handler(cloneSigningState());
      return () => signingListeners.delete(handler);
    },
    // Register a handler for every inbound server frame. Returns an unsubscribe.
    onMessage(handler) {
      messageListeners.add(handler);
      return () => messageListeners.delete(handler);
    },
    // Register a handler fired on every (re)open so a consumer can replay its
    // subscriptions and rejoin its lane rooms. Returns an unsubscribe.
    onOpen(handler) {
      openListeners.add(handler);
      return () => openListeners.delete(handler);
    },
    // Register a connection up/down handler. Returns an unsubscribe.
    onLive(handler) {
      liveListeners.add(handler);
      return () => liveListeners.delete(handler);
    },
  };

  return sharedTransport;
}
