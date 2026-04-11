pub(super) fn script() -> String {
    r#"
      (() => {
        const frame = window.frameElement;
        const statusPill = document.getElementById("table-status");
        const bootstrap = document.getElementById("table-stream-bootstrap");
        const datastarReady = import("/static/vendored/datastar.js").catch(() => null);
        const serverId = String(frame?.dataset?.linceServerId || "").trim();
        const viewId = Number(String(frame?.dataset?.linceViewId || "").trim());

        const state = {
          controller: null,
          reconnectTimer: null,
          reconnectAttempt: 0,
          streamGeneration: 0,
          streamUrl: "",
        };

        function setStatus(text, tone = "idle") {
          if (!statusPill) {
            return;
          }

          statusPill.textContent = text;
          statusPill.dataset.tone = tone;
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

        function clearStream() {
          stopReconnectTimer();

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

        datastarReady.then(() => {
          state.streamUrl = buildStreamUrl();
          if (!state.streamUrl) {
            setStatus("Configurar", "idle");
            return;
          }

          connectStream(false);
        });
      })();
    "#.to_string()
}
