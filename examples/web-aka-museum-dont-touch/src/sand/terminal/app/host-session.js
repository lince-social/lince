function terminalHost() {
  const host = window.LinceWidgetHost;
  if (!host || typeof host.openTerminalSession !== "function") {
    throw new Error("The board host does not provide terminal sessions.");
  }
  return host;
}

function terminalBytes(value) {
  if (value instanceof Uint8Array) {
    return value;
  }
  if (value instanceof ArrayBuffer) {
    return new Uint8Array(value);
  }
  if (ArrayBuffer.isView(value)) {
    return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  }
  return new Uint8Array(0);
}

function terminalSnapshot(session, geometry, overrides = {}) {
  return {
    id: String(overrides.id || session?.id || ""),
    shell: String(overrides.shell || session?.shell || ""),
    cwd: String(overrides.cwd || session?.cwd || ""),
    cols: Number(overrides.cols || session?.cols || geometry.cols) || geometry.cols,
    rows: Number(overrides.rows || session?.rows || geometry.rows) || geometry.rows,
    exitCode: overrides.exitCode ?? overrides.exit_code ?? null,
  };
}

function errorMessage(error) {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error || "Terminal host error.");
}

// The iframe-facing contract stays binary and session-oriented. frame.js owns
// postMessage correlation and byte encoding; the sand never opens a socket or
// sends terminal traffic through Protein, Actions, or peer lanes.
export function openHostTerminalSession(geometry, handlers = {}) {
  const initialGeometry = {
    cols: geometry.cols,
    rows: geometry.rows,
    pixelWidth: geometry.pixelWidth,
    pixelHeight: geometry.pixelHeight,
  };
  let active = true;
  let session = null;
  let reportedError = "";

  function reportError(error) {
    const message = errorMessage(error);
    if (message !== reportedError) {
      reportedError = message;
      handlers.onError?.(message);
    }
  }

  const ready = Promise.resolve(
    terminalHost().openTerminalSession({
      geometry: initialGeometry,
      onData(value) {
        if (!active) {
          return;
        }
        const bytes = terminalBytes(value);
        if (bytes.length > 0) {
          handlers.onBytes?.(bytes);
        }
      },
      onExit(exit = {}) {
        if (!active) {
          return;
        }
        active = false;
        const details = typeof exit === "number" ? { exitCode: exit } : {};
        handlers.onClosed?.(terminalSnapshot(session, initialGeometry, details));
      },
      onError(error) {
        if (active) {
          reportError(error);
        }
      },
    }),
  )
    .then((openedSession) => {
      session = openedSession;
      if (!session || typeof session.write !== "function") {
        throw new Error("The board returned an invalid terminal session.");
      }
      if (!active) {
        session.close?.();
        return terminalSnapshot(session, initialGeometry);
      }
      const snapshot = terminalSnapshot(session, initialGeometry);
      handlers.onReady?.(snapshot);
      return snapshot;
    })
    .catch((error) => {
      if (active) {
        reportError(error);
      }
      throw error;
    });

  return {
    ready,
    write(bytes) {
      const payload = terminalBytes(bytes);
      if (!active || payload.length === 0) {
        return;
      }
      void ready
        .then(() => {
          if (active) {
            session.write(payload);
          }
        })
        .catch(() => {});
    },
    resize(nextGeometry) {
      if (!active) {
        return;
      }
      const size = {
        cols: nextGeometry.cols,
        rows: nextGeometry.rows,
        pixelWidth: nextGeometry.pixelWidth,
        pixelHeight: nextGeometry.pixelHeight,
      };
      void ready
        .then(() => {
          if (active) {
            session.resize?.(size);
          }
        })
        .catch(() => {});
    },
    close() {
      if (!active) {
        return;
      }
      active = false;
      void ready.then(() => session?.close?.()).catch(() => {});
    },
  };
}
