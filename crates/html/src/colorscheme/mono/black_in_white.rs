pub fn presentation_colorscheme_mono_black_in_white() -> &'static str {
    "
        /* Colorscheme Colors */
        --white: white;
        --lightgray: #ededed;

        --black: black;
        --darkgray: #444444;

        --thin-border: rgba(0, 0, 0, 0.35);

        /* Lince's Colors */
        --background-color: var(--white);
        --text-normal: var(--black);
        --border: var(--black);

        --active-button-txt: var(--white);
        --active-button-bg: var(--black);
        --active-button-bg-hover: var(--darkgray);
        --active-button-border: var(--black);

        --inactive-button-txt: var(--black);
        --inactive-button-bg: var(--white);
        --inactive-button-bg-hover: var(--lightgray);
        --inactive-button-border: var(--white);

        --table-border: var(--thin-border);
        --table-th-bg: var(--white);
        --table-td-bg: var(--white);

        --input-txt: var(--black);
        --input-bg: var(--white);
        --input-border-color: var(--black);
        --input-focus-shadow: var(--black);
    "
}
