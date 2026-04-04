use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};
use maud::{Markup, html};

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: r#"record-crud.html"#,
        lang: r#"pt-BR"#,
        manifest: PackageManifest {
            icon: r#"▤"#.into(),
            title: r#"Record CRUD"#.into(),
            author: r#"Lince Labs"#.into(),
            version: r#"0.4.0"#.into(),
            description: r#"Widget compacto para criar, atualizar e excluir registros da tabela record."#.into(),
            details: r#"Esse package le server_id do iframe e usa o proxy local do Lince em /host/integrations/servers/{server_id}/table/record para criar, atualizar e excluir registros com debug completo da requisicao."#.into(),
            initial_width: 4,
            initial_height: 5,
            requires_server: true,
            permissions: vec![r#"write_records"#.into()],
        },
        head_links: vec![],
        inline_styles: vec![r#"      :root {
        color-scheme: dark;
        --panel: #171d24;
        --panel-soft: #1c242d;
        --line: rgba(255, 255, 255, 0.08);
        --line-strong: rgba(255, 255, 255, 0.14);
        --text: #eef2f7;
        --muted: #92a0af;
        --accent: #e9edf5;
        --danger: #ff9aa8;
        --ok: #84f0bb;
        --mono: "IBM Plex Mono", "SFMono-Regular", monospace;
      }
      * { box-sizing: border-box; }
      html, body { min-height: 100%; margin: 0; background: transparent; }
      body {
        min-height: 100vh;
        padding: 14px;
        color: var(--text);
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
      }
      .app {
        display: grid;
        grid-template-rows: auto auto auto 1fr auto;
        gap: 12px;
        min-height: calc(100vh - 28px);
      }
      .header, .grid, .actions, .stack { display: grid; gap: 8px; }
      .eyebrow {
        color: var(--muted);
        font-size: 0.68rem;
        font-weight: 600;
        letter-spacing: 0.16em;
        text-transform: uppercase;
      }
      .title { margin: 0; font-size: 1rem; font-weight: 600; letter-spacing: -0.02em; }
      .copy { margin: 0; color: var(--muted); font-size: 0.78rem; line-height: 1.45; }
      .grid--double { grid-template-columns: repeat(2, minmax(0, 1fr)); }
      .field, .button, .textarea, .select {
        width: 100%;
        border: 1px solid var(--line);
        border-radius: 12px;
        background: var(--panel);
        color: var(--text);
        font: inherit;
      }
      .field, .textarea, .select { padding: 10px 11px; }
      .textarea { min-height: 78px; resize: vertical; }
      .button {
        min-height: 38px;
        padding: 0 12px;
        cursor: pointer;
        transition: border-color 160ms ease, background 160ms ease, color 160ms ease;
      }
      .button:hover { border-color: var(--line-strong); background: var(--panel-soft); }
      .button--primary {
        border-color: rgba(255, 255, 255, 0.16);
        background: rgba(233, 237, 245, 0.08);
        color: var(--accent);
        font-weight: 600;
      }
      .button--danger { color: var(--danger); }
      .meta {
        display: flex;
        justify-content: space-between;
        gap: 10px;
        align-items: center;
        color: var(--muted);
        font-size: 0.72rem;
      }
      .stack { min-height: 0; grid-template-rows: auto 1fr; }
      .label {
        color: var(--muted);
        font-family: var(--mono);
        font-size: 0.66rem;
        font-weight: 600;
        letter-spacing: 0.14em;
        text-transform: uppercase;
      }
      pre {
        margin: 0;
        min-height: 0;
        overflow: auto;
        padding: 12px;
        border: 1px solid var(--line);
        border-radius: 14px;
        background: rgba(10, 12, 15, 0.66);
        color: var(--text);
        font-family: var(--mono);
        font-size: 0.72rem;
        line-height: 1.5;
        white-space: pre-wrap;
        word-break: break-word;
      }
      .status--ok { color: var(--ok); }
      .status--error { color: var(--danger); }
    "#],
        body: body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(r#"      const frame = window.frameElement;
      const serverId = String(frame?.dataset?.linceServerId || "").trim();
      const elements = {
        mode: document.getElementById("mode"),
        recordId: document.getElementById("record-id"),
        quantity: document.getElementById("quantity"),
        head: document.getElementById("head"),
        body: document.getElementById("body"),
        run: document.getElementById("run"),
        seedNeed: document.getElementById("seed-need"),
        seedGive: document.getElementById("seed-give"),
        sqlPreview: document.getElementById("sql-preview"),
        resultPreview: document.getElementById("result-preview"),
        status: document.getElementById("status"),
      };

      function setStatus(text, tone) {
        elements.status.textContent = text;
        elements.status.className = tone ? "status--" + tone : "";
      }

      function setExample(kind) {
        if (kind === "need") {
          elements.mode.value = "create";
          elements.quantity.value = "-1";
          elements.head.value = "Need ride to clinic";
          elements.body.value = "Need transport on Monday morning";
        } else {
          elements.mode.value = "create";
          elements.quantity.value = "3";
          elements.head.value = "Bread loaves";
          elements.body.value = "Fresh bread available today";
        }
        syncFormState();
      }

      function buildBaseUrl() {
        if (!serverId) {
          return null;
        }
        return "/host/integrations/servers/" + encodeURIComponent(serverId) + "/table/record";
      }

      function buildPayload() {
        return {
          quantity: Number(elements.quantity.value.trim() || "1"),
          head: elements.head.value.trim() || null,
          body: elements.body.value.trim() || null,
        };
      }

      function buildRequest() {
        const baseUrl = buildBaseUrl();
        if (!baseUrl) {
          return { error: "Widget sem server_id. Configure o servidor no host." };
        }
        const mode = elements.mode.value;
        const id = Number(elements.recordId.value);

        if (mode === "create") {
          return { method: "POST", url: baseUrl, payload: buildPayload() };
        }
        if (!Number.isInteger(id) || id <= 0) {
          return { error: "-- informe um id valido" };
        }
        if (mode === "update") {
          return { method: "PATCH", url: baseUrl + "/" + id, payload: buildPayload() };
        }
        return { method: "DELETE", url: baseUrl + "/" + id, payload: null };
      }

      function syncFormState() {
        const mode = elements.mode.value;
        const destructive = mode === "delete";
        const request = buildRequest();
        elements.quantity.disabled = destructive;
        elements.head.disabled = destructive;
        elements.body.disabled = destructive;
        elements.run.textContent = mode === "create" ? "Criar registro" : mode === "update" ? "Atualizar registro" : "Excluir registro";
        elements.sqlPreview.textContent = request.error ? request.error : JSON.stringify(request, null, 2);
      }

      async function runRequest() {
        const request = buildRequest();
        if (request.error) {
          setStatus(request.error.includes("server_id") ? "Configurar" : "ID invalido para essa operacao.", "error");
          elements.resultPreview.textContent = request.error;
          return;
        }

        setStatus("Executando via backend local...", "");
        elements.resultPreview.textContent = JSON.stringify({ phase: "request", ...request }, null, 2);

        const response = await fetch(request.url, {
          method: request.method,
          headers: request.payload ? { "Content-Type": "application/json" } : undefined,
          body: request.payload ? JSON.stringify(request.payload) : undefined,
        });
        const raw = await response.text();
        let parsed = null;

        try {
          parsed = raw ? JSON.parse(raw) : null;
        } catch {
          parsed = null;
        }

        elements.resultPreview.textContent = JSON.stringify(
          {
            phase: "response",
            url: request.url,
            status: response.status,
            ok: response.ok,
            request,
            raw,
            parsed,
          },
          null,
          2,
        );

        if (response.status === 401) {
          window.LinceWidgetHost?.invalidateServerAuth?.(serverId);
          setStatus("Bloqueado", "error");
          return;
        }
        if (!response.ok) {
          setStatus("Falha na execucao.", "error");
          return;
        }
        setStatus(parsed ? "CRUD executado." : "Resposta sem JSON.", parsed ? "ok" : "error");
      }

      elements.mode.addEventListener("change", syncFormState);
      elements.recordId.addEventListener("input", syncFormState);
      elements.quantity.addEventListener("input", syncFormState);
      elements.head.addEventListener("input", syncFormState);
      elements.body.addEventListener("input", syncFormState);
      elements.run.addEventListener("click", () => {
        runRequest().catch((error) => {
          setStatus("Erro de rede.", "error");
          elements.resultPreview.textContent = JSON.stringify(
            {
              phase: "exception",
              message: String(error && error.message ? error.message : error),
              request: buildRequest(),
            },
            null,
            2,
          );
        });
      });
      elements.seedNeed.addEventListener("click", () => setExample("need"));
      elements.seedGive.addEventListener("click", () => setExample("give"));
      syncFormState();
    "#)],
    }
}

