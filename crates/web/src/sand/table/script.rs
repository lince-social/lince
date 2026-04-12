pub(super) fn script() -> String {
    r#"
      (() => {
        const frame = window.frameElement;
        const statusPill = document.getElementById("table-status");
        const tableDetails = document.getElementById("table-details");
        const bootstrap = document.getElementById("table-stream-bootstrap");
        const tablePanel = document.getElementById("table-body");
        const datastarReady = import("/static/vendored/datastar.js").catch(() => null);
        const serverId = String(frame?.dataset?.linceServerId || "").trim();
        const viewId = Number(String(frame?.dataset?.linceViewId || "").trim());
        const instanceId = String(frame?.dataset?.packageInstanceId || "preview").trim() || "preview";
        const settingsKey = "table-nerd/" + instanceId;

        const state = {
          controller: null,
          reconnectTimer: null,
          reconnectAttempt: 0,
          scrollTimer: null,
          streamGeneration: 0,
          streamUrl: "",
          nerdMode: false,
          focusedRowIndex: 0,
          focusedColumnIndex: 0,
        };

        function clamp(value, min, max) {
          return Math.min(max, Math.max(min, value));
        }

        function setStatus(text, tone = "idle") {
          if (!statusPill) {
            return;
          }

          statusPill.textContent = text;
          statusPill.dataset.tone = tone;
        }

        function readSettings() {
          try {
            const raw = window.localStorage?.getItem?.(settingsKey);
            if (!raw) {
              return null;
            }

            const parsed = JSON.parse(raw);
            if (!parsed || typeof parsed !== "object") {
              return null;
            }

            return {
              nerdMode: String(parsed.mode || "common") === "helix",
            };
          } catch {
            return null;
          }
        }

        function writeSettings() {
          try {
            window.localStorage?.setItem?.(
              settingsKey,
              JSON.stringify({
                mode: state.nerdMode ? "helix" : "common",
              }),
            );
          } catch {
            // ignore storage failures
          }
        }

        function currentModeSelect() {
          return document.getElementById("table-mode");
        }

        function syncModeSelect() {
          const select = currentModeSelect();
          if (!select) {
            return;
          }

          select.value = state.nerdMode ? "helix" : "common";
        }

        function buildStreamUrl() {
          if (!serverId) {
            return "";
          }

          if (!Number.isInteger(viewId) || viewId <= 0) {
            return "";
          }

          return (
            "/host/integrations/servers/" +
            encodeURIComponent(serverId) +
            "/views/" +
            encodeURIComponent(viewId) +
            "/table/stream"
          );
        }

        function parseEventBlock(block) {
          const lines = block.split("\n");
          let eventName = "message";
          const dataLines = [];

          for (const line of lines) {
            if (line.startsWith("event:")) {
              eventName = line.slice(6).trim();
            } else if (line.startsWith("data:")) {
              dataLines.push(line.slice(5).trimStart());
            }
          }

          return { event: eventName, data: dataLines.join("\n") };
        }

        function parseSsePayload(payload) {
          if (typeof payload !== "string") {
            return payload;
          }

          try {
            return JSON.parse(payload);
          } catch {
            return payload;
          }
        }

        function dispatchDatastarEvent(eventName, detail) {
          document.dispatchEvent(
            new CustomEvent("datastar-fetch", {
              detail: {
                type: eventName,
                argsRaw: detail,
              },
            }),
          );
        }

        function stopReconnectTimer() {
          if (state.reconnectTimer) {
            window.clearTimeout(state.reconnectTimer);
            state.reconnectTimer = null;
          }
        }

        function stopScrollTimer() {
          if (state.scrollTimer) {
            window.clearTimeout(state.scrollTimer);
            state.scrollTimer = null;
          }
        }

        function setScrolling(active) {
          if (!tablePanel) {
            return;
          }

          if (active) {
            tablePanel.dataset.scrolling = "true";
            stopScrollTimer();
            state.scrollTimer = window.setTimeout(() => {
              delete tablePanel.dataset.scrolling;
              state.scrollTimer = null;
            }, 160);
            return;
          }

          delete tablePanel.dataset.scrolling;
          stopScrollTimer();
        }

        function clearSelectionAttributes() {
          if (!tablePanel) {
            return;
          }

          tablePanel
            .querySelectorAll("[data-row-focused], [data-focused-cell]")
            .forEach((node) => {
              delete node.dataset.rowFocused;
              delete node.dataset.focusedCell;
            });
        }

        function rows() {
          return tablePanel
            ? Array.from(tablePanel.querySelectorAll("tbody tr"))
            : [];
        }

        function columns() {
          return tablePanel
            ? Array.from(tablePanel.querySelectorAll("thead th"))
            : [];
        }

        function focusedRow() {
          return rows()[state.focusedRowIndex] || null;
        }

        function focusedCell() {
          const row = focusedRow();
          if (!row) {
            return null;
          }

          return row.querySelectorAll("td")[state.focusedColumnIndex] || null;
        }

        function syncSelection() {
          if (!tablePanel) {
            return;
          }

          const currentRows = rows();
          const currentColumns = columns();
          if (!currentRows.length) {
            state.focusedRowIndex = 0;
            state.focusedColumnIndex = 0;
            delete tablePanel.dataset.mode;
            clearSelectionAttributes();
            return;
          }

          state.focusedRowIndex = clamp(state.focusedRowIndex, 0, currentRows.length - 1);
          state.focusedColumnIndex = clamp(
            state.focusedColumnIndex,
            0,
            Math.max(0, currentColumns.length - 1),
          );

          clearSelectionAttributes();
          syncModeSelect();

          if (state.nerdMode) {
            tablePanel.dataset.mode = "helix";
            currentRows.forEach((row, rowIndex) => {
              row.dataset.rowIndex = String(rowIndex);
              if (rowIndex === state.focusedRowIndex) {
                row.dataset.rowFocused = "true";
                row.scrollIntoView({ block: "nearest", inline: "nearest" });
              }
            });

          } else {
            delete tablePanel.dataset.mode;
            currentRows.forEach((row, rowIndex) => {
              row.dataset.rowIndex = String(rowIndex);
            });
          }
        }

        function setNerdMode(enabled) {
          state.nerdMode = enabled === true;
          writeSettings();
          syncSelection();
        }

        function moveFocus(rowDelta, columnDelta) {
          const currentRows = rows();
          const currentColumns = columns();
          if (!currentRows.length) {
            return;
          }

          if (rowDelta !== 0) {
            state.focusedRowIndex = clamp(
              state.focusedRowIndex + rowDelta,
              0,
              currentRows.length - 1,
            );
          }

          if (columnDelta !== 0 && currentColumns.length) {
            state.focusedColumnIndex = clamp(
              state.focusedColumnIndex + columnDelta,
              0,
              currentColumns.length - 1,
            );
          }

          syncSelection();
        }

        function handleTableKeydown(event) {
          if (!state.nerdMode || !tablePanel) {
            return;
          }

          if (event.metaKey || event.ctrlKey || event.altKey) {
            return;
          }

          const key = event.key;
          if (key === "j" || key === "ArrowDown") {
            event.preventDefault();
            moveFocus(1, 0);
            return;
          }

          if (key === "k" || key === "ArrowUp") {
            event.preventDefault();
            moveFocus(-1, 0);
            return;
          }

          if (key === "h" || key === "ArrowLeft") {
            event.preventDefault();
            moveFocus(0, -1);
            return;
          }

          if (key === "l" || key === "ArrowRight") {
            event.preventDefault();
            moveFocus(0, 1);
            return;
          }
        }

        function clearStream() {
          stopReconnectTimer();
          setScrolling(false);

          if (state.controller) {
            state.controller.abort();
            state.controller = null;
          }
        }

        function scheduleReconnect() {
          stopReconnectTimer();

          if (!state.streamUrl) {
            return;
          }

          const delay = Math.min(12000, 1200 * Math.max(1, state.reconnectAttempt + 1));
          state.reconnectAttempt += 1;
          state.reconnectTimer = window.setTimeout(() => connectStream(false), delay);
        }

        async function connectStream(reset) {
          clearStream();

          if (!state.streamUrl) {
            setStatus("Configurar", "idle");
            return;
          }

          if (reset) {
            setStatus("Connecting", "loading");
          }

          const generation = ++state.streamGeneration;
          const controller = new AbortController();
          state.controller = controller;

          try {
            if (bootstrap) {
              bootstrap.dataset.streamUrl = state.streamUrl;
            }

            const response = await fetch(state.streamUrl, {
              headers: {
                Accept: "text/event-stream",
              },
              cache: "no-store",
              signal: controller.signal,
            });

            if (controller.signal.aborted || generation !== state.streamGeneration) {
              return;
            }

            if (response.status === 401) {
              window.LinceWidgetHost?.invalidateServerAuth?.(serverId);
              setStatus("Bloqueado", "error");
              return;
            }

            if (!response.ok || !response.body) {
              const raw = await response.text().catch(() => "");
              throw new Error(raw || `Nao foi possivel abrir o stream (${response.status}).`);
            }

            state.reconnectAttempt = 0;
            setStatus("Live", "live");

            const reader = response.body.getReader();
            const decoder = new TextDecoder();
            let buffer = "";

            while (true) {
              const { value, done } = await reader.read();
              if (done || controller.signal.aborted || generation !== state.streamGeneration) {
                break;
              }

              buffer += decoder.decode(value, { stream: true });
              const blocks = buffer.split("\n\n");
              buffer = blocks.pop() || "";

              for (const block of blocks) {
                const trimmed = block.trim();
                if (!trimmed) {
                  continue;
                }

                const event = parseEventBlock(trimmed);
                if (!event.data || !event.event.startsWith("datastar-")) {
                  continue;
                }

                const payload = parseSsePayload(event.data);
                if (payload && typeof payload === "object") {
                  dispatchDatastarEvent(event.event, payload);
                }
              }
            }

            if (controller.signal.aborted || generation !== state.streamGeneration) {
              return;
            }

            setStatus("Reconnecting", "loading");
            scheduleReconnect();
          } catch (error) {
            if (controller.signal.aborted || generation !== state.streamGeneration) {
              return;
            }

            setStatus("Offline", "error");
            scheduleReconnect();
            if (error instanceof Error) {
              console.error(error);
            }
          } finally {
            if (state.controller === controller) {
              state.controller = null;
            }
          }
        }

        window.TableWidget = {
          reconnect() {
            state.streamUrl = buildStreamUrl();
            if (!state.streamUrl) {
              setStatus("Configurar", "idle");
              return;
            }
            state.reconnectAttempt = 0;
            connectStream(true);
          },
        };

        if (tablePanel) {
          tablePanel.tabIndex = 0;
          tablePanel.addEventListener("keydown", handleTableKeydown);
          tablePanel.addEventListener("pointerdown", () => {
            if (!state.nerdMode) {
              return;
            }
            window.requestAnimationFrame(() => {
              tablePanel.focus({ preventScroll: true });
            });
          });
          tablePanel.addEventListener("focus", () => {
            if (state.nerdMode) {
              syncSelection();
            }
          });

          const observer = new MutationObserver(() => {
            syncSelection();
          });
          observer.observe(tablePanel, { childList: true, subtree: true });
        }

        const storedSettings = readSettings();
        if (storedSettings) {
          state.nerdMode = storedSettings.nerdMode;
        }
        syncModeSelect();

        document.addEventListener("change", (event) => {
          const target = event.target;
          if (!(target instanceof HTMLSelectElement) || target.id !== "table-mode") {
            return;
          }

          setNerdMode(target.value === "helix");
          if (state.nerdMode && tablePanel) {
            tablePanel.focus({ preventScroll: true });
          }
        });

        datastarReady.then(() => {
          state.streamUrl = buildStreamUrl();
          if (!state.streamUrl) {
            setStatus("Configurar", "idle");
            return;
          }

          syncSelection();
          connectStream(false);
        });

        if (tableDetails) {
          const observer = new MutationObserver(() => {
            syncModeSelect();
          });
          observer.observe(tableDetails, { childList: true, subtree: true });
        }
      })();
    "#.to_string()
}
