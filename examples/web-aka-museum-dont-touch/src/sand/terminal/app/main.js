import { GhosttyRuntime } from "./ghostty-runtime.js";
import { openHostTerminalSession } from "./host-session.js";
import { createQueryReplyInterpreter } from "./query-replies.js";

const viewport = document.getElementById("viewport");
const buffer = document.getElementById("buffer");
const themeStyle = document.getElementById("ghostty-theme");
const statusPill = document.getElementById("status-pill");
const connectionButton = document.getElementById("connection-button");
const panelStatusDot = document.getElementById("panel-status-dot");
const infoPanel = document.getElementById("info-panel");
const closePanelButton = document.getElementById("close-panel-button");
const sessionMeta = document.getElementById("session-meta");
const restartButton = document.getElementById("restart-button");
const interruptButton = document.getElementById("interrupt-button");
const measureWidth = document.getElementById("measure-width");
const measureHeight = document.getElementById("measure-height");

const RESIZE_DELAY_MS = 90;

const state = {
  runtime: null,
  session: null,
  hostSession: null,
  geometry: null,
  resizeTimer: 0,
  renderFrame: 0,
  followOutput: true,
  disposed: false,
  replyInterpreter: null,
  renderNote: "",
  renderedCss: "",
  renderedHtml: "",
};

function setStatus(text, tone) {
  statusPill.textContent = text;
  connectionButton.dataset.tone = tone;
  panelStatusDot.dataset.tone = tone;
  connectionButton.setAttribute("aria-label", `Open terminal controls; ${text}`);
}

function setMeta(text) {
  sessionMeta.textContent = text || "Idle";
}

function scrollToBottom() {
  viewport.scrollTop = viewport.scrollHeight;
}

function isNearBottom() {
  const distance = viewport.scrollHeight - viewport.clientHeight - viewport.scrollTop;
  return distance < 32;
}

function formatSessionMeta(session, note = "") {
  const parts = [];
  if (session?.cols && session?.rows) {
    parts.push(`${session.cols}x${session.rows}`);
  }
  if (session?.cwd) {
    parts.push(session.cwd);
  }
  if (note) {
    parts.push(note);
  }
  return parts.join(" • ");
}

function measureGeometry() {
  const widthRect = measureWidth.getBoundingClientRect();
  const heightRect = measureHeight.getBoundingClientRect();
  const charWidth = widthRect.width > 0 ? widthRect.width / 10 : 8;
  const charHeight = heightRect.height > 0 ? heightRect.height : 18;
  const bufferStyles = window.getComputedStyle(buffer);
  const horizontalPadding =
    Number.parseFloat(bufferStyles.paddingLeft || "0") +
    Number.parseFloat(bufferStyles.paddingRight || "0");
  const verticalPadding =
    Number.parseFloat(bufferStyles.paddingTop || "0") +
    Number.parseFloat(bufferStyles.paddingBottom || "0");

  const cols = Math.max(
    2,
    Math.floor(Math.max(0, viewport.clientWidth - horizontalPadding) / Math.max(charWidth, 1)),
  );
  const rows = Math.max(
    1,
    Math.floor(Math.max(0, viewport.clientHeight - verticalPadding) / Math.max(charHeight, 1)),
  );

  return {
    cols,
    rows,
    cellWidth: Math.max(1, Math.round(charWidth)),
    cellHeight: Math.max(1, Math.round(charHeight)),
    pixelWidth: Math.max(1, Math.round(cols * charWidth)),
    pixelHeight: Math.max(1, Math.round(rows * charHeight)),
  };
}

function sameGeometry(left, right) {
  return (
    Boolean(left) &&
    Boolean(right) &&
    left.cols === right.cols &&
    left.rows === right.rows &&
    left.cellWidth === right.cellWidth &&
    left.cellHeight === right.cellHeight
  );
}

