import { Elysia } from "elysia";
import { html } from "@elysiajs/html";
import * as elements from "typed-html";
import { getActiveConfiguration } from "./utils/getData";

export default async function page() {
    function Home({ children }: elements.Children) {
        return `<!DOCTYPE html>
    <html lang="en">
      <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <meta http-equiv="X-UA-Compatible" content="ie=edge">
        <title>Lince</title>
        <!-- <link rel="stylesheet" href="style.css"> -->
        <script src="https://unpkg.com/htmx.org@2.0.4"></script>
        <script src="https://cdn.tailwindcss.com"></script>
      </head>
        ${children}
    </html>`;
    }

    async function ActiveConfiguration() {
        const data = await getActiveConfiguration();
        const activeConfiguration = data[0].configurationname;
        return (
            <p class="bg-red-400 hover:bg-red-300 max-w-min rounded p-2">
                {activeConfiguration}
            </p>
        );
    }

    const app = new Elysia()
        .use(html())
        .get("/", async ({ html }) => {
            const activeConfigElement = await ActiveConfiguration();
            return html(
                <body>
                    <Home>{activeConfigElement}</Home>
                    <button class="bg-blue-500" hx-post="/but" hx-swap="outerHTML">
                        Click Me!
                    </button>
                </body>,
            );
        })
        .post("/but", () => <div>You clicked me hmmm, fuck!</div>)
        .listen(3000);

    console.log(`Serving at ${app.server?.hostname}:${app.server?.port}`);
}

page();
