import { Elysia } from "elysia";
import { html } from "@elysiajs/html";
import * as elements from "typed-html";

import Page from "./components/sections/Page";
import Body from "./components/sections/Body";
import ConfigurationsUnhovered, {
  ConfigurationsHovered,
} from "./components/Configurations";
import Tables from "./components/Tables";
import handleConfigurationChange from "./utils/handleConfiguration";
import { handleViewToggle } from "./utils/handleView";
import HandleOperationInput from "./utils/handleOperation";
import { sql } from "bun";
import { RunQuery } from "./utils/crud";

export default async function app() {
  const app = new Elysia()
    .use(html())
    .get("/", async ({ html }) => {
      const body = await Body();
      return html(
        <Page>
          <body class="m-4">
            <div>{body}</div>
          </body>
        </Page>,
      );
    })
    .post("/configurationclick/:id", async ({ params: { id } }) => {
      await handleConfigurationChange(id);
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
    .delete("/view", async (context) => {
      const body = await context.body;
      console.log(body);
      return (
        <main id="main">
          <div>{await ConfigurationsHovered()}</div>
          <div>{await Tables()}</div>
        </main>
      );
    })

    .post("/operation", async ({ body }) => {
      const { operation } = await body;
      return await HandleOperationInput(operation);
    })
    .post("/query", async ({ body }) => {
      const { query } = await body;
      await sql(query);
    })
    .put("/data", async ({ body }) => {
      const { table, set, where } = await body;
      await RunQuery(`UPDATE ${table} SET ${set} WHERE ${where}`);
      return Body();
    })
    .listen(3000);

  console.log(`Serving at ${app.server?.hostname}:${app.server?.port}`);
}

app();
