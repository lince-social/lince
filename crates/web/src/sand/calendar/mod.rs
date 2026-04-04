use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};
use maud::{Markup, PreEscaped, html};

const STYLE: &str = r#"
      :root {
        color-scheme: dark;
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
        --bg: #171b21;
        --bg-soft: #1c2129;
        --cell: #232a33;
        --line: rgba(255, 255, 255, 0.07);
        --text: #edf1f7;
        --text-soft: #c3cad5;
        --text-muted: #7f8894;
        --green: #29d3a1;
        --blue: #58a8ff;
        --orange: #ff9f39;
        --pink: #ef4e8b;
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
        grid-template-rows: auto auto auto 1fr;
        gap: 16px;
        min-height: 100vh;
        padding: 18px;
        background: var(--bg);
        color: var(--text);
      }

      button {
        font: inherit;
      }

      .eyebrow {
        color: var(--text-muted);
        font-size: 11px;
        font-weight: 500;
        letter-spacing: 0.18em;
        text-transform: uppercase;
      }

      .header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        gap: 12px;
      }

      .month-title {
        color: var(--text);
        font-size: 16px;
        font-weight: 600;
      }

      .nav {
        display: flex;
        gap: 6px;
      }

      .nav button {
        display: inline-grid;
        place-items: center;
        width: 28px;
        height: 28px;
        border: 1px solid var(--line);
        border-radius: 9px;
        background: var(--bg-soft);
        color: var(--text-soft);
        cursor: pointer;
      }

      .nav button svg {
        width: 13px;
        height: 13px;
      }

      .weekday-row,
      .days {
        display: grid;
        grid-template-columns: repeat(7, minmax(0, 1fr));
        gap: 8px;
      }

      .weekday {
        color: var(--text-muted);
        text-align: center;
        font-size: 11px;
      }

      .days {
        align-content: start;
      }

      .day {
        position: relative;
        display: grid;
        place-items: center;
        min-height: 34px;
        border: 1px solid transparent;
        border-radius: 12px;
        background: transparent;
        color: var(--text-soft);
        cursor: pointer;
        transition:
          border-color 160ms ease,
          background 160ms ease,
          color 160ms ease;
      }

      .day--outside {
        color: rgba(127, 136, 148, 0.35);
      }

      .day--today {
        background: var(--cell);
        color: var(--text);
      }

      .day--selected {
        border-color: rgba(41, 211, 161, 0.28);
        background: rgba(41, 211, 161, 0.12);
        color: var(--green);
        font-weight: 600;
      }

      .day__dots {
        position: absolute;
        right: 5px;
        bottom: 4px;
        display: flex;
        gap: 3px;
      }

      .dot {
        width: 5px;
        height: 5px;
        border-radius: 999px;
      }

      .dot--blue {
        background: var(--blue);
      }

      .dot--orange {
        background: var(--orange);
      }

      .dot--pink {
        background: var(--pink);
      }

      .dot--green {
        background: var(--green);
      }
"#;

const BODY: &str = r##"
    <header>
      <span class="eyebrow">Calendario</span>
    </header>

    <section class="header">
      <div id="month-title" class="month-title"></div>
      <div class="nav">
        <button id="prev-button" type="button" aria-label="Mes anterior">
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
            <path d="m10 3.5-4 4.5 4 4.5"></path>
          </svg>
        </button>
        <button id="next-button" type="button" aria-label="Proximo mes">
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
            <path d="m6 3.5 4 4.5-4 4.5"></path>
          </svg>
        </button>
      </div>
    </section>

    <div class="weekday-row">
      <span class="weekday">D</span>
      <span class="weekday">S</span>
      <span class="weekday">T</span>
      <span class="weekday">Q</span>
      <span class="weekday">Q</span>
      <span class="weekday">S</span>
      <span class="weekday">S</span>
    </div>

    <section id="days" class="days"></section>
"##;

