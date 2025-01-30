import { join } from "path";
import * as elements from "typed-html";
import { file, sql } from "bun";
import Tables, { Table } from "./Tables";
import { ConfigurationsHovered } from "./Configurations";
import Body from "./sections/Body";

export async function QueryInputComponent() {
  return (
    <form class="w-full">
      <input
        class="rounded text-white font-bold bg-black border border-white shadow-md shadow-white/50 w-full h-12"
        placeholder="Query to run ..."
        name="query"
        hx-post="/query"
        hx-target="#body"
        autofocus
      />
    </form>
  );
}

export async function RunQuery(query: string) {
  await sql(query);
  return Body();
}

export async function getTableData() {
  const result = await sql`SELECT views FROM configuration WHERE quantity = 1`;
  const views = result[0].views;

  const activeViews = Object.keys(views).filter((viewName) => views[viewName]);

  const queriedQueries = await Promise.all(
    activeViews.map(async (activeView) => {
      return await sql`SELECT query FROM view WHERE view_name = ${activeView}`;
    }),
  );

  const mappedQueries = queriedQueries.map((query) => query[0].query);

  const tableNames = mappedQueries.map((query) => {
    const words = query.split(" ");
    let tableName = null;
    for (let i = 0; i < words.length; i++) {
      if (words[i].toUpperCase() === "FROM" && i + 1 < words.length) {
        tableName = words[i + 1];
        break;
      }
    }
    return tableName;
  });

  const data = await Promise.all(
    mappedQueries.map(async (query) => {
      const queriedData = await sql(query);
      return queriedData;
    }),
  );
  return [data, tableNames];
}

export async function RunSqlFileComponent() { }

export async function PrintHelpComponent() {
  try {
    const filePath = join(import.meta.dir, "../../README.md");
    const docs = Bun.file(filePath);
    const content = await docs.text();

    return (
      <div class="prose max-w-none p-4 bg-gray-800 rounded-lg shadow-md">
        <pre class="whitespace-pre-wrap">{content}</pre>
      </div>
    );
  } catch (error) {
    console.log(error);
  }
}

export async function CreateDataComponent(table: string) {
  const result =
    await sql`SELECT column_name FROM information_schema.columns WHERE table_name = ${table};`;
  const columns = result
    .filter((col) => col.column_name)
    .map((col) => col.column_name);
  const tableNames =
    await sql`SELECT table_name FROM information_schema.tables WHERE table_schema = 'public'`;

  return (
    <form
      class="border border-white rounded font-bold flex flex-col p-2"
      hx-post="/data"
      hx-trigger="keydown[event.key === 'Enter']"
      hx-target="#body"
      hx-vals={`js:{table: "${table}"}`}
    >
      <h2>Create item in table: {`${table}`}</h2>
      {columns.map((col) => (
        <div key={col} class="flex flex-col">
          <label class="font-bold" for={col}>
            {col}
          </label>
          <input
            type="text"
            id={col}
            name={col}
            class="bg-black text-white rounded border border-white"
          />
        </div>
      ))}
    </form>
  );
}

export async function CreateData(data) {
  console.log(data);
  return await Body();
}

export async function ReadDataComponent(table: string) {
  return <Table data={await sql(`SELECT * FROM ${table}`)} tableName={table} />;
}

export async function UpdateDataComponent(table: string) {
  return (
    <form
      class="flex flex-col border border-white m-4 p-4 rounded space-y-1"
      hx-put="/dataform"
      hx-trigger="keyup[event.key === 'Enter']"
      hx-target="#body"
    >
      <label for="table">UPDATE</label>
      <input
        id="table"
        value={`${table}`}
        name="table"
        class="rounded bg-black text-white border border-white hover:shadow-white/50 hover:shadow-md"
      />
      <label for="set">SET</label>
      <input
        id="set"
        name="set"
        class="rounded bg-black text-white border border-white hover:shadow-white/50 hover:shadow-md"
      />
      <label for="where">WHERE</label>
      <input
        id="where"
        name="where"
        class="rounded bg-black text-white border border-white hover:shadow-white/50 hover:shadow-md"
      />
    </form>
  );
}

export async function UpdateData(table, set, where) {
  await RunQuery(`UPDATE ${table} SET ${set} WHERE ${where}`);
  return await Body();
}

export async function DeleteDataComponent(table: string) {
  return (
    <form
      class="flex flex-col border border-white m-4 p-4 rounded space-y-1"
      hx-delete="/data"
      hx-trigger="keyup[event.key === 'Enter']"
      hx-target="#body"
    >
      <label for="table">DELETE FROM</label>
      <input
        id="table"
        value={`${table}`}
        name="table"
        class="rounded bg-black text-white border border-white hover:shadow-white/50 hover:shadow-md"
      />
      <label for="where">WHERE</label>
      <input
        id="where"
        name="where"
        class="rounded bg-black text-white border border-white hover:shadow-white/50 hover:shadow-md"
      />
    </form>
  );
}

export async function getActiveConfiguration() {
  return await sql`SELECT id, configurationName, quantity, views FROM configuration WHERE quantity = 1`;
}

export async function getInactiveConfigurations() {
  return await sql`SELECT id, configurationName, quantity, views  FROM configuration WHERE quantity <> 1`;
}

export async function getViews() {
  return await sql`SELECT viewName, query FROM view`;
}

export async function ConfigurationChange(id: string) {
  try {
    await sql`
      UPDATE configuration
      SET quantity = CASE
        WHEN id = ${id} THEN 1
        ELSE 0
      END
      WHERE EXISTS (
        SELECT 1 FROM configuration WHERE id = ${id}
      )
    `;
    return (
      <main id="main">
        <div>{await ConfigurationsHovered()} </div>
        <div> {await Tables()} </div>
      </main>
    );
  } catch (error) {
    console.log("Error updating quantities:", error);
  }
}

export async function ToggleView(views, view, configurationId) {
  const jsonviews = JSON.parse(views);
  const jsonview = JSON.parse(view);

  Object.keys(jsonview).forEach((viewName) => {
    jsonviews[viewName] = !jsonview[viewName];
  });

  await sql`UPDATE configuration SET views = ${jsonviews} WHERE id = ${configurationId};`;

  return (
    <main id="main">
      <div>{await ConfigurationsHovered()}</div>
      <div>{await Tables()}</div>
    </main>
  );
}

export async function DeleteView(viewName, configurationId) {
  const query = `UPDATE configuration SET views = views - '${viewName}' WHERE id = ${configurationId};`;
  await sql(query);

  return (
    <main id="main">
      <div>{await ConfigurationsHovered()}</div>
      <div>{await Tables()}</div>
    </main>
  );
}

export async function CreateViewComponent() {
  return <p>osidnodicn</p>;
}

export async function EditableRow(table, id) {
  const row = await sql`SELECT * FROM ${table} WHERE id = ${id}`;
  return (
    <form
      hx-put={`/data`}
      hx-target="#body"
      hx-trigger="keydown[key === 'Enter'] from:body"
    ></form>
  );
}

export async function CreateView(view, configurationId) {
  try {
    console.log(views, configurationId);

    const queriedViews = await prisma.view.findMany();

    if (queriedViews) {
      console.log(queriedViews);
    }
  } catch (error) {
    console.log(
      `Error: ${error}, when creating new view in configuration with id: ${configurationId}. Views received: ${views}`,
    );
  }
}
