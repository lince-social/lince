function socketUrl(path) {
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${window.location.host}${path}`;
}

function geometryQuery(geometry) {
  const params = new URLSearchParams({
    cols: String(geometry.cols),
    rows: String(geometry.rows),
    pixelWidth: String(geometry.pixelWidth),
    pixelHeight: String(geometry.pixelHeight),
  });
  return params.toString();
}

export function openTerminalSocket(geometry, handlers) {
  const socket = new WebSocket(socketUrl(`/host/terminal/stream?${geometryQuery(geometry)}`));
  socket.binaryType = "arraybuffer";

  let readyResolve = () => {};
  let readyReject = () => {};
  const ready = new Promise((resolve, reject) => {
    readyResolve = resolve;
    readyReject = reject;
  });

  const pendingBinary = [];
  const pendingControl = [];
  let settled = false;

  function flushPending() {
    if (socket.readyState !== WebSocket.OPEN) {
      return;
    }

    while (pendingControl.length > 0) {
      socket.send(JSON.stringify(pendingControl.shift()));
    }

    while (pendingBinary.length > 0) {
      socket.send(pendingBinary.shift());
    }
  }

  function settleReadyError(message) {
    if (settled) {
      return;
    }
    settled = true;
    readyReject(new Error(message));
  }

  socket.addEventListener("open", () => {
    flushPending();
  });

  socket.addEventListener("message", async (event) => {
    if (typeof event.data === "string") {
      let frame = null;
      try {
        frame = JSON.parse(event.data);
      } catch (error) {
        handlers.onError?.(error.message || "Invalid terminal control frame.");
        settleReadyError(error.message || "Invalid terminal control frame.");
        return;
      }

      switch (frame.type) {
        case "ready":
          if (!settled) {
            settled = true;
            readyResolve(frame.session);
          }
          handlers.onReady?.(frame.session);
          flushPending();
          return;
        case "snapshot":
          handlers.onSnapshot?.(frame.session);
          return;
        case "reset":
          handlers.onReset?.(frame.session);
          return;
        case "closed":
          handlers.onClosed?.(frame.session);
          if (!settled) {
            settled = true;
            readyResolve(frame.session);
          }
          return;
        case "error":
          handlers.onError?.(frame.message || "Terminal socket error.");
          settleReadyError(frame.message || "Terminal socket error.");
          return;
        default:
          handlers.onError?.(`Unknown terminal frame: ${frame.type}`);
          return;
      }
    }

    const bytes =
      event.data instanceof ArrayBuffer
        ? new Uint8Array(event.data)
        : new Uint8Array(await event.data.arrayBuffer());
    handlers.onBytes?.(bytes);
  });

  socket.addEventListener("error", () => {
    const message = "Falha na conexao em tempo real com o terminal.";
    handlers.onError?.(message);
    settleReadyError(message);
  });

  socket.addEventListener("close", () => {
    if (!settled) {
      settled = true;
      readyReject(new Error("Terminal socket closed before ready."));
    }
  });

  return {
    ready,
    sendInput(bytes) {
      if (!(bytes instanceof Uint8Array) || bytes.length === 0) {
        return;
      }

      if (socket.readyState === WebSocket.OPEN) {
        socket.send(bytes);
        return;
      }

      pendingBinary.push(bytes.slice());
    },
    resize(geometryValue) {
      const command = {
        type: "resize",
        cols: geometryValue.cols,
        rows: geometryValue.rows,
        pixelWidth: geometryValue.pixelWidth,
        pixelHeight: geometryValue.pixelHeight,
      };

      if (socket.readyState === WebSocket.OPEN) {
        socket.send(JSON.stringify(command));
        return;
      }

      pendingControl.push(command);
    },
    close() {
      if (socket.readyState === WebSocket.CLOSED || socket.readyState === WebSocket.CLOSING) {
        return;
      }

      socket.close();
    },
  };
}
