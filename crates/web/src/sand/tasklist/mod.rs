use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};
use maud::{Markup, PreEscaped, html};

const STYLE: &str = r#"
      :root {
        color-scheme: dark;
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
        --bg: #0f1217;
        --panel: #151a21;
        --panel-soft: #1a2028;
        --border: rgba(255, 255, 255, 0.08);
        --text: #eef2f7;
        --text-soft: #c7ced8;
        --text-muted: #89919d;
        --accent: #eef2f8;
        --accent-ink: #0a0d10;
        --success: #8dddb2;
        --danger: #ff8d99;
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
        align-content: start;
        gap: 14px;
        min-height: 100vh;
        padding: 16px;
        background: var(--bg);
        color: var(--text);
      }

      button,
      input {
        font: inherit;
      }

      .widget {
        min-height: auto;
        padding: 0;
      }

      .card {
        display: grid;
        gap: 14px;
        min-height: auto;
        padding: 0;
        border: 0;
        border-radius: 0;
        background: transparent;
      }

      .header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 12px;
      }

      .eyebrow,
      .count,
      .toolbar__button {
        font-size: 10px;
        font-weight: 600;
        letter-spacing: 0.14em;
        text-transform: uppercase;
      }

      .eyebrow {
        color: var(--text-muted);
      }

      .title {
        margin: 4px 0 0;
        font-size: 16px;
        font-weight: 600;
        letter-spacing: -0.03em;
      }

      .subtitle {
        margin: 6px 0 0;
        color: var(--text-soft);
        font-size: 13px;
      }

      .count {
        display: inline-flex;
        align-items: center;
        min-height: 28px;
        padding: 0 10px;
        border: 1px solid var(--border);
        border-radius: 999px;
        background: rgba(255, 255, 255, 0.03);
        color: var(--text-soft);
        white-space: nowrap;
      }

      .compose {
        display: grid;
        grid-template-columns: minmax(0, 1fr) auto;
        gap: 10px;
      }

      .compose__input {
        width: 100%;
        min-height: 40px;
        padding: 0 13px;
        border: 1px solid var(--border);
        border-radius: 14px;
        background: var(--panel-soft);
        color: var(--text);
        outline: none;
      }

      .compose__button,
      .task__toggle,
      .task__remove,
      .toolbar__button {
        border: 1px solid var(--border);
        transition:
          border-color 160ms cubic-bezier(0.22, 1, 0.36, 1),
          background 160ms cubic-bezier(0.22, 1, 0.36, 1),
          color 160ms cubic-bezier(0.22, 1, 0.36, 1),
          transform 160ms cubic-bezier(0.22, 1, 0.36, 1);
      }

      .compose__button {
        min-width: 42px;
        min-height: 40px;
        padding: 0 12px;
        border-radius: 14px;
        background: var(--accent);
        color: var(--accent-ink);
        cursor: pointer;
      }

      .toolbar {
        display: flex;
        justify-content: flex-end;
      }

      .toolbar__button {
        min-height: 30px;
        padding: 0 10px;
        border-radius: 999px;
        background: transparent;
        color: var(--text-muted);
        cursor: pointer;
      }

      .toolbar__button[disabled] {
        opacity: 0.4;
        cursor: not-allowed;
      }

      .list {
        display: grid;
        gap: 8px;
        align-content: start;
      }

      .task {
        display: grid;
        grid-template-columns: auto minmax(0, 1fr) auto;
        gap: 10px;
        align-items: center;
        padding: 12px;
        border: 1px solid var(--border);
        border-radius: 16px;
        background: var(--panel-soft);
      }

      .task--done {
        background: rgba(255, 255, 255, 0.02);
      }

      .task__toggle {
        display: inline-grid;
        place-items: center;
        width: 20px;
        height: 20px;
        padding: 0;
        border-radius: 999px;
        background: #101318;
        color: transparent;
        cursor: pointer;
      }

      .task--done .task__toggle {
        background: var(--success);
        color: #09100c;
      }

      .task__toggle svg,
      .task__remove svg {
        width: 11px;
        height: 11px;
      }

      .task__title {
        color: var(--text);
        font-size: 13px;
        font-weight: 500;
        line-height: 1.35;
        word-break: break-word;
      }

      .task--done .task__title {
        color: var(--text-muted);
        text-decoration: line-through;
      }

      .task__remove {
        display: inline-grid;
        place-items: center;
        width: 24px;
        height: 24px;
        padding: 0;
        border-radius: 10px;
        background: transparent;
        color: var(--text-muted);
        cursor: pointer;
      }

      .task__remove:hover {
        color: var(--danger);
        border-color: rgba(255, 141, 153, 0.18);
        background: rgba(255, 141, 153, 0.08);
      }

      .empty {
        padding: 18px 14px;
        border: 1px dashed rgba(255, 255, 255, 0.1);
        border-radius: 18px;
        color: var(--text-muted);
        font-size: 13px;
        text-align: center;
      }

      @media (max-width: 420px) {
        .compose {
          grid-template-columns: 1fr;
        }
      }
"#;

