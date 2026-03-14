pub fn presentation_colorscheme_catppuccin_macchiato() -> &'static str {
    "
        /* Colorscheme Colors */
        --rosewater: #f4dbd6;
        --flamingo: #f0c6c6;
        --pink: #f5bde6;
        --mauve: #c6a0f6;
        --red: #ed8796;
        --maroon: #ee99a0;
        --peach: #f5a97f;
        --yellow: #eed49f;
        --green: #a6da95;
        --teal: #8bd5ca;
        --sky: #91d7e3;
        --sapphire: #7dc4e4;
        --blue: #8aadf4;
        --lavender: #b7bdf8;

        --text: #cad3f5;
        --subtext1: #b8c0e0;
        --subtext0: #a5adcb;
        --overlay2: #939ab7;
        --overlay1: #8087a2;
        --overlay0: #6e738d;

        --surface2: #5b6078;
        --surface1: #494d64;
        --surface0: #363a4f;

        --base: #24273a;
        --mantle: #1e2030;
        --crust: #181926;

        --white: white;

        --thin-border-color: rgba(255, 255, 255, 0.35);

        /* Lince's Colors */
        --background-color: var(--crust);
        --text-normal: var(--text);
        --border: var(--text);

        --configuration-bg: var(--base);

        --active-button-txt: var(--surface1);
        --active-button-bg: var(--blue);
        --active-button-border: var(--blue);
        --active-button-bg-hover: var(--mauve);
        --active-button-border-hover: var(--mauve);

        --inactive-button-txt: var(--surface1);
        --inactive-button-bg: var(--red);
        --inactive-button-border: var(--red);
        --inactive-button-bg-hover: var(--peach);
        --inactive-button-border-hover: var(--peach);

        --table-border-color: var(--thin-border-color);
        --table-border-width: 0rem;
        --table-cell-border-width: 0.00625rem;
        --table-border-radius: 0.5rem;

        --table-th-bg: var(--base);
        --table-th-bg-hover: var(--surface0);

        --table-tr-bg-hover: var(--surface2);
        --table-td-bg: var(--surface1);
        --table-td-bg-hover: var(--surface2);

        --button-add-row-bg: var(--green);
        --button-add-row-border-radius: 1rem;
        --button-add-row-border-color: var(--green);
        --button-add-row-bg-hover: var(--teal);
        --button-add-row-border-color-hover: var(--teal);

        --button-collection-id-bg: var(--blue);
        --button-collection-id-bg-hover: var(--mauve);
        --button-collection-id-border-color: var(--blue);
        --button-collection-id-border-color-hover: var(--mauve);
        --button-collection-id-border-radius: 0.1rem;

        --input-txt: var(--text);
        --input-bg: var(--overlay0);
        --input-border-color: var(--subtext1);
        --input-focus-shadow: var(--pink);

        --modal-border-color: var(--rosewater);
    "
}
