// The board owns EXACTLY ONE WebSocket to the Cell transport
// (`/host/transport/ws`). Every board-side consumer — the unified widget bridge
// (new-way sands + legacy chrome) and the Data-panel Protein config — multiplexes
// over this single connection instead of opening its own socket (Stage 8b, base
// task 1: "merge the two WebSockets into one").
//
// The wire envelope is identical for every consumer (bare `new WebSocket`, no
// auth handshake; `{type:subscribe|act|lane_join|lane_send,...}` out;
// `{type:snapshot|update|action_ok|error|lane_event,...}` back), so a single
// socket with fan-out message listeners is enough: each consumer filters the
// messages it owns by subscription/request `id` (ids never collide across
// consumers) and by lane `room`.

let socket = null;
let ready = false;
const outbox = []; // frames queued until the socket opens
const messageListeners = new Set(); // fn(message) — every consumer sees every frame
const openListeners = new Set(); // fn() — replay subscriptions / rejoin rooms on (re)open
const liveListeners = new Set(); // fn(bool) — connection up/down

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
