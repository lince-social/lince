import { join } from "path";
import * as elements from "typed-html";
import { sql } from "bun";
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
  const result = await sql`
    SELECT v.query 
    FROM configuration_view cv
    JOIN view v ON cv.view_id = v.id
    WHERE cv.is_active = true
      AND cv.configuration_id = (SELECT id FROM configuration WHERE quantity = 1);
  `;

  const mappedQueries = result.map((row) => row.query);

  const tableNames = mappedQueries.map((query) => {
    const words = query.split(" ");
    return words[words.indexOf("FROM") + 1] || null;
  });

  const data = await Promise.all(mappedQueries.map((query) => sql(query)));

  return [data, tableNames];
}

export async function RunSqlFileComponent() {
  return null;
}

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

// export async function DeleteRow(table, body) {
//   console.log(table);
//   console.log(body);
//   const { ...fields } = await body;
//
//   const fieldNames = [];
//   const fieldValues = [];
//
//   for (const [key, value] of Object.entries(fields)) {
//     if (value !== "") {
//       fieldNames.push(key);
//       fieldValues.push(typeof value === "string" ? `'${value}'` : value);
//       // console.log(key);
//       // console.log(value);
//     }
//   }
//
//   const query = `DELETE FROM ${table} (${fieldNames}) VALUES (${fieldValues})`;
//   console.log(query);
//   // await sql(query);
//   return Body();
// }
export async function DeleteRow(table, body) {
  const { ...fields } = await body;

  const conditions = [];

  for (const [key, value] of Object.entries(fields)) {
    if (value !== "") {
      // For strings, wrap the value in quotes, for other types, leave them as is
      const condition = `${key} = ${typeof value === "string" ? `'${value}'` : value}`;
      conditions.push(condition);
    }
  }

  const whereClause = conditions.join(" AND ");

  const query = `DELETE FROM ${table} WHERE ${whereClause}`;
  console.log(query);
  await sql(query);
  return await Body();
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
            class="text-white bg-[#1e1e2e] rounded border border-white"
          />
        </div>
      ))}
    </form>
  );
}

// export async function CreateData(data) {
//   const fields = [];
//   const values = [];
//
//   for (const [key, value] of Object.entries(data)) {
//     if (value !== "") {
//       fields.push(key);
//       values.push(value);
//     }
//   }
//
//   const query = `INSERT INTO ${data.table} (${sql(fields)}) VALUES (${sql(values)})`;
//   await sql(query);
//   return Body();
// }
export async function CreateData(body) {
  const { table, ...fields } = await body;

  const fieldNames = [];
  const fieldValues = [];

  for (const [key, value] of Object.entries(fields)) {
    if (value !== "") {
      fieldNames.push(key);
      fieldValues.push(typeof value === "string" ? `'${value}'` : value);
    }
  }

  const query = `INSERT INTO ${table} (${fieldNames}) VALUES (${fieldValues})`;
  await sql(query);
  return Body();
}

export async function ReadDataComponent(table: string) {
  return <Table data={await sql(`SELECT * FROM ${table}`)} table={table} />;
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
  return await sql`SELECT id, configurationName, quantity FROM configuration WHERE quantity = 1`;
}

export async function getInactiveConfigurations() {
  return await sql`SELECT id, configurationName, quantity FROM configuration WHERE quantity <> 1`;
}

export async function CreateView(configurationId, body) {
  try {
    const { viewname, query } = await body;
    console.log(configurationId, viewname, query);

    // Check if the view already exists
    const existingView = await sql`
      SELECT id FROM view 
      WHERE view_name = ${viewname} AND query = ${query}
      LIMIT 1;
    `;

    let viewId;

    if (existingView.length > 0) {
      viewId = existingView[0].id;
    } else {
      // Insert the new view and get its ID
      const insertedView = await sql`
        INSERT INTO view (view_name, query) 
        VALUES (${viewname}, ${query}) 
        RETURNING id;
      `;
      viewId = insertedView[0].id;
    }

    // Insert into configuration_view if it doesn't already exist
    await sql`
      INSERT INTO configuration_view (configuration_id, view_id, is_active) 
      VALUES (${configurationId}, ${viewId}, true)
      ON CONFLICT (configuration_id, view_id) DO NOTHING;
    `;
    return await Body();
  } catch (error) {
    const { viewname, query } = await body;
    console.log(
      `Error: ${error}, when creating new view in configuration with id: ${configurationId}. View received: ${viewname}, Query received: ${query}`,
    );
    return { success: false, error: error };
  }
}