const SCRIPT: &str = r##"
      (() => {
        const instanceId =
          window.frameElement?.dataset?.packageInstanceId || "default";
        const STORAGE_KEY = `lince-calendar-package/v1/${instanceId}`;
        const monthTitle = document.getElementById("month-title");
        const daysNode = document.getElementById("days");
        const prevButton = document.getElementById("prev-button");
        const nextButton = document.getElementById("next-button");

        const today = new Date();
        today.setHours(12, 0, 0, 0);

        const state = loadState();

        const events = {
          "2026-03-07": ["blue"],
          "2026-03-12": ["orange"],
          "2026-03-15": ["green"],
          "2026-03-19": ["green"],
          "2026-03-20": ["pink"],
          "2026-03-28": ["blue"],
        };

        function createDefaultState() {
          return {
            monthOffset: 0,
            selectedDate: today.toISOString().slice(0, 10),
          };
        }

        function loadState() {
          try {
            const raw = window.localStorage.getItem(STORAGE_KEY);
            if (!raw) {
              return createDefaultState();
            }

            const parsed = JSON.parse(raw);
            return {
              monthOffset: Number(parsed?.monthOffset) || 0,
              selectedDate:
                typeof parsed?.selectedDate === "string"
                  ? parsed.selectedDate
                  : createDefaultState().selectedDate,
            };
          } catch {
            return createDefaultState();
          }
        }

        function saveState() {
          try {
            window.localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
          } catch {}
        }

        function getCurrentMonthDate() {
          return new Date(today.getFullYear(), today.getMonth() + state.monthOffset, 1, 12);
        }

        function toKey(date) {
          const year = date.getFullYear();
          const month = String(date.getMonth() + 1).padStart(2, "0");
          const day = String(date.getDate()).padStart(2, "0");
          return `${year}-${month}-${day}`;
        }

        function buildGrid(monthDate) {
          const firstDay = new Date(monthDate.getFullYear(), monthDate.getMonth(), 1, 12);
          const start = new Date(firstDay);
          start.setDate(firstDay.getDate() - firstDay.getDay());

          return Array.from({ length: 35 }, (_, index) => {
            const date = new Date(start);
            date.setDate(start.getDate() + index);
            return date;
          });
        }

        function render() {
          const monthDate = getCurrentMonthDate();
          const grid = buildGrid(monthDate);

          monthTitle.textContent = monthDate.toLocaleDateString("pt-BR", {
            month: "short",
            year: "numeric",
          }).replace(".", "");

          daysNode.innerHTML = grid
            .map((date) => {
              const key = toKey(date);
              const isOutside = date.getMonth() !== monthDate.getMonth();
              const isToday = key === toKey(today);
              const isSelected = key === state.selectedDate;
              const dots = (events[key] || [])
                .map((color) => `<span class="dot dot--${color}"></span>`)
                .join("");

              return `
                <button
                  class="day${isOutside ? " day--outside" : ""}${isToday ? " day--today" : ""}${isSelected ? " day--selected" : ""}"
                  type="button"
                  data-date="${key}"
                >
                  <span>${date.getDate()}</span>
                  ${dots ? `<span class="day__dots">${dots}</span>` : ""}
                </button>
              `;
            })
            .join("");
        }

        daysNode.addEventListener("click", (event) => {
          const button = event.target.closest("[data-date]");
          if (!button) {
            return;
          }

          state.selectedDate = button.dataset.date;
          saveState();
          render();
        });

        prevButton.addEventListener("click", () => {
          state.monthOffset -= 1;
          saveState();
          render();
        });

        nextButton.addEventListener("click", () => {
          state.monthOffset += 1;
          saveState();
          render();
        });

        render();
      })();
"##;

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "calendar.html",
        lang: "pt-BR",
        manifest: PackageManifest {
            icon: "◫".into(),
            title: "Calendar".into(),
            author: "Lince Labs".into(),
            version: "0.1.0".into(),
            description:
                "Calendario compacto no estilo de painel lateral, com selecao de dia e pontos de eventos.".into(),
            details:
                "Widget de calendario no mesmo vocabulario do relogio lateral: cabecalho pequeno, navegacao mensal, grade compacta e pequenos acentos de evento por dia. O estado de selecao e navegacao fica salvo localmente por instancia.".into(),
            initial_width: 3,
            initial_height: 3,
            requires_server: false,
            permissions: vec![],
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
