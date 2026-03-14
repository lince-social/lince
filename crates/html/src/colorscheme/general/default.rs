pub fn presentation_colorscheme_general_default() -> &'static str {
    "
        /* Colorscheme Colors */
        --perfect-blue:  #A6BDD5;
        --dark-perfect-blue: #9bafc4;
        --darker-perfect-blue: #91a8bf;

        --white: white;

        --active-txt: #9aa7c7;
        --active-bg: #9bb1c6;
        --active-bg-hover: #96a9ba;

        --inactive-txt: #8f79a0;
        --inactive-bg: #71889e;
        --inactive-bg-hover: #7a93a8;
        --thin-border: rgba(255, 255, 255, 0.35);

        /* Lince's Colors */
        --background-color: var(--perfect-blue);
        --text-normal: var(--white);
        --border: var(--white);

        --active-button-txt: var(--white);
        --active-button-bg: var(--active-bg);
        --active-button-bg-hover: var(--active-bg-hover);
        --active-button-border: var(--white);

        --inactive-button-txt: var(--white);
        --inactive-button-bg: var(--inactive-bg);
        --inactive-button-bg-hover: var(--inactive-bg-hover);
        --inactive-button-border: var(--white);

        --table-border: var(--thin-border);
        --table-th-bg: var(--dark-perfect-blue);
        --table-td-bg: var(--darker-perfect-blue);

        --input-txt: var(--text-normal);
        --input-bg: var(--background-color);
        --input-border-color: var(--white);
        --input-focus-shadow: var(--white);
    "
}
