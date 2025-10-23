use crate::{
    application::configuration::get_active_colorscheme,
    infrastructure::cross_cutting::InjectedServices,
};

pub async fn presentation_html_style(services: InjectedServices) -> String {
    "<style>
        :root {"
        .to_string()
        + get_active_colorscheme(services.clone()).await
        + "}

        body {
            background-color: var(--background-color);
            color: var(--text-normal);
            margin: 0.5rem;
        }

        main {
            display: flex;
            flex-direction: row;
            gap: 1rem;
            flex-wrap: wrap;
        }

        table, th, td {
            border-collapse: collapse;
            padding: 0.25rem;
        }
        table {
            border: var(--table-border-width) solid var(--table-border-color);
            border-radius: var(--table-border-radius);
        }
        th, td {
            border: var(--table-cell-border-width) solid var(--table-border);
        }
        .rounded-table {
            border-collapse: separate;
            border-spacing: 0;
            overflow: hidden;
        }

        .rounded-table th,
        .rounded-table td {
            padding: 0.5rem;
        }

        .rounded-table .top-left {
            border-top-left-radius: 0.75rem;
        }
        .rounded-table .top-right {
            border-top-right-radius: 0.75rem;
        }
        .rounded-table .bottom-left {
            border-bottom-left-radius: 0.75rem;
        }
        .rounded-table .bottom-right {
            border-bottom-right-radius: 0.75rem;
        }

        .separa {
            justify-content: space-between;
        }

        .fence--row {
            display: flex;
            flex-direction: row;
        }
        .fence--row > * {
            border: none; /* start with no borders */
        }
        .fence--row > * + * {
            border-left: 1.5px solid transparent;
        }

        .breakword {
            white-space: pre-wrap; word-break: break-word;
        }

        .fence-col {
            display: flex;
            /* flex-direction: column; */
        }
        .fence-col > * {
            border: none;
        }
        .fence-col > * + * {
            border-top: 1.33px solid transparent;
        }
        th {
            background-color: var(--table-th-bg);
        }
        th:hover {
            background-color: var(--table-th-bg-hover);
        }
        td {
            background-color: var(--table-td-bg);
        }
        td:hover {
            background-color: var(--table-td-bg-hover);
        }

        input {
            all: unset;
            color: var(--input-txt);
            background: var(--input-bg);
            border: 1px solid var(--input-border-color);
            padding: 0.25rem;
            border-radius: 2px;
        }

        input:focus {
            outline: none;
            box-shadow: 0 0 5px 2px var(--input-focus-shadow);
        }

        .autosize-textarea {
            white-space: pre; /* preserve newlines, do not wrap */
            word-break: normal;
            overflow: auto; /* allow scrollbars when needed and show resize UI */
            resize: both; /* let users resize horizontally and vertically */
            min-width: 3rem;
            max-width: 100%;
            box-sizing: border-box;
            padding: 0.25rem;
            border: 1px solid var(--input-border-color);
            border-radius: 2px;
            background: var(--input-bg);
            color: var(--input-txt);
        }

        .plain-button {
            all: unset;
            cursor: pointer;
            white-space: pre-wrap;
            word-break: break-word;
            display: inline-block;
        }

        .configurations {
            background: var(--configuration-bg);
            padding: 0.5rem;
        }

        .active {
            background-color: var(--active-button-bg);
            color: var(--active-button-txt);
            border: 1px solid var(--active-button-border);
            border-radius: 2px;
        }
        .active:hover {
            background-color: var(--active-button-bg-hover);
            color: var(--active-button-txt);
            border: 1px solid var(--active-button-border-hover);
            border-radius: 2px;
        }

        .inactive {
            background-color: var(--inactive-button-bg);
            color: var(--inactive-button-txt);
            border: 1px solid var(--inactive-button-border);
            border-radius: 2px;
        }
        .inactive:hover {
            background-color: var(--inactive-button-bg-hover);
            color: var(--inactive-button-txt);
            border: 1px solid var(--inactive-button-border-hover);
            border-radius: 2px;
        }

        .filled {
            background: var(--background-color);
        }

        .middle_y {
            display: flex;
            align-items: center;
        }

        .modal {
            position: fixed;
            top: 50%;
            left: 50%;
            transform: translate(-50%, -50%);
            z-index: 1;
        }

        .shy {
            width: min-content;
            height: min-content;
        }

        .row {
            display: flex;
            flex-direction: row;
        }

        .column {
            display: flex;
            flex-direction: column;
        }

        .xs_gap {
            gap: 0.25rem;
        }

        .s_gap {
            display: flex;
            gap: 0.5rem;
        }

        #button-add-row {
            background: var(--button-add-row-bg);
            border: 1px solid var(--button-add-row-border-color);
            border-radius: var(--button-add-row-border-radius);
        }
        #button-add-row:hover {
            background: var(--button-add-row-bg-hover);
            border: 1px solid var(--button-add-row-border-color-hover);
        }

        .s_padding{
            padding: 0.25rem;
        }
        .m_padding{
            padding: 0.5rem;
        }

        .s_margin{
            margin: 0.25rem;
        }

        .stripped {
            padding: 0rem;
            margin: 0rem;
        }

        /* Karma cell styles for combined condition/consequence display */
        .karma-cell {
            display: flex;
            flex-direction: column;
            gap: 0.25rem;
        }

        .karma-primary {
            font-weight: bold;
            font-size: 0.9em;
        }

        .karma-secondary {
            font-size: 0.8em;
            opacity: 0.8;
            font-style: italic;
        }

        .glow {
        box-shadow: 0 0 20px white, 0 0 40px white, 0 0 60px white;
        animation: glow 10s infinite alternate;
      }

      @keyframes glow {
        from {
          box-shadow: 0 0 1rem white, 0 0 2rem white;
        }
        to {
          box-shadow: 0 0 1.1rem white, 0 0 2.2rem white;
        }
        }

    </style>"
}
