// The board holds ONE WebSocket PER ORGAN it is driving, not one in total.
//
// Each sand binds to a host (`card.serverId`): our own Cell, or a contact's,
// reached through our Cell's iroh relay at `/live/{organ}/connect`. Sand A can
// be looking at Organ A while sand B is looking at Organ B, so a single socket
// switched between targets cannot express what the board needs — the previous
// board-wide `setLiveOrgan` did exactly that and is gone.
//
// Everything below the registry is therefore PER CONNECTION: the socket, its
// outbox, its listener sets, and above all its signing session. A session is
// bound to one Person on one Cell, and two connections routinely authenticate
// as different Persons at the same time; sharing any of that state across them
// would sign an Action for one Cell with the identity proved to another.
//
// The remote Cell speaks EXACTLY the protocol our own does — `Session` is
// transport-agnostic and the live relay only pipes frames — so a connection is
// the same code regardless of where it points. Our own Cell does the reaching
// over iroh; the browser never leaves localhost.
//
// The signer: the server binds an authoritative Person to a fresh challenge;
// WebCrypto creates a non-extractable Ed25519 private key and IndexedDB
// preserves the CryptoKey object for that Person across reconnects. Private key
// bytes never cross this module or enter Cell storage. Trusted-local sessions
// are explicitly identified by the server and remain the only sessions allowed
// to send legacy unsigned `act` frames. The key store is keyed by Person, so
// two connections authenticating as different Persons keep separate keys and
// one authenticating as the same Person on a reconnect keeps its own.

const textEncoder = new TextEncoder();
const SIGNING_KEY_DB = "lince-browser-identity";
const SIGNING_KEY_STORE = "person-keys";

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

// One connection to one Cell. `organId` is "" for our own.
// How long to wait before dialling a host again, doubling per failure.
//
// A fixed one-second retry is what a board looks like when a host is simply
// not answering: a connection per second, forever, and a status light that
// flickers green on every upgrade and red again a moment later. Backing off
// keeps recovery quick when the host blinks and quiet when it is gone.
const RECONNECT_BASE_MS = 1000;
const RECONNECT_CAP_MS = 30000;
// How long a session must survive before the backoff counts it as a success.
//
// Clearing the count the moment a session opens would leave a host that lets us
// in and then drops us right back at one dial a second — the same storm, just
// past the handshake. A session has to actually last to earn a prompt retry.
const SESSION_STABLE_MS = 5000;

