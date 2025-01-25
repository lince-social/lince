import { Elysia } from "elysia";
import { html } from "@elysiajs/html";
import * as elements from "typed-html";
import Page from "./components/sections/Page";
import Body from "./components/sections/Body";

export default async function app() {
  const app = new Elysia()
    .use(html())
    .get("/", async ({ html }) => {
      const body = await Body();
      return html(
        <Page>
          <body>
            <div>{body}</div>
          </body>
        </Page>,
      );
    })
    .post("/but", () => <div>You clicked me hmmm, fuck!</div>)
    .listen(3000);

  console.log(`Serving at ${app.server?.hostname}:${app.server?.port}`);
}

app();
