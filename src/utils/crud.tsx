import { join } from "path";
import * as elements from "typed-html";
import { file, sql } from "bun";
import { Table } from "../components/Tables";

export async function QueryInput() {
  return (
    <form class="w-full">
      <input
        class="rounded text-white font-bold bg-black border border-white shadow-md shadow-white/50 w-full h-12"
        placeholder="Query to run ..."
        name="query"
        hx-post="/query"
        autofocus
      />
    </form>
  );
}

export async function RunQuery(query: string) {
  await sql(query);
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
        tableName = words[i + 1]; // Get the word after "FROM"
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

export async function RunSqlFile() { }

export async function PrintHelp() {
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

export async function CreateData(table: string) {
  return (
    <form>
      <input placeholder="asxas" />
    </form>
  );
}

export async function ReadData(table: string) {
  return <Table data={await sql(`SELECT * FROM ${table}`)} tableName={table} />;
}

export async function UpdateData(table: string) {
  return (
    <form
      class="flex flex-col border border-white m-4 p-4 rounded space-y-1"
      hx-put="/data"
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

export async function DeleteData(table: string, where: string = undefined) { }

export async function getActiveConfiguration() {
  return await sql`SELECT id, configurationName, quantity, views FROM configuration WHERE quantity = 1`;
}

export async function getInactiveConfigurations() {
  return await sql`SELECT id, configurationName, quantity, views  FROM configuration WHERE quantity <> 1`;
}

export async function getViews() {
  return await sql`SELECT viewName, query FROM view`;
}
