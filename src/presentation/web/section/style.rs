use crate::infrastructure::cross_cutting::InjectedServices;

pub async fn presentation_web_style(services: InjectedServices) -> String {
    "<style>
        :root {"
        .to_string()
        + services
            .use_cases
            .configuration
            .get_active_colorscheme
            .execute(services.clone())
            .await
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

        s_padding{
            padding: 0.25rem;
        }

    </style>"
}
