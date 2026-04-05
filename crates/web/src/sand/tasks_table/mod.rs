use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};
use maud::{Markup, PreEscaped, html};

pub(crate) const FEATURE_FLAG: &str = "sand.tasks_table";

const STYLE: &str = r#"
      :root {
        color-scheme: dark;
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
        --bg: #161a20;
        --bg-soft: #1b2027;
        --bg-muted: #202631;
        --line: rgba(255, 255, 255, 0.07);
        --line-soft: rgba(255, 255, 255, 0.045);
        --text: #edf1f7;
        --text-soft: #c1c8d2;
        --text-muted: #7d8794;
        --mono: "IBM Plex Mono", "SFMono-Regular", monospace;
        --green-bg: rgba(34, 197, 125, 0.12);
        --green: #45df98;
        --orange-bg: rgba(249, 115, 22, 0.12);
        --orange: #ff9b44;
        --red-bg: rgba(239, 68, 68, 0.12);
        --red: #ff7676;
        --blue-bg: rgba(59, 130, 246, 0.12);
        --blue: #61a6ff;
        --purple-bg: rgba(139, 92, 246, 0.12);
        --purple: #b08aff;
        --cyan-bg: rgba(14, 165, 233, 0.12);
        --cyan: #59c5ff;
        --amber-bg: rgba(245, 158, 11, 0.12);
        --amber: #f4c253;
      }

      * {
        box-sizing: border-box;
      }

      html,
      body {
        min-height: 100%;
      }

      body {
        margin: 0;
        display: grid;
        grid-template-rows: auto auto 1fr auto;
        gap: 18px;
        min-height: 100vh;
        padding: 18px 20px;
        background: var(--bg);
        color: var(--text);
      }

      button,
      input {
        font: inherit;
      }

      .topbar {
        display: flex;
        justify-content: space-between;
        gap: 18px;
        align-items: flex-start;
      }

      .eyebrow {
        color: var(--text-muted);
        font-family: var(--mono);
        font-size: 11px;
        letter-spacing: 0.18em;
        text-transform: uppercase;
      }

      .header-title {
        display: flex;
        align-items: center;
        gap: 10px;
      }

      .warning {
        display: inline-grid;
        place-items: center;
        width: 16px;
        height: 16px;
        color: var(--amber);
      }

      .chips,
      .header-actions,
      .task-tags {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
        align-items: center;
      }

      .chips {
        margin-top: 8px;
      }

      .chip,
      .filter-button,
      .pill {
        display: inline-flex;
        align-items: center;
        min-height: 24px;
        padding: 0 9px;
        border: 1px solid var(--line);
        border-radius: 8px;
        background: rgba(255, 255, 255, 0.03);
        color: var(--text-soft);
        font-family: var(--mono);
        font-size: 11px;
        line-height: 1;
      }

      .chip--green,
      .pill--done {
        background: var(--green-bg);
        color: var(--green);
        border-color: rgba(69, 223, 152, 0.22);
      }

      .chip--cyan,
      .pill--progress {
        background: rgba(31, 177, 132, 0.13);
        color: #39d9a3;
        border-color: rgba(57, 217, 163, 0.22);
      }

      .chip--muted,
      .pill--todo {
        background: rgba(255, 255, 255, 0.03);
        color: var(--text-muted);
      }

      .filter-button {
        gap: 8px;
        background: transparent;
        cursor: pointer;
      }

      .filter-button svg {
        width: 13px;
        height: 13px;
      }

      .table {
        min-height: 0;
        overflow: auto;
        border-top: 1px solid var(--line);
      }

      .table-header,
      .table-row {
        display: grid;
        grid-template-columns: minmax(260px, 3fr) minmax(112px, 1fr) minmax(96px, 0.8fr) minmax(156px, 1.3fr) minmax(96px, 0.8fr);
        gap: 16px;
        align-items: center;
      }

      .table-header {
        min-height: 42px;
        color: var(--text-muted);
        font-family: var(--mono);
        font-size: 11px;
        letter-spacing: 0.08em;
        text-transform: uppercase;
      }

      .header-cell {
        padding-top: 8px;
      }

      .sort-button {
        display: inline-flex;
        gap: 6px;
        align-items: center;
        padding: 0;
        border: 0;
        background: transparent;
        color: inherit;
        font: inherit;
        letter-spacing: inherit;
        text-transform: inherit;
        cursor: pointer;
      }

      .sort-button svg {
        width: 12px;
        height: 12px;
        opacity: 0.65;
        transition: transform 160ms ease;
      }

      .table-body {
        border-top: 1px solid var(--line-soft);
      }

      .table-row {
        min-height: 58px;
        border-bottom: 1px solid var(--line-soft);
      }

      .task-main {
        display: grid;
        grid-template-columns: auto minmax(0, 1fr);
        gap: 14px;
        align-items: center;
        min-width: 0;
      }

      .task-icon {
        display: inline-grid;
        place-items: center;
        width: 22px;
        height: 22px;
        color: var(--text-muted);
      }

      .task-icon svg {
        width: 16px;
        height: 16px;
      }

      .task-icon--done {
        color: var(--green);
      }

      .task-icon--progress {
        color: #31d0a1;
      }

      .task-name {
        min-width: 0;
        color: var(--text);
        font-size: 13px;
        font-weight: 500;
        line-height: 1.5;
      }

      .task-name--done {
        color: #8e97a3;
        text-decoration: line-through;
      }

      .pill--high {
        background: var(--orange-bg);
        color: var(--orange);
        border-color: rgba(255, 155, 68, 0.22);
      }

      .pill--medium {
        background: rgba(234, 179, 8, 0.12);
        color: #efc94c;
        border-color: rgba(239, 201, 76, 0.22);
      }

      .pill--critical {
        background: var(--red-bg);
        color: var(--red);
        border-color: rgba(255, 118, 118, 0.22);
      }

      .tag {
        display: inline-flex;
        align-items: center;
        min-height: 24px;
        padding: 0 8px;
        border: 1px solid transparent;
        border-radius: 7px;
        font-family: var(--mono);
        font-size: 11px;
      }

      .tag--frontend {
        background: var(--purple-bg);
        color: var(--purple);
        border-color: rgba(176, 138, 255, 0.2);
      }

      .tag--feature {
        background: var(--cyan-bg);
        color: var(--cyan);
        border-color: rgba(89, 197, 255, 0.2);
      }

      .tag--docs {
        background: rgba(250, 204, 21, 0.12);
        color: #f6d05c;
        border-color: rgba(246, 208, 92, 0.22);
      }

      .tag--backend {
        background: rgba(16, 185, 129, 0.12);
        color: #37d39f;
        border-color: rgba(55, 211, 159, 0.22);
      }

      .tag--devops {
        background: rgba(236, 72, 153, 0.12);
        color: #ef6bb1;
        border-color: rgba(239, 107, 177, 0.22);
      }

      .tag--bug {
        background: rgba(239, 68, 68, 0.12);
        color: #ff7e7e;
        border-color: rgba(255, 126, 126, 0.22);
      }

      .date {
        color: #a2acb8;
        font-family: var(--mono);
        font-size: 12px;
      }

      .compose {
        display: grid;
        grid-template-columns: minmax(0, 1fr) auto;
        gap: 12px;
        padding-top: 12px;
        border-top: 1px solid var(--line);
      }

      .compose__input {
        width: 100%;
        min-height: 34px;
        padding: 0 14px;
        border: 1px solid var(--line);
        border-radius: 10px;
        background: var(--bg-soft);
        color: var(--text-soft);
      }

      .compose__button {
        width: 34px;
        height: 34px;
        border: 0;
        border-radius: 11px;
        background: #27c89a;
        color: #07110e;
        font-size: 22px;
        line-height: 1;
        cursor: pointer;
      }

      @media (max-width: 860px) {
        body {
          padding: 16px;
        }

        .table-header,
        .table-row {
          min-width: 760px;
        }
      }