const BODY: &str = r##"
    <section class="widget">
      <article class="card">
        <div class="header">
          <div>
            <span class="eyebrow">Tasks</span>
            <h1 class="title">Tasklist</h1>
            <p class="subtitle">Fila curta, editavel e persistida localmente.</p>
          </div>
          <span id="count" class="count">0 abertas</span>
        </div>

        <form id="compose" class="compose">
          <input
            id="task-input"
            class="compose__input"
            type="text"
            maxlength="72"
            autocomplete="off"
            placeholder="Adicionar tarefa"
          />
          <button class="compose__button" type="submit" aria-label="Adicionar tarefa">+</button>
        </form>

        <div class="toolbar">
          <button id="clear-completed" class="toolbar__button" type="button">Limpar feitas</button>
        </div>

        <div id="task-list" class="list"></div>
      </article>
    </section>
"##;

const SCRIPT: &str = r##"
      (() => {
        const instanceId =
          window.frameElement?.dataset?.packageInstanceId || "default";
        const STORAGE_KEY = `lince-tasklist-package/v4/${instanceId}`;
        const initialTasks = [
          { id: "1", title: "Validar os imports .lince", done: false },
          { id: "2", title: "Revisar o board atual", done: true },
          { id: "3", title: "Desenhar os proximos widgets", done: false },
        ];

        const compose = document.getElementById("compose");
        const input = document.getElementById("task-input");
        const taskList = document.getElementById("task-list");
        const count = document.getElementById("count");
        const clearCompleted = document.getElementById("clear-completed");

        function createId() {
          return `${Date.now()}-${Math.round(Math.random() * 1000)}`;
        }

        function loadTasks() {
          try {
            const raw = window.localStorage.getItem(STORAGE_KEY);
            if (!raw) {
              return initialTasks.slice();
            }

            const parsed = JSON.parse(raw);
            if (!Array.isArray(parsed)) {
              return initialTasks.slice();
            }

            return parsed
              .map((task) => ({
                id: String(task.id || createId()),
                title: String(task.title || "").trim(),
                done: Boolean(task.done),
              }))
              .filter((task) => task.title);
          } catch {
            return initialTasks.slice();
          }
        }

        let tasks = loadTasks();

        function saveTasks() {
          try {
            window.localStorage.setItem(STORAGE_KEY, JSON.stringify(tasks));
          } catch {}
        }

        function escapeHtml(value) {
          return String(value)
            .replaceAll("&", "&amp;")
            .replaceAll("<", "&lt;")
            .replaceAll(">", "&gt;")
            .replaceAll('"', "&quot;")
            .replaceAll("'", "&#39;");
        }

        function render() {
          const openTasks = tasks.filter((task) => !task.done).length;
          count.textContent = `${openTasks} abertas`;
          clearCompleted.disabled = !tasks.some((task) => task.done);

          if (!tasks.length) {
            taskList.innerHTML = `
              <div class="empty">Sem tarefas. Adicione a primeira para ativar o card.</div>
            `;
            return;
          }

          taskList.innerHTML = tasks
            .map(
              (task) => `
                <article class="task${task.done ? " task--done" : ""}" data-task-id="${task.id}">
                  <button class="task__toggle" type="button" data-action="toggle" aria-label="Alternar status">
                    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                      <path d="m3.5 8 3 3 6-6"></path>
                    </svg>
                  </button>
                  <div class="task__title">${escapeHtml(task.title)}</div>
                  <button class="task__remove" type="button" data-action="remove" aria-label="Remover tarefa">
                    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                      <path d="M4 4l8 8"></path>
                      <path d="M12 4 4 12"></path>
                    </svg>
                  </button>
                </article>
              `,
            )
            .join("");
        }

        function commit() {
          saveTasks();
          render();
        }

        compose.addEventListener("submit", (event) => {
          event.preventDefault();

          const title = input.value.trim();
          if (!title) {
            input.focus();
            return;
          }

          tasks = [{ id: createId(), title, done: false }, ...tasks];
          input.value = "";
          commit();
          input.focus();
        });

        taskList.addEventListener("click", (event) => {
          const actionButton = event.target.closest("[data-action]");
          const taskNode = event.target.closest("[data-task-id]");
          if (!actionButton || !taskNode) {
            return;
          }

          const taskId = taskNode.dataset.taskId;
          const action = actionButton.dataset.action;

          if (action === "toggle") {
            tasks = tasks.map((task) =>
              task.id === taskId ? { ...task, done: !task.done } : task,
            );
            commit();
            return;
          }

          if (action === "remove") {
            tasks = tasks.filter((task) => task.id !== taskId);
            commit();
          }
        });

        clearCompleted.addEventListener("click", () => {
          tasks = tasks.filter((task) => !task.done);
          commit();
        });

        render();
      })();
"##;

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "tasklist.html",
        lang: "pt-BR",
        manifest: PackageManifest {
            icon: "☑".into(),
            title: "Tasklist".into(),
            author: "Lince Labs".into(),
            version: "0.2.0".into(),
            description:
                "Tasklist mais enxuta, com add, toggle, remove e persistencia local.".into(),
            details:
                "Widget independente de tarefas com estado salvo no proprio package via localStorage. O host so instala, posiciona e persiste o layout; a logica de tarefas continua inteiramente dentro do HTML.".into(),
            initial_width: 4,
            initial_height: 4,
            permissions: vec!["read_tasks".into(), "write_tasks".into()],
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