function createConnection(organId) {
  const target = String(organId || "");
  // A remote host is reached through our own Cell's relay, so the websocket
  // opening proves only that OUR Cell is up. The session is not usable until
  // the relay says the far Cell let us in.
  const remote = Boolean(target);
  let socket = null;
  let ready = false;
  let closed = false; // torn down on purpose; stop reconnecting
  let failures = 0; // dials that did not produce a lasting session
  let retryTimer = null;
  let stableTimer = null; // pending "this session lasted" verdict
  // Set when the host said something only the user can fix — no credential
  // held, or a refused one. Retrying on a timer cannot help, so we stop and
  // wait to be told to try again.
  let stalled = null;
  const outbox = []; // frames queued until the socket opens
  const messageListeners = new Set(); // fn(message) — every consumer sees every frame
  const openListeners = new Set(); // fn() — replay subscriptions / rejoin rooms on (re)open
  const liveListeners = new Set(); // fn(bool) — connection up/down
  const signingListeners = new Set(); // fn(state) — signer availability/identity
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

  // Our own Cell, or a contact's Cell through our own Cell's relay. The uid is
  // interpolated into a path, so it is escaped: an Organ uid that walked the
  // path would have the board driving an endpoint nobody chose.
  function transportWsUrl() {
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const path = target
      ? `/live/${encodeURIComponent(target)}/connect`
      : "/host/transport/ws";
    return `${protocol}//${window.location.host}${path}`;
  }

  // The session is usable: flush what was queued and tell everyone.
  //
  // Deliberately NOT called on the websocket's own `open` for a remote host.
  // Flushing there would push a board's subscriptions into a socket that the
  // relay is about to close, destroying them once per retry; held back, they
  // are still in the outbox when a session finally opens.
  function markReady() {
    if (ready) return;
    ready = true;
    stableTimer = window.setTimeout(() => {
      stableTimer = null;
      if (ready) failures = 0;
    }, SESSION_STABLE_MS);
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
  }

  function connect() {
    if (closed || stalled) return;
    if (retryTimer) {
      window.clearTimeout(retryTimer);
      retryTimer = null;
    }
    socket = new WebSocket(transportWsUrl());

    socket.addEventListener("open", () => {
      if (!remote) {
        markReady();
      }
    });

    socket.addEventListener("message", (event) => {
      let message = null;
      try {
        message = JSON.parse(event.data);
      } catch {
        return;
      }
      // Relay frames. They are between the board and its own Cell, so they are
      // handled here and not passed on to sands.
      if (message?.type === "live_ready") {
        markReady();
        return;
      }
      if (message?.type === "live_unavailable") {
        handleLiveUnavailable(message);
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
      const wasReady = ready;
      socket = null;
      ready = false;
      // A session that ended before it was judged stable counts against the
      // backoff exactly like a dial that never opened. Otherwise a host that
      // admits us and immediately drops us is retried once a second forever —
      // the same storm, one handshake further along.
      if (stableTimer) {
        window.clearTimeout(stableTimer);
        stableTimer = null;
        failures += 1;
      }
      resetSigningSession("The Cell connection closed; signed Actions are paused.");
      // The reset above starts a fresh session that is merely "waiting for a
      // challenge". That is the wrong thing to say when we have already been
      // told why this host will not have us, and it is the only thing a sand
      // has to show for being empty — so put the reason back.
      if (stalled) {
        setSigningUnavailable(
          signingError(stalled.message, stalled.code),
          session.generation,
        );
      }
      // Only announce a drop to consumers that were told it was up. A dial that
      // never became a session was never live, and reporting it as one is what
      // made the status light blink instead of settling on "not connected".
      if (wasReady) {
        for (const fn of liveListeners) {
          try {
            fn(false);
          } catch (error) {
            console.warn("[transport] live listener failed", error);
          }
        }
      } else {
        failures += 1;
      }
      // The Cell session is fresh on every (re)connect; consumers re-establish
      // their subscriptions and lane rooms via their open listeners.
      if (!closed && !stalled) {
        scheduleReconnect();
      }
    });
  }

  // Wait, then dial again — longer after each dial that never became a session.
  function scheduleReconnect() {
    if (retryTimer) return;
    const backoff = Math.min(
      RECONNECT_CAP_MS,
      RECONNECT_BASE_MS * 2 ** Math.max(0, failures - 1),
    );
    // Jitter so a board with several sands on a host that just went down does
    // not dial it in a burst on every round.
    const delay = backoff * (0.5 + Math.random() * 0.5);
    retryTimer = window.setTimeout(() => {
      retryTimer = null;
      connect();
    }, delay);
  }

  // The host said something a timer cannot fix. Stop dialling and say why.
  function handleLiveUnavailable(message) {
    const code = String(message.code || "live_unavailable");
    const text = String(message.message || "That Lince is not answering.");
    if (message.fatal) {
      stalled = { code, message: text };
      if (retryTimer) {
        window.clearTimeout(retryTimer);
        retryTimer = null;
      }
    }
    setSigningUnavailable(signingError(text, code), session.generation);
  }

  function ensureConnected() {
    if (!socket && !closed && !stalled) {
      connect();
    }
  }

  ensureConnected();

  return {
    organ: target,
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
    // Whether this connection has given up, and why.
    //
    // A host that wants a login we do not hold is not "offline" and must not be
    // drawn as such: nothing will change until the user logs in.
    getStall() {
      return stalled ? { ...stalled } : null;
    },
    // Try again now, after the user changed something that could matter — a
    // login, a new binding. Without this, "stop retrying" would read to the
    // user as "logging in does nothing until you reload the page".
    retry() {
      const wasStalled = stalled;
      stalled = null;
      failures = 0;
      if (closed) return;
      if (!socket) {
        connect();
        return;
      }
      // The host's refusal and its close arrive separately, so a stalled
      // connection may still be holding a socket that is on its way out.
      // Closing it here makes the reconnect happen either way.
      if (wasStalled) {
        socket.close();
      }
    },
    // Register a connection up/down handler. Returns an unsubscribe.
    onLive(handler) {
      liveListeners.add(handler);
      return () => liveListeners.delete(handler);
    },
    // Drop this connection for good. Queued frames are discarded rather than
    // redirected: they were addressed to THIS Cell, and delivering them
    // anywhere else would apply an Action to the wrong Organ's store.
    close() {
      closed = true;
      outbox.length = 0;
      if (retryTimer) {
        window.clearTimeout(retryTimer);
        retryTimer = null;
      }
      if (stableTimer) {
        window.clearTimeout(stableTimer);
        stableTimer = null;
      }
      rejectQueuedActions(
        signingError("The host binding changed before this Action was sent.", "host_unbound"),
      );
      if (socket) socket.close();
      socket = null;
      ready = false;
    },
  };
}

// organ id ("" = our own Cell) -> connection. Created on first use and kept:
// a board with three sands on the same host shares one socket to it.
const connections = new Map();

// Our own Organ's uid, as the Cell reports it.
//
// The board's convention is that an EMPTY binding means our own Cell, but the
// local Organ also has a perfectly real uid, and it appears in `/organ` as the
// first profile. A card that ends up holding that uid — from an older build, a
// default that reached for `serverProfiles[0].id`, or a board state saved
// before this was understood — is then treated as REMOTE: every write is sent
// to `/live/{uid}/connect`, which asks `store::organs::contact` for a Cell that
// is not a contact of itself, and every write fails with "not a contact".
//
// Nothing about that is recoverable by the user: the host picker shows "Local
// Lince" selected, because the local uid is not among the remotes it lists, so
// the setting LOOKS right while the card is bound elsewhere.
//
// So the two names for the same Cell are collapsed HERE, at the one function
// every caller goes through, rather than at each of the dozen call sites where
// forgetting one would bring the bug back.
// The same collapsing also has to catch a binding to an Organ this Cell has
// never heard of. Wiping the data directory mints a NEW local Organ uid while
// the board's saved cards still name the old one — and a uid that is neither
// our own nor a contact is dialled as a contact, fails, and shows the picker
// sitting innocently on "Local Lince". A binding nobody can resolve is not a
// remote host; it is a stale note, and our own Cell is the only honest place
// for it to point.
//
// Guarded on `hostsKnown` so this never fires before `/organ` has answered.
// Downgrading an unknown id to local while the list is still empty would send
// a genuinely remote sand's writes into THIS Cell, which is far worse than the
// bug being fixed.
let localOrganId = "";
let remoteOrganIds = new Set();
let hostsKnown = false;

export function setLocalOrganId(organId, remoteIds) {
  localOrganId = String(organId || "");
  if (Array.isArray(remoteIds)) {
    remoteOrganIds = new Set(remoteIds.map((id) => String(id || "")).filter(Boolean));
    hostsKnown = true;
  }
}

function hostKey(organId) {
  const key = String(organId || "");
  if (!key) return "";
  if (key === localOrganId) return "";
  if (hostsKnown && !remoteOrganIds.has(key)) return "";
  return key;
}

// The connection for a host binding, opening one if this is the first sand to
// ask for it.
export function getTransportFor(organId) {
  const key = hostKey(organId);
  let connection = connections.get(key);
  if (!connection) {
    connection = createConnection(key);
    connections.set(key, connection);
  }
  return connection;
}

// Our own Cell. Board chrome that is not a sand — the Data panel, the
// notification feed — talks to this one.
export function getSharedTransport() {
  return getTransportFor("");
}

// Every connection currently open, for consumers that must reach all of them
// (inbound frame fan-out, reconnect replay).
export function listTransports() {
  return [...connections.values()];
}

// Called when a host is no longer bound by any sand.
export function releaseTransport(organId) {
  const key = hostKey(organId);
  if (!key) return; // our own Cell is never dropped, under either of its names
  const connection = connections.get(key);
  if (!connection) return;
  connections.delete(key);
  connection.close();
}