"#;

const BODY: &str = r##"
    <header class="topbar">
      <div>
        <div class="header-title">
          <span class="warning" aria-hidden="true">
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M8 2.5 14 13H2L8 2.5Z"></path>
              <path d="M8 6v3.5"></path>
              <path d="M8 12.1h.01"></path>
            </svg>
          </span>
          <span class="eyebrow">Tarefas</span>
        </div>
        <div id="summary-chips" class="chips"></div>
      </div>
      <div class="header-actions">
        <button class="filter-button" type="button" aria-label="Filtros">
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
            <path d="M2.5 3.5h11l-4 4.6v3.2l-3 1.2V8.1l-4-4.6Z"></path>
          </svg>
          Filtros
        </button>
      </div>
    </header>

    <section class="table">
      <div class="table-header">
        <div class="header-cell">
          <button class="sort-button" type="button" data-sort-key="task">
            Tarefa
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="m4 6 4 4 4-4"></path>
            </svg>
          </button>
        </div>
        <div class="header-cell">
          <button class="sort-button" type="button" data-sort-key="status">
            Status
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="m4 6 4 4 4-4"></path>
            </svg>
          </button>
        </div>
        <div class="header-cell">
          <button class="sort-button" type="button" data-sort-key="priority">
            Prioridade
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="m4 6 4 4 4-4"></path>
            </svg>
          </button>
        </div>
        <div class="header-cell">Tags</div>
        <div class="header-cell">
          <button class="sort-button" type="button" data-sort-key="date">
            Data
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="m4 6 4 4 4-4"></path>
            </svg>
          </button>
        </div>
      </div>
      <div id="table-body" class="table-body"></div>
    </section>

    <footer class="compose">
      <input class="compose__input" type="text" placeholder="Nova tarefa..." aria-label="Nova tarefa" />
      <button class="compose__button" type="button" aria-label="Adicionar">+</button>
    </footer>
