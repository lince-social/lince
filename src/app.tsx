import * as elements from "typed-html";
import { Elysia } from "elysia";
import { html } from "@elysiajs/html";

import Page from "./components/sections/Page";
import Body from "./components/sections/Body";
import ConfigurationsUnhovered, {
  ConfigurationsHovered,
} from "./components/Configurations";
import Tables from "./components/Tables";
import {
  ConfigurationChange,
  CreateData,
  DeleteView,
  handleViewToggle,
  RunQuery,
  UpdateData,
} from "./components//Crud";
import OperationComponent from "./components/Operation";
import ClosedTableOptions, {
  OpenedTableOptions,
} from "./components/TableOptions";

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
      await ConfigurationChange(id);
    })
    .get("/configurationhovered", async () => {
      return await ConfigurationsHovered();
    })
    .get("/configurationunhovered", async () => {
      return await ConfigurationsUnhovered();
    })
    .post("/view", async (context) => {
      const body = await context.body;
      const { views, view, configurationId } = body;
      await handleViewToggle(views, view, configurationId);
      return (
        <main id="main">
          <div>{await ConfigurationsHovered()}</div>
          <div>{await Tables()}</div>
        </main>
      );
    })
    .delete("/view", async ({ query }) => {
      const { viewName, configurationId } = query;
      return await DeleteView(viewName, configurationId);
    })

    .post("/operation", async ({ body }) => {
      const { operation } = await body;
      return await OperationComponent(operation);
    })
    .post("/query", async ({ body }) => {
      const { query } = await body;
      return await RunQuery(query);
    })
    .post("/data", async ({ body }) => {
      return await CreateData(body);
    })
    .put("/data", async ({ body }) => {
      const { table, set, where } = await body;
      return await UpdateData(table, set, where);
    })
    .delete("/data", async ({ query }) => {
      const { table, where } = query;
      return await RunQuery(`DELETE FROM ${table} WHERE ${where}`);
    })
    .get("/openedtableoptions", await OpenedTableOptions())
    .get("/closedtableoptions", await ClosedTableOptions())
    .listen(3000);

  console.log(`Serving at ${app.server?.hostname}:${app.server?.port}`);
}

app();
