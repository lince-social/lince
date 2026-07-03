pub(super) fn script() -> String {
    String::from(
        r##"
      (() => {
        const root = document.querySelector("[data-record-info-root]");
        const bridge = window.LinceWidgetHost || null;
        if (!root) {
          return;
        }

        const state = {
          meta: bridge?.getMeta?.() || {},
          active: null, // { serverId, viewId, recordId, record }
          row: null,
          status: "idle", // idle | loading | live | error
          statusLabel: "",
          stream: null,
          streamGeneration: 0,
        };

        function escapeHtml(value) {
          return String(value ?? "")
            .replace(/&/g, "&amp;")
            .replace(/</g, "&lt;")
            .replace(/>/g, "&gt;")
            .replace(/"/g, "&quot;")
            .replace(/'/g, "&#39;");
        }

        function teardownStream() {
          state.streamGeneration += 1;
          if (state.stream) {
            state.stream.close();
            state.stream = null;
          }
        }

        function resolveOrigin(eventPayload) {
          const serverId =
            String(eventPayload?.serverId || "").trim() ||
            String(state.meta?.serverId || "").trim();
          const rawViewId = eventPayload?.viewId ?? state.meta?.viewId;
          const viewId = Number(rawViewId || 0) || null;
          return { serverId, viewId };
        }

        function streamBase(origin) {
          if (!origin.serverId || !Number.isInteger(origin.viewId) || origin.viewId <= 0) {
            return "";
          }

          return (
            "/host/integrations/servers/" +
            encodeURIComponent(origin.serverId) +
            "/views/" +
            encodeURIComponent(origin.viewId)
          );
        }

        function findRecordRow(payload, recordId) {
          const rows = Array.isArray(payload?.rows) ? payload.rows : [];
          const target = String(recordId ?? "");
          return (
            rows.find((row) => String(row?.id ?? "") === target) || null
          );
        }

        function fieldsForRender() {
          if (state.row && typeof state.row === "object") {
            return state.row;
          }
          if (state.active?.record && typeof state.active.record === "object") {
            return state.active.record;
          }
          return null;
        }

        function render() {
          if (!state.active) {
            root.innerHTML =
              '<div class="recordInfoBall" title="Record Info: aguardando um recordClicked">◉</div>';
            return;
          }

          const fields = fieldsForRender();
          const title = fields?.head
            ? String(fields.head)
            : "Record #" + String(state.active.recordId ?? "?");
          const entries = fields
            ? Object.entries(fields).map(
                ([key, value]) =>
                  "<dt>" +
                  escapeHtml(key) +
                  "</dt><dd>" +
                  escapeHtml(value === null || value === undefined ? "—" : value) +
                  "</dd>",
              )
            : [];

          root.innerHTML = [
            '<section class="recordInfoPanel">',
            '<header class="recordInfoPanel__header">',
            '<h2 class="recordInfoPanel__title">' + escapeHtml(title) + "</h2>",
            '<button type="button" class="recordInfoPanel__close" data-record-info-close aria-label="Fechar">×</button>',
            "</header>",
            '<span class="recordInfoStatus" data-state="' +
              escapeHtml(state.status) +
              '">' +
              escapeHtml(state.statusLabel || state.status) +
              "</span>",
            entries.length
              ? '<dl class="recordInfoFields">' + entries.join("") + "</dl>"
              : '<p class="recordInfoEmpty">Sem dados para esse record ainda.</p>',
            "</section>",
          ].join("");
        }

        function setStatus(status, label) {
          state.status = status;
          state.statusLabel = label || "";
          render();
        }

        async function loadSnapshot(origin, recordId, generation) {
          const base = streamBase(origin);
          if (!base) {
            setStatus("idle", "Sem view configurada; mostrando dados do evento.");
            return;
          }

          try {
            const response = await fetch(base + "/snapshot");
            const payload = await response.json().catch(() => null);
            if (generation !== state.streamGeneration) {
              return;
            }
            if (!response.ok) {
              throw new Error(payload?.error || "Falha ao consultar a view.");
            }

            state.row = findRecordRow(payload, recordId);
            setStatus(
              "live",
              state.row ? "" : "Record fora da view configurada.",
            );
          } catch (error) {
            if (generation !== state.streamGeneration) {
              return;
            }
            setStatus(
              "error",
              error instanceof Error ? error.message : "Falha ao consultar a view.",
            );
          }
        }

        function openStream(origin, recordId) {
          // One dedicated request pair per event: tear the previous stream
          // down before snapshot + EventSource for the new record.
          teardownStream();
          const generation = state.streamGeneration;
          void loadSnapshot(origin, recordId, generation);

          const base = streamBase(origin);
          if (!base) {
            return;
          }

          const source = new EventSource(base + "/stream");
          state.stream = source;

          source.addEventListener("snapshot", (event) => {
            if (generation !== state.streamGeneration || state.stream !== source) {
              return;
            }

            let payload = null;
            try {
              payload = JSON.parse(String(event.data || "null"));
            } catch {
              payload = null;
            }
            state.row = findRecordRow(payload, recordId);
            setStatus("live", state.row ? "" : "Record fora da view configurada.");
          });

          source.addEventListener("error", () => {
            if (generation !== state.streamGeneration || state.stream !== source) {
              return;
            }
            setStatus("error", "Stream offline.");
          });
        }

        function close() {
          teardownStream();
          state.active = null;
          state.row = null;
          setStatus("idle", "");
        }

        root.addEventListener("click", (event) => {
          if (event.target.closest("[data-record-info-close]")) {
            close();
          }
        });

        bridge?.subscribe?.((detail) => {
          state.meta = detail?.meta || {};
        });

        bridge?.onEvent?.("recordClicked", (payload) => {
          const data = payload?.data || {};
          const origin = resolveOrigin(data);
          state.active = {
            serverId: origin.serverId,
            viewId: origin.viewId,
            recordId: data.recordId ?? null,
            record: data.record && typeof data.record === "object" ? data.record : null,
          };
          state.row = null;
          setStatus("loading", "Consultando a view...");
          openStream(origin, state.active.recordId);
        });

        render();
      })();
    "##,
    )
}