function renderTerminal(note = "") {
  if (!state.runtime) {
    return;
  }

  const formatted = state.runtime.formatHtml();

  if (formatted.css !== state.renderedCss) {
    themeStyle.textContent = formatted.css;
    state.renderedCss = formatted.css;
  }

  if (formatted.html !== state.renderedHtml) {
    buffer.innerHTML = formatted.html || "";
    state.renderedHtml = formatted.html || "";
  }

  if (state.followOutput) {
    scrollToBottom();
  }

  if (state.session) {
    setMeta(formatSessionMeta(state.session, note));
  }
}

function scheduleRender(note = "") {
  if (note) {
    state.renderNote = note;
  }

  if (state.renderFrame) {
    return;
  }

  state.renderFrame = window.requestAnimationFrame(() => {
    state.renderFrame = 0;
    const renderNote = state.renderNote;
    state.renderNote = "";
    renderTerminal(renderNote);
  });
}

function resetRenderCache() {
  state.renderedCss = "";
  state.renderedHtml = "";
}

function applySessionSnapshot(session) {
  state.session = session;
}

async function syncGeometry() {
  if (!state.runtime) {
    return;
  }

  const nextGeometry = measureGeometry();
  if (sameGeometry(nextGeometry, state.geometry)) {
    return;
  }

  state.geometry = nextGeometry;
  state.runtime.resize(nextGeometry);
  scheduleRender();

  if (state.hostSession) {
    state.hostSession.resize(nextGeometry);
    state.session = { ...state.session, cols: nextGeometry.cols, rows: nextGeometry.rows };
    setMeta(formatSessionMeta(state.session));
  }
}

function scheduleResize() {
  if (state.resizeTimer) {
    window.clearTimeout(state.resizeTimer);
  }
  state.resizeTimer = window.setTimeout(() => {
    state.resizeTimer = 0;
    void syncGeometry();
  }, RESIZE_DELAY_MS);
}

function enqueueInput(bytes) {
  if (!(bytes instanceof Uint8Array) || bytes.length === 0 || !state.hostSession) {
    return;
  }

  state.hostSession.write(bytes);
}

function enqueueInputText(text) {
  if (!text) {
    return;
  }

  enqueueInput(new TextEncoder().encode(text));
}

function resetTerminalRuntime(note = "") {
  if (!state.runtime || !state.geometry) {
    return;
  }

  state.runtime.resetTerminal(state.geometry);
  state.replyInterpreter = createReplyInterpreter();
  resetRenderCache();
  scheduleRender(note);
}

function processOutputBytes(bytes) {
  if (!state.runtime || !(bytes instanceof Uint8Array) || bytes.length === 0) {
    return;
  }

  let segmentStart = 0;
  for (let index = 0; index < bytes.length; index += 1) {
    const result = state.replyInterpreter?.feedByte(bytes[index]) || null;
    if (!result?.boundary) {
      continue;
    }

    const segment = bytes.subarray(segmentStart, index + 1);
    if (segment.length > 0) {
      state.runtime.writeBytes(segment);
    }
    const reply = result.sequence
      ? state.replyInterpreter?.replyFor(result.sequence)
      : null;
    if (reply) {
      enqueueInputText(reply);
    }
    segmentStart = index + 1;
  }

  const tail = bytes.subarray(segmentStart);
  if (tail.length > 0) {
    state.runtime.writeBytes(tail);
  }

  scheduleRender();
}

async function startSession() {
  if (!state.runtime) {
    return;
  }

  state.geometry = measureGeometry();
  resetTerminalRuntime();
  setStatus("Opening", "busy");
  setMeta("Starting shell");

  let hostSession = null;
  hostSession = openHostTerminalSession(state.geometry, {
    onReady(session) {
      if (state.hostSession !== hostSession) {
        return;
      }
      applySessionSnapshot(session);
      setStatus("Online", "ready");
      scheduleRender();
      viewport.focus();
    },
    onBytes(bytes) {
      if (state.hostSession !== hostSession) {
        return;
      }
      processOutputBytes(bytes);
    },
    onClosed(session) {
      if (state.hostSession !== hostSession) {
        return;
      }
      applySessionSnapshot(session);
      const exitCode = session?.exitCode;
      setStatus(exitCode == null ? "Closed" : `Exit ${exitCode}`, "error");
      scheduleRender("session ended");
      state.hostSession = null;
    },
    onError(message) {
      if (state.hostSession !== hostSession) {
        return;
      }
      setStatus("Host Error", "error");
      setMeta(message || "Falha no stream do terminal.");
    },
  });

  state.hostSession = hostSession;

  try {
    await hostSession.ready;
  } catch (error) {
    if (state.hostSession === hostSession) {
      state.hostSession = null;
      setStatus("Boot Error", "error");
      setMeta(error.message || "Falha ao iniciar o Ghostty terminal.");
    }
  }
}