fn body() -> Markup {
    let modes = [
        ("create", "Create"),
        ("update", "Update"),
        ("delete", "Delete"),
    ];
    let request_panels = [
        ("Request", "sql-preview", "-- aguardando"),
        ("Debug", "result-preview", "-- sem execucao"),
    ];

    html! {
        main class="app" {
            header class="header" {
                span class="eyebrow" { "Backend CRUD" }
                h1 class="title" { "record CRUD" }
                p class="copy" {
                    "Executa CRUD na tabela record via backend local conectado ao servidor externo."
                }
            }
            section class="grid grid--double" {
                select class="select" id="mode" {
                    @for (value, label) in modes {
                        option value=(value) { (label) }
                    }
                }
                input
                    class="field"
                    id="record-id"
                    type="number"
                    min="1"
                    step="1"
                    placeholder="id (update/delete)";
            }
            section class="grid" {
                div class="grid grid--double" {
                    input
                        class="field"
                        id="quantity"
                        type="number"
                        step="any"
                        value="1"
                        placeholder="quantity";
                    input
                        class="field"
                        id="head"
                        type="text"
                        placeholder="head"
                        value="Buy apples";
                }
                textarea class="textarea" id="body" placeholder="body" { "Need to buy apples this week" }
            }
            section class="actions" {
                div class="meta" {
                    span id="status" { "Pronto." }
                    span { "quantity < 0 = necessity" }
                }
                div class="grid grid--double" {
                    button class="button button--primary" id="run" { "Executar" }
                    button class="button button--danger" id="seed-need" { "Need" }
                }
                button class="button" id="seed-give" { "Contribution" }
            }
            @for (label, id, initial_text) in request_panels {
                section class="stack" {
                    div class="label" { (label) }
                    pre id=(id) { (initial_text) }
                }
            }
        }
    }
}
