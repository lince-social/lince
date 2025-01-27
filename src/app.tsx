import { Elysia } from "elysia";
import { html } from "@elysiajs/html";
import * as elements from "typed-html";

import Page from "./components/sections/Page";
import Body from "./components/sections/Body";
import ConfigurationsUnhovered, {
  ConfigurationsHovered,
} from "./components/Configurations";
import Tables from "./components/Tables";
import handleConfigurationClick from "./utils/handleConfiguration";
import { handleViewToggle } from "./utils/handleView";

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
    .post("/configurationclick/:id", async ({ params: { id } }) => {
      await handleConfigurationClick(id);
      return await (
        <main id="main">
          <div>{await ConfigurationsHovered()}</div>
          <div>{await Tables()}</div>
        </main>
      );
    })
    .post("/configurationhovered", async () => {
      return await ConfigurationsHovered();
    })
    .post("/configurationunhovered", async () => {
      return await ConfigurationsUnhovered();
    })
    .post("/view", async (context) => {
      const body = await context.body;
      const { views, view, configurationId } = body;
      await handleViewToggle(views, view, configurationId);
      return await (
        <main id="main">
          <div>{await ConfigurationsHovered()}</div>
          <div>{await Tables()}</div>
        </main>
      );
    })
    .get("/tables", Tables())
    .listen(3000);

  console.log(`Serving at ${app.server?.hostname}:${app.server?.port}`);
}

app();
