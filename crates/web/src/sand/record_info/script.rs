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
          proteinUnsubscribe: null,
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
          if (state.proteinUnsubscribe) {
            state.proteinUnsubscribe();
            state.proteinUnsubscribe = null;
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

        function recordIdentity(recordId, record) {
          const uid = String(record?.uid || "").trim();
          if (uid) {
            return { kind: "uid", value: uid };
          }
          const slug = String(record?.slug || "").trim();
          if (slug) {
            return { kind: "slug", value: slug };
          }
          const raw = String(recordId ?? "").trim();
          if (raw.startsWith("rec_") || raw.startsWith("record_")) {
            return { kind: "uid", value: raw };
          }
          return raw ? { kind: "legacy-id", value: raw } : null;
        }

        function findProteinRow(rows, identity) {
          if (!identity) {
            return null;
          }
          if (identity.kind === "uid") {
            return rows.find((row) => String(row?.uid || "") === identity.value) || null;
          }
          if (identity.kind === "slug") {
            return rows.find((row) => String(row?.slug || "") === identity.value) || null;
          }
          return null;
        }

        function proteinForIdentity(identity) {
          const protein = {
            source: "record",
            include: { facts: { limit: 12 } },
            order: [{ desc: "created_at" }],
            limit: 200,
          };
          if (identity?.kind === "uid") {
            protein.where = [{ uid_eq: identity.value }];
            protein.limit = 1;
          } else if (identity?.kind === "slug") {
            protein.where = [{ slug_eq: identity.value }];
            protein.limit = 1;
          }
          return protein;
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

        function formatValue(value) {
          if (value === null || value === undefined) {
            return "—";
          }
          if (Array.isArray(value) || typeof value === "object") {
            try {
              return JSON.stringify(value, null, 2);
            } catch {
              return String(value);
            }
          }
          return String(value);
        }

        function renderFacts(fields) {
          const facts = Array.isArray(fields?.facts) ? fields.facts : [];
          if (!facts.length) {
            return "";
          }
          return [
            '<section class="recordInfoFacts">',
            '<h3 class="recordInfoFacts__title">Proveniência</h3>',
            '<ul class="recordInfoFacts__list">',
            facts
              .map((fact) => {
                const delta = Number(fact?.delta || 0);
                const sign = delta > 0 ? "+" : "";
                const cause = [fact?.cause_kind, fact?.cause].filter(Boolean).join(" · ");
                return (
                  '<li class="recordInfoFact">' +
                  '<span class="recordInfoFact__delta">' +
                  escapeHtml(sign + String(delta)) +
                  "</span>" +
                  '<span class="recordInfoFact__cause">' +
                  escapeHtml(cause || "fact") +
                  "</span>" +
                  '<span class="recordInfoFact__when">' +
                  escapeHtml(fact?.at || "") +
                  "</span>" +
                  "</li>"
                );
              })
              .join(""),
            "</ul>",
            "</section>",
          ].join("");
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
            ? Object.entries(fields)
                .filter(([key]) => key !== "facts")
                .map(
                  ([key, value]) =>
                    "<dt>" +
                    escapeHtml(key) +
                    "</dt><dd>" +
                    escapeHtml(formatValue(value)) +
                    "</dd>",
                )
            : [];
          const factsHtml = renderFacts(fields);

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
            factsHtml,
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

        function openProtein(recordId, record) {
          if (typeof bridge?.subscribeProtein !== "function") {
            return false;
          }
          const identity = recordIdentity(recordId, record);
          if (!identity || identity.kind === "legacy-id") {
            return false;
          }

          teardownStream();
          const generation = state.streamGeneration;
          const protein = proteinForIdentity(identity);
          state.proteinUnsubscribe = bridge.subscribeProtein(
            "record-info",
            protein,
            ({ rows, live }) => {
              if (generation !== state.streamGeneration) {
                return;
              }
              const list = Array.isArray(rows) ? rows : [];
              state.row = findProteinRow(list, identity);
              setStatus(
                live === false ? "loading" : "live",
                state.row ? "" : "Record fora do Protein configurado.",
              );
            },
          );
          return true;
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
          setStatus("loading", "Consultando Protein...");
          if (!openProtein(state.active.recordId, state.active.record)) {
            setStatus("loading", "Consultando a view...");
            openStream(origin, state.active.recordId);
          }
        });

        render();
      })();
    "##,
    )
}