function stopSession() {
  const hostSession = state.hostSession;
  state.hostSession = null;
  if (hostSession) {
    hostSession.close();
  }
}

function openInfoPanel() {
  connectionButton.hidden = true;
  connectionButton.setAttribute("aria-expanded", "true");
  infoPanel.hidden = false;
  closePanelButton.focus();
}

function closeInfoPanel() {
  infoPanel.hidden = true;
  connectionButton.hidden = false;
  connectionButton.setAttribute("aria-expanded", "false");
  viewport.focus();
}

async function restartSession() {
  stopSession();
  state.session = null;
  await startSession();
}

function shouldAllowBrowserShortcut(event) {
  const key = String(event.key || "").toLowerCase();
  const selectionText = window.getSelection?.()?.toString?.() || "";

  if ((event.ctrlKey || event.metaKey) && selectionText && key === "c") {
    return true;
  }

  if ((event.ctrlKey || event.metaKey) && event.shiftKey && key === "v") {
    return true;
  }

  return false;
}

function handleKeyboard(event) {
  if (!state.runtime || !state.hostSession || event.isComposing) {
    return;
  }

  if (document.activeElement !== viewport) {
    return;
  }

  if (shouldAllowBrowserShortcut(event)) {
    return;
  }

  if (event.type === "keydown") {
    event.preventDefault();
  }

  const action = event.type === "keyup" ? 0 : event.repeat ? 2 : 1;
  const bytes = state.runtime.encodeKeyboardEvent(event, action);
  enqueueInput(bytes);
}

function handlePaste(event) {
  if (!state.hostSession) {
    return;
  }

  const text = event.clipboardData?.getData("text/plain") || "";
  if (!text) {
    return;
  }

  event.preventDefault();
  enqueueInput(new TextEncoder().encode(text));
}

async function dispose() {
  state.disposed = true;
  window.clearTimeout(state.resizeTimer);
  if (state.renderFrame) {
    window.cancelAnimationFrame(state.renderFrame);
    state.renderFrame = 0;
  }
  stopSession();
  state.runtime?.destroy();
}

async function main() {
  try {
    setStatus("Loading", "busy");
    setMeta("Loading Ghostty wasm");

    state.runtime = await GhosttyRuntime.create("vendor/ghostty-vt.wasm");
    state.geometry = measureGeometry();
    state.replyInterpreter = createReplyInterpreter();

    const observer = new ResizeObserver(() => {
      scheduleResize();
    });
    observer.observe(viewport);
    window.addEventListener("beforeunload", () => {
      void dispose();
    });

    viewport.addEventListener("pointerdown", () => {
      viewport.focus();
    });
    viewport.addEventListener("keydown", handleKeyboard);
    viewport.addEventListener("keyup", handleKeyboard);
    viewport.addEventListener("paste", handlePaste);
    viewport.addEventListener("scroll", () => {
      state.followOutput = isNearBottom();
    });

    connectionButton.addEventListener("click", openInfoPanel);
    closePanelButton.addEventListener("click", closeInfoPanel);
    infoPanel.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        event.preventDefault();
        closeInfoPanel();
      }
    });
    restartButton.addEventListener("click", () => {
      void restartSession();
    });
    interruptButton.addEventListener("click", () => {
      enqueueInput(new Uint8Array([0x03]));
    });

    await startSession();
  } catch (error) {
    console.error(error);
    setStatus("Boot Error", "error");
    setMeta(error.message || "Falha ao iniciar o Ghostty terminal.");
  }
}

void main();

function createReplyInterpreter() {
  return createQueryReplyInterpreter(state.runtime);
}
