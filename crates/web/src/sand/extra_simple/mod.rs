use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};
use maud::{Markup, html};

pub(crate) const FEATURE_FLAG: &str = "sand.extra_simple";

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: r#"extra-simple.html"#,
        lang: r#"pt-BR"#,
        manifest: PackageManifest {
            icon: r#"◍"#.into(),
            title: r#"Extra Simple View"#.into(),
            author: r#"Lince Labs"#.into(),
            version: r#"0.4.0"#.into(),
            description: r#"Widget minimo que acompanha o SSE de uma view via backend local."#.into(),
            details: r#"Esse package le server_id e view_id do iframe, monta a rota /host/integrations/servers/{server_id}/views/{view_id}/stream e mostra estados de configuracao, bloqueio e erro sem implementar auth."#.into(),
            initial_width: 4,
            initial_height: 3,
            requires_server: true,
            permissions: vec![r#"read_view_stream"#.into()],
        },
        head_links: vec![],
        inline_styles: vec![r#"      :root {
        color-scheme: dark;
        --bg: #11151b;
        --panel: #171d24;
        --line: rgba(255, 255, 255, 0.08);
        --text: #eef2f7;
        --muted: #93a0ae;
        --ok: #7bf1b2;
        --warn: #f6d47a;
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
        grid-template-rows: auto auto 1fr;
        gap: 12px;
        min-height: calc(100vh - 28px);
      }
      .header { display: grid; gap: 5px; }
      .eyebrow {
        color: var(--muted);
        font-size: 0.68rem;
        font-weight: 600;
        letter-spacing: 0.16em;
        text-transform: uppercase;
      }
      .title { margin: 0; font-size: 0.98rem; font-weight: 600; letter-spacing: -0.02em; }
      .copy { margin: 0; color: var(--muted); font-size: 0.8rem; line-height: 1.45; }
      .status {
        display: inline-flex;
        align-items: center;
        gap: 8px;
        min-height: 38px;
        padding: 0 12px;
        border: 1px solid var(--line);
        border-radius: 12px;
        background: var(--panel);
        color: var(--muted);
        font-size: 0.76rem;
        letter-spacing: 0.04em;
      }
      .status::before {
        content: "";
        width: 8px;
        height: 8px;
        border-radius: 999px;
        background: var(--warn);
        box-shadow: 0 0 0 6px rgba(246, 212, 122, 0.08);
      }
      .status[data-state="live"]::before {
        background: var(--ok);
        box-shadow: 0 0 0 6px rgba(123, 241, 178, 0.08);
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
        line-height: 1.55;
        white-space: pre-wrap;
        word-break: break-word;
      }
    "#],
        body: body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(r#"      const frame = window.frameElement;
      const serverId = String(frame?.dataset?.linceServerId || "").trim();
      const viewIdRaw = String(frame?.dataset?.linceViewId || "").trim();
      const viewId = Number(viewIdRaw);
      const titleEl = document.getElementById("title");
      const statusEl = document.getElementById("status");
      const outputEl = document.getElementById("output");

      function setStatus(text, state) {
        statusEl.textContent = text;
        statusEl.dataset.state = state;
      }

      function setOutput(value) {
        outputEl.textContent = value;
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

      function buildStreamUrl() {
        if (!serverId) {
          return { error: "Widget sem server_id. Configure o servidor no host." };
        }

        if (!Number.isInteger(viewId) || viewId <= 0) {
          return { error: "Widget sem view_id valido. Configure a view no host." };
        }

        return {
          url:
            "/host/integrations/servers/" +
            encodeURIComponent(serverId) +
            "/views/" +
            encodeURIComponent(viewId) +
            "/stream",
        };
      }

      async function streamView() {
        titleEl.textContent = Number.isInteger(viewId) && viewId > 0 ? `view/${viewId} stream` : "View stream";
        const target = buildStreamUrl();
        if (target.error) {
          setStatus("Configurar", "connecting");
          setOutput(target.error);
          return;
        }

        setStatus("Abrindo stream local...", "connecting");
        const response = await fetch(target.url, {
          headers: { Accept: "text/event-stream" },
        });

        if (response.status === 401) {
          window.LinceWidgetHost?.invalidateServerAuth?.(serverId);
          setStatus("Bloqueado", "connecting");
          setOutput("Servidor bloqueado. Entre com suas credenciais no host para desbloquear esse widget.");
          return;
        }

        if (!response.ok || !response.body) {
          const payload = await response.text().catch(() => "");
          throw new Error(payload || "Nao foi possivel abrir o stream.");
        }

        setStatus("Stream ativo", "live");

        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = "";

        while (true) {
          const { value, done } = await reader.read();
          if (done) {
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
            if (!event.data) {
              continue;
            }

            try {
              setOutput(JSON.stringify(JSON.parse(event.data), null, 2));
            } catch {
              setOutput(event.data);
            }
          }
        }

        setStatus("Stream encerrado", "connecting");
      }

      streamView().catch((error) => {
        setStatus("Erro", "connecting");
        setOutput(String(error && error.message ? error.message : error));
      });
    "#)],
    }
}

fn body() -> Markup {
    html! {
        main class="app" {
            header class="header" {
                span.eyebrow { "SSE view" }
                h1.title id="title" { "View stream" }
                p class="copy" { "Consome o stream remoto por meio do backend local do Lince." }
            }
             .status id="status" data-state="connecting" { "Aguardando configuracao..." }
            pre id="output" { "Defina um servidor e uma view no host." }
        }
    }
}
