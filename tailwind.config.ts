import type { Config } from "tailwindcss";
import plugin from "tailwindcss/plugin";

export default {
  content: [
    "./src/components/**/*.{js,ts,jsx,tsx,mdx}",
    "./src/app/**/*.{js,ts,jsx,tsx,mdx}",
  ],
  theme: {
    extend: {
      colors: {
        "rosewater-theme": "var(--rosewater)",
        "flamingo-theme": "var(--flamingo)",
        "pink-theme": "var(--pink)",
        "mauve-theme": "var(--mauve)",
        "red-theme": "var(--red)",
        "maroon-theme": "var(--maroon)",
        "peach-theme": "var(--peach)",
        "yellow-theme": "var(--yellow)",
        "green-theme": "var(--green)",
        "teal-theme": "var(--teal)",
        "sky-theme": "var(--sky)",
        "sapphire-theme": "var(--sapphire)",
        "blue-theme": "var(--blue)",
        "lavender-theme": "var(--lavender)",
        "text-theme": "var(--text)",
        "subtext1-theme": "var(--subtext1)",
        "subtext0-theme": "var(--subtext0)",
        "overlay2-theme": "var(--overlay2)",
        "overlay1-theme": "var(--overlay1)",
        "overlay0-theme": "var(--overlay0)",
        "surface2-theme": "var(--surface2)",
        "surface1-theme": "var(--surface1)",
        "surface0-theme": "var(--surface0)",
        "base-theme": "var(--base)",
        "mantle-theme": "var(--mantle)",
        "crust-theme": "var(--crust)",
        "background-theme": "var(--background)", // From your custom theme
        "foreground-theme": "var(--foreground)", // From your custom theme
      },
    },
  },
  plugins: [
    plugin(function({ addBase }) {
      addBase({
        body: {
          color: "var(--text)", // Use the custom text color
          backgroundColor: "var(--base)", // Use the custom background color
        },
      });
    }),
  ],
} satisfies Config;