export async function getViews() {
  return await sql`SELECT viewName, query FROM view`;
}

export async function ConfigurationChange(id: string) {
  try {
    await sql`
      UPDATE configuration
      SET quantity = CASE WHEN id = ${id} THEN 1 ELSE 0 END;
    `;

    return (
      <main id="main">
        <div>{await ConfigurationsHovered()}</div>
        <div>{await Tables()}</div>
      </main>
    );
  } catch (error) {
    console.log("Error updating configuration:", error);
  }
}

export async function ToggleView(body) {
  const { configurationId, viewId, isActive } = body;
  const isActiveBool = isActive === "true";

  await sql`
    UPDATE configuration_view
    SET is_active = ${!isActiveBool}
    WHERE configuration_id = ${configurationId} AND view_id = ${viewId};
  `;

  return (
    <main id="main">
      <div>{await ConfigurationsHovered()}</div>
      <div>{await Tables()}</div>
    </main>
  );
}

export async function DeleteView(query) {
  const { viewId, configurationId } = query;

  await sql`
    DELETE FROM configuration_view
    WHERE configuration_id = ${configurationId} AND view_id = ${viewId};
  `;

  return (
    <main id="main">
      <div>{await ConfigurationsHovered()}</div>
      <div>{await Tables()}</div>
    </main>
  );
}

export async function CreateViewComponent(configurationId, view_name, query) {
  return <p>osidnodicn</p>;
}

export async function InitialAddView(configurationId, viewname, query) {
  return (
    <div>
      {await AddViewInput(configurationId, viewname, query)}
      {await MatchedViewProperties(configurationId, viewname, query)}
    </div>
  );
}
export async function AddViewInput(configurationId, viewname, query) {
  return (
    <div>
      <form
        id="addviewcomponent"
        hx-trigger={`keydown[key === "Enter"]`}
        hx-post={`/view/${configurationId}`}
        hx-target="#body"
        class="flex relative space-x-2 p-1 rounded border border-white"
      >
        <input
          name="viewname"
          placeholder="Add view"
          class="rounded text-black bg-white"
          value={viewname}
          autofocus
        />
        <input
          name="query"
          placeholder="Query..."
          class="rounded text-black bg-white"
          value={query}
        />
      </form>
    </div>
  );
}

export async function MatchedViewProperties(configurationId, viewname, query) {
  const views = await sql`SELECT view_name, query FROM view`;

  function containsAllChars(str: string, chars: string): boolean {
    if (typeof chars !== "string") return false;
    return chars
      .split("")
      .every((char) => str.toLowerCase().includes(char.toLowerCase()));
  }

  const queriedNames = views
    .filter((item) => containsAllChars(item.view_name, viewname))
    .map((item) => item.view_name);

  const queriedQueries = views
    .filter((item) => containsAllChars(item.query, query))
    .map((item) => item.query);

  return (
    <div
      id="matchedviewproperties"
      class="z-50 relative flex flex-wrap justify-between w-full mt-2 space-x-2"
    >
      <ul class="absolute left-0 top-full mt-2 bg-black text-white p-2 rounded border border-white min-w-[150px] max-w-[45%]">
        {queriedNames.length === 0 ? (
          <li class="trucate px-2 py-1">{viewname}</li>
        ) : (
          queriedNames.map((item) => <li class="truncate px-2 py-1">{item}</li>)
        )}
      </ul>
      <ul class="absolute right-0 top-full mt-2 bg-black text-white p-2 rounded border border-white min-w-[150px] max-w-[45%]">
        {queriedQueries.length === 0 ? (
          <li
            hx-triger="click"
            hx-post={`/matchedviewqueryclick/${configurationId}/${query}}`}
            class="truncate px-2 py-1"
          >
            {query}
          </li>
        ) : (
          queriedQueries.map((item) => (
            <li class="truncate px-2 py-1">{item}</li>
          ))
        )}
      </ul>
    </div>
  );
}