"##;

const SCRIPT: &str = r##"
      (() => {
        const rows = [
          {
            task: "Revisar PR do frontend",
            status: "todo",
            priority: "high",
            tags: ["frontend", "feature"],
            date: "2026-03-10",
          },
          {
            task: "Atualizar documentacao da API",
            status: "done",
            priority: "medium",
            tags: ["docs", "backend"],
            date: "2026-03-06",
          },
          {
            task: "Deploy staging environment",
            status: "progress",
            priority: "critical",
            tags: ["devops"],
            date: "2026-03-07",
          },
          {
            task: "Code review do modulo auth",
            status: "todo",
            priority: "high",
            tags: ["backend", "feature"],
            date: "2026-03-09",
          },
          {
            task: "Corrigir bug no login OAuth",
            status: "todo",
            priority: "critical",
            tags: ["bug", "backend"],
            date: "2026-03-08",
          },
          {
            task: "Merge branch feature/dashboard",
            status: "done",
            priority: "low",
            tags: ["frontend"],
            date: "2026-03-05",
          },
          {
            task: "Setup CI/CD pipeline",
            status: "progress",
            priority: "medium",
            tags: ["devops"],
            date: "2026-03-12",
          },
        ];

        const state = {
          sortKey: "date",
          direction: "desc",
        };

        const priorityRank = {
          critical: 4,
          high: 3,
          medium: 2,
          low: 1,
        };

        const statusRank = {
          progress: 3,
          todo: 2,
          done: 1,
        };

        const statusLabel = {
          todo: "A fazer",
          progress: "Em progresso",
          done: "Concluido",
        };

        const priorityLabel = {
          critical: "Critico",
          high: "Alto",
          medium: "Medio",
          low: "Baixo",
        };

        const tableBody = document.getElementById("table-body");
        const summaryChips = document.getElementById("summary-chips");
        const buttons = Array.from(document.querySelectorAll("[data-sort-key]"));

        function formatDate(value) {
          const date = new Date(`${value}T12:00:00`);
          return date.toLocaleDateString("pt-BR", {
            day: "2-digit",
            month: "short",
          });
        }

        function renderSummary() {
          const todo = rows.filter((row) => row.status === "todo").length;
          const progress = rows.filter((row) => row.status === "progress").length;
          const done = rows.filter((row) => row.status === "done").length;

          summaryChips.innerHTML = `
            <span class="chip chip--muted">${todo} a fazer</span>
            <span class="chip chip--cyan">${progress} em progresso</span>
            <span class="chip chip--green">${done} concluidas</span>
          `;
        }

        function renderIcon(status) {
          if (status === "done") {
            return `
              <span class="task-icon task-icon--done" aria-hidden="true">
                <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                  <path d="m3.5 8 3 3 6-6"></path>
                </svg>
              </span>
            `;
          }

          if (status === "progress") {
            return `
              <span class="task-icon task-icon--progress" aria-hidden="true">
                <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M12.7 8A4.7 4.7 0 1 1 8 3.3"></path>
                </svg>
              </span>
            `;
          }

          return `
            <span class="task-icon" aria-hidden="true">
              <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
                <circle cx="8" cy="8" r="5.6"></circle>
              </svg>
            </span>
          `;
        }

        function renderTags(tags) {
          return tags
            .map((tag) => `<span class="tag tag--${tag}">${tag}</span>`)
            .join("");
        }

        function sortRows() {
          const sorted = [...rows].sort((left, right) => {
            let comparison = 0;

            if (state.sortKey === "priority") {
              comparison =
                priorityRank[left.priority] - priorityRank[right.priority];
            } else if (state.sortKey === "status") {
              comparison = statusRank[left.status] - statusRank[right.status];
            } else if (state.sortKey === "date") {
              comparison =
                new Date(left.date).getTime() - new Date(right.date).getTime();
            } else {
              comparison = String(left[state.sortKey]).localeCompare(
                String(right[state.sortKey]),
                "pt-BR",
              );
            }

            return state.direction === "asc" ? comparison : -comparison;
          });

          return sorted;
        }

        function renderTable() {
          const sorted = sortRows();

          tableBody.innerHTML = sorted
            .map(
              (row) => `
                <article class="table-row">
                  <div class="task-main">
                    ${renderIcon(row.status)}
                    <div class="task-name${row.status === "done" ? " task-name--done" : ""}">
                      ${row.task}
                    </div>
                  </div>
                  <div><span class="pill pill--${row.status}">${statusLabel[row.status]}</span></div>
                  <div><span class="pill pill--${row.priority}">${priorityLabel[row.priority]}</span></div>
                  <div class="task-tags">${renderTags(row.tags)}</div>
                  <div class="date">${formatDate(row.date)}</div>
                </article>
              `,
            )
            .join("");

          buttons.forEach((button) => {
            const isActive = button.dataset.sortKey === state.sortKey;
            button.style.color = isActive ? "var(--text)" : "var(--text-muted)";
            button.querySelector("svg").style.transform =
              isActive && state.direction === "desc"
                ? "rotate(180deg)"
                : "rotate(0deg)";
          });
        }

        buttons.forEach((button) => {
          button.addEventListener("click", () => {
            const key = button.dataset.sortKey;

            if (state.sortKey === key) {
              state.direction = state.direction === "asc" ? "desc" : "asc";
            } else {
              state.sortKey = key;
              state.direction = key === "date" ? "desc" : "asc";
            }

            renderTable();
          });
        });

        renderSummary();
        renderTable();
      })();
"##;

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "tasks-table.html",
        lang: "pt-BR",
        manifest: PackageManifest {
            icon: "▦".into(),
            title: "Tasks table".into(),
            author: "Lince Labs".into(),
            version: "0.3.0".into(),
            description:
                "Tabela de tarefas mais proxima de um painel operacional, com ordenacao, badges e ritmo visual compacto.".into(),
            details:
                "Esse widget empurra o package table para mais perto da referencia da imagem: cabecalho tecnico, chips de resumo, linhas com status e prioridade e uma densidade visual de painel serio. O sort continua acontecendo inteiro dentro do proprio HTML.".into(),
            initial_width: 6,
            initial_height: 5,
            requires_server: false,
            permissions: vec!["read_table".into(), "read_metrics".into()],
        },
        head_links: vec![],
        inline_styles: vec![STYLE],
        body: body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(SCRIPT)],
    }
}

fn body() -> Markup {
    html! {
        (PreEscaped(BODY))
    }
}
