import { Elysia } from "elysia";
import { html } from "@elysiajs/html";
import * as elements from "typed-html";
import { getActiveConfiguration } from "./utils/getData";

export default async function page() {
    function BaseHtml({ children }: elements.Children) {
        return `<!DOCTYPE html>
    <html lang="en">
      <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <meta http-equiv="X-UA-Compatible" content="ie=edge">
        <title>Lince</title>
        <!-- <link rel="stylesheet" href="style.css"> -->
      </head>
        ${children}
    </html>`;
    }

    const app = new Elysia()
        .use(html())
        .get("/", async () => {
            const data = await getActiveConfiguration();
            return data[0].configurationname;
        })
        .get("/based", ({ html }) => html(<BasebaseHtml))
        .listen(3000);
    console.log(`Serving at ${app.server?.hostname}:${app.server?.port}`);
}

page();
