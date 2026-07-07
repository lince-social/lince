// The board-side of the re-pointed widget bridge (blueprint VII.4, Stage 8b).
//
// The parent board owns exactly ONE WebSocket to /ws. Every sand runs in an
// iframe and never opens its own socket; it talks to this bridge over
// postMessage, and this bridge multiplexes each sand's Protein subscriptions
// and Actions onto the single transport connection.
//
// This is the whole migration's linchpin: it replaces the old SSE-view +
// /host table-CRUD data plane with Protein (reads) + Actions (writes), while
// leaving the sand's control plane (host metadata, cardState, ABI events)
// intact. See docs/stage-8b-web-sand-migration.md §3.

export function createBridge(options = {}) {
  const onLive = typeof options.onLive === "function" ? options.onLive : null;
  let ws = null;
  let ready = false;
  const outbox = []; // messages queued until the socket opens

  // wsSubId ("<instanceId>:<subId>") -> { win, subId } so an incoming
  // snapshot/update can be routed back to the iframe that asked for it.
  const subs = new Map();
  // reqId -> { win } so an ActionOk/Error can be routed back.
  const acts = new Map();
  // instanceId -> Window, registered when a frame announces itself ready.
  const frames = new Map();

  let reqSeq = 0;
  const nextReqId = () => `r${++reqSeq}`;

  function connect() {
    const proto = location.protocol === "https:" ? "wss" : "ws";
    ws = new WebSocket(`${proto}://${location.host}/ws`);
    ws.onopen = () => {
      ready = true;
      while (outbox.length) ws.send(JSON.stringify(outbox.shift()));
      broadcastLive(true);
    };
    ws.onclose = () => {
      ready = false;
      broadcastLive(false);
      setTimeout(connect, 1000);
    };
    ws.onmessage = (e) => onServerMessage(JSON.parse(e.data));
  }

  function send(msg) {
    if (ready && ws) ws.send(JSON.stringify(msg));
    else outbox.push(msg);
  }

  function onServerMessage(msg) {
    if (msg.type === "snapshot" || msg.type === "update") {
      const target = subs.get(msg.id);
      if (target) postToFrame(target.win, { type: "lince:protein-rows", subId: target.subId, rows: msg.rows });
    } else if (msg.type === "action_ok") {
      const target = acts.get(msg.id);
      if (target) {
        postToFrame(target.win, { type: "lince:action-result", reqId: target.frameReqId, ok: true, created: msg.created, facts: msg.facts });
        acts.delete(msg.id);
      }
    } else if (msg.type === "error") {
      const s = subs.get(msg.id);
      const a = acts.get(msg.id);
      if (a) { postToFrame(a.win, { type: "lince:action-result", reqId: a.frameReqId, ok: false, message: msg.message }); acts.delete(msg.id); }
      if (s) postToFrame(s.win, { type: "lince:protein-error", subId: s.subId, message: msg.message });
    } else if (msg.type === "lane_event") {
      // Sand-to-sand ABI events ride ephemeral lanes, never the Ledger.
      for (const win of frames.values()) postToFrame(win, { type: "lince:lane-event", room: msg.room, from: msg.from, payload: msg.payload });
    }
  }

  function postToFrame(win, payload) {
    win?.postMessage(payload, "*");
  }

  function broadcastLive(live) {
    for (const win of frames.values()) postToFrame(win, { type: "lince:live", live });
    if (onLive) onLive(live); // notify the board chrome (parent), not just sands
  }

  // Messages coming up from the sand iframes.
  function onFrameMessage(event) {
    const data = event.data;
    if (!data || typeof data !== "object") return;
    const win = event.source;

    switch (data.type) {
      case "lince:ready": {
        if (data.instanceId) frames.set(data.instanceId, win);
        postToFrame(win, { type: "lince:live", live: ready });
        break;
      }
      case "lince:protein-subscribe": {
        const wsSubId = `${data.instanceId}:${data.subId}`;
        subs.set(wsSubId, { win, subId: data.subId });
        send({ type: "subscribe", id: wsSubId, protein: data.protein });
        break;
      }
      case "lince:protein-subscribe-saved": {
        const wsSubId = `${data.instanceId}:${data.subId}`;
        subs.set(wsSubId, { win, subId: data.subId });
        send({ type: "subscribe_saved", id: wsSubId, name: data.name });
        break;
      }
      case "lince:protein-unsubscribe": {
        const wsSubId = `${data.instanceId}:${data.subId}`;
        subs.delete(wsSubId);
        send({ type: "unsubscribe", id: wsSubId });
        break;
      }
      case "lince:action": {
        // Map our transport-level reqId to the frame's own reqId so the
        // ActionOk/Error result finds its way back to the right promise.
        const reqId = nextReqId();
        acts.set(reqId, { win, frameReqId: data.reqId });
        send({ type: "act", id: reqId, action: data.action });
        break;
      }
      case "lince:lane-join":
        send({ type: "lane_join", room: data.room });
        break;
      case "lince:lane-send":
        send({ type: "lane_send", room: data.room, payload: data.payload });
        break;
    }
  }

  window.addEventListener("message", onFrameMessage);
  connect();

  return {
    registerFrame(instanceId, win) { frames.set(instanceId, win); },
  };
}
