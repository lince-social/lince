pub fn presentation_colorscheme_mono_white_in_black() -> &'static str {
    "
        /* Colorscheme Colors */
        --white: white;
        --lightgray: #ededed;

        --black: black;
        --darkgray: #444444;

        --thin-border: rgba(255, 255, 255, 0.35);

        /* Lince's Colors */
        --background-color: var(--black);
        --text-normal: var(--white);
        --border: var(--white);

        --active-button-txt: var(--black);
        --active-button-bg: var(--white);
        --active-button-bg-hover: var(--lightgray);
        --active-button-border: var(--white);

        --inactive-button-txt: var(--white);
        --inactive-button-bg: var(--black);
        --inactive-button-bg-hover: var(--darkgray);
        --inactive-button-border: var(--black);

        --table-border: var(--thin-border);
        --table-th-bg: var(--black);
        --table-td-bg: var(--black);

        --input-txt: var(--white);
        --input-bg: var(--black);
        --input-border-color: var(--white);
        --input-focus-shadow: var(--white);
    "
}
