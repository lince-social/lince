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
  EditableRow,
  RunQuery,
  ToggleView,
  UpdateData,
} from "./components//Crud";
import OperationComponent from "./components/Operation";

export default async function app() {
  const app = new Elysia()
    .use(html())
    .get("/", async ({ html }) => {
      const body = await Body();
      return await html(
        <Page>
          <body class="m-4">
            <div>{body}</div>
          </body>
        </Page>,
      );
    })
    .post("/configurationclick/:id", async ({ params: { id } }) => {
      return await ConfigurationChange(id);
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
      return await ToggleView(views, view, configurationId);
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
    .put("/dataform", async ({ body }) => {
      const { table, set, where } = await body;
      return await UpdateData(table, set, where);
    })
    // .put(
    //   "/datanotform/:table/:set/:where",
    //   async ({ params: { table, set, where } }) => {
    //     console.log(table, set, where);
    //     return await UpdateData(table, set, where);
    //   },
    // )
    .put(
      "/datanotform/:table/:key/:id",
      async ({ params: { table, key, id }, body }) => {
        const { value } = body; // Extract value from request body
        console.log(table, key, id, value);
        return await UpdateData(table, `${key}='${value}'`, `id=${id}`);
      },
    )

    .delete("/data", async ({ query }) => {
      const { table, where } = query;
      return await RunQuery(`DELETE FROM ${table} WHERE ${where}`);
    })
    .get("/editablerow/:table/:id", async ({ params: { table, id } }) => {
      return await EditableRow(table, id);
    })
    .get(
      "/inputcell/:table/:id/:key/:value",
      async ({ params: { table, id, key, value } }) => {
        return (
          <td>
            <form
              hx-put={`/datanotform/${table}/${key}/${id}`}
              hx-target="#body"
            >
              <input
                name="value"
                value={decodeURIComponent(value)}
                class="text-white bg-black"
              />
            </form>
          </td>
        );
      },
    )
    .listen(3000);

  console.log(`Serving at ${app.server?.hostname}:${app.server?.port}`);
}

app();
