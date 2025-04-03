// import * as elements from "typed-html";
// import Body from "../sections/Body";
// import { sql } from "bun";
// import { Table } from "../tables/Tables";
//
// export async function QueryInputComponent() {
//   return (
//     <form class="w-full">
//       <input
//         class="rounded text-white font-bold bg-black border border-white shadow-md shadow-white/50 w-full h-12"
//         placeholder="Query to run ..."
//         name="query"
//         hx-post="/query"
//         hx-target="#body"
//         autofocus
//       />
//     </form>
//   );
// }
//
// export async function RunQuery(query: string) {
//   await sql(query);
//   return Body();
// }
//
// export async function CreateDataComponent(table: string) {
//   const result =
//     await sql`SELECT column_name FROM information_schema.columns WHERE table_name = ${table};`;
//   const columns = result
//     .filter((col) => col.column_name)
//     .map((col) => col.column_name);
//   const tableNames =
//     await sql`SELECT table_name FROM information_schema.tables WHERE table_schema = 'public'`;
//
//   return (
//     <form
//       class="border border-white rounded font-bold flex flex-col p-2"
//       hx-post="/data"
//       hx-trigger="keydown[event.key === 'Enter']"
//       hx-target="#body"
//       hx-vals={`js:{table: "${table}"}`}
//     >
//       <h2>Create item in table: {`${table}`}</h2>
//       {columns.map((col) => (
//         <div key={col} class="flex flex-col">
//           <label class="font-bold" for={col}>
//             {col}
//           </label>
//           <input
//             type="text"
//             id={col}
//             name={col}
//             class="text-white bg-[#1e1e2e] rounded border border-white"
//           />
//         </div>
//       ))}
//     </form>
//   );
// }
//
// export async function CreateData(body) {
//   const { table, ...fields } = await body;
//
//   const fieldNames = [];
//   const fieldValues = [];
//
//   for (const [key, value] of Object.entries(fields)) {
//     if (value !== "") {
//       fieldNames.push(key);
//       fieldValues.push(typeof value === "string" ? `'${value}'` : value);
//     }
//   }
//
//   const query = `INSERT INTO ${table} (${fieldNames}) VALUES (${fieldValues})`;
//   await sql(query);
//   return Body();
// }
//
// export async function ReadDataComponent(table: string) {
//   return <Table data={await sql(`SELECT * FROM ${table}`)} table={table} />;
// }
//
// export async function UpdateDataComponent(table: string) {
//   return (
//     <form
//       class="flex flex-col border border-white m-4 p-4 rounded space-y-1"
//       hx-put="/dataform"
//       hx-trigger="keyup[event.key === 'Enter']"
//       hx-target="#body"
//     >
//       <label for="table">UPDATE</label>
//       <input
//         id="table"
//         value={`${table}`}
//         name="table"
//         class="rounded bg-black text-white border border-white hover:shadow-white/50 hover:shadow-md"
//       />
//       <label for="set">SET</label>
//       <input
//         id="set"
//         name="set"
//         class="rounded bg-black text-white border border-white hover:shadow-white/50 hover:shadow-md"
//       />
//       <label for="where">WHERE</label>
//       <input
//         id="where"
//         name="where"
//         class="rounded bg-black text-white border border-white hover:shadow-white/50 hover:shadow-md"
//       />
//     </form>
//   );
// }
//
// export async function UpdateData(table, set, where) {
//   await RunQuery(`UPDATE ${table} SET ${set} WHERE ${where}`);
//   return await Body();
// }
//
// export async function DeleteDataComponent(table: string) {
//   return (
//     <form
//       class="flex flex-col border border-white m-4 p-4 rounded space-y-1"
//       hx-delete="/data"
//       hx-trigger="keyup[event.key === 'Enter']"
//       hx-target="#body"
//     >
//       <label for="table">DELETE FROM</label>
//       <input
//         id="table"
//         value={`${table}`}
//         name="table"
//         class="rounded bg-black text-white border border-white hover:shadow-white/50 hover:shadow-md"
//       />
//       <label for="where">WHERE</label>
//       <input
//         id="where"
//         name="where"
//         class="rounded bg-black text-white border border-white hover:shadow-white/50 hover:shadow-md"
//       />
//     </form>
//   );
// }
//
// export async function RunSqlFileComponent() {
//   return null;
// }
//
// export async function PrintHelpComponent() {
//   try {
//     const filePath = join(import.meta.dir, "../../README.md");
//     const docs = Bun.file(filePath);
//     const content = await docs.text();
//
//     return (
//       <div class="prose max-w-none p-4 bg-gray-800 rounded-lg shadow-md">
//         <pre class="whitespace-pre-wrap">{content}</pre>
//       </div>
//     );
//   } catch (error) {
//     console.log(error);
//   }
// }
//
// export async function ZeroRecordQuantity(operation: string) {
//   try {
//     await sql`UPDATE record SET quantity = 0 WHERE id = ${operation}`
//     return true
//   } catch (error) {
//     console.log(`Error when updating quantity of record with id = ${operation}, error: ${error}`)
//   }
// }
// import * as elements from "typed-html";
// import {
//   CreateDataComponent,
//   DeleteDataComponent,
//   PrintHelpComponent,
//   QueryInputComponent,
//   ReadDataComponent,
//   RunSqlFileComponent,
//   UpdateDataComponent,
//   ZeroRecordQuantity,
// } from "./CrudOperation";
// import { saveDatabase } from "../../../db/startup";
// import { ConfigurationChange } from "../configurations/CrudConfigurations";
// import Body, { FatherBody } from "../sections/Body";
// import { isNumericString } from "elysia/utils";
//
// export async function HandleOperation(operation: string) {
//
//
//   switch (true) {
//     case operation === "configuration":
//       return await ReadDataComponent("configuration");
//     case operation === "configuration_view":
//       return await ReadDataComponent("configuration_view");
//     case operation === "history":
//       return await ReadDataComponent("history");
//     case operation === "record":
//       return await ReadDataComponent("record");
//     case operation === "karma":
//       return await ReadDataComponent("karma");
//     case operation === "frequency":
//       return await ReadDataComponent("frequency");
//     case operation === "command":
//       return await ReadDataComponent("command");
//     case operation === "sum":
//       return await ReadDataComponent("sum");
//     case operation === "transfer":
//       return await ReadDataComponent("transfer");
//     case operation === "view":
//       return await ReadDataComponent("view");
//     case operation === "karma_consequence":
//       return await ReadDataComponent("karma_consequence");
//     case operation === "karma_condition":
//       return await ReadDataComponent("karma_condition");
//   }
//
//   switch (true) {
//     case /c/.test(operation):
//       return await CreateDataComponent(table);
//     case /r/.test(operation):
//       return await ReadDataComponent(table);
//     case /u/.test(operation):
//       return await UpdateDataComponent(table);
//     case /d/.test(operation):
//       return await DeleteDataComponent(table);
//     case /s/.test(operation):
//       saveDatabase();
//       return;
//     case /a/.test(operation):
//       const id = operation.match(/\d+/)?.[0];
//       if (id) return ConfigurationChange(id);
//       return;
//     case /q/.test(operation):
//       return await QueryInputComponent();
//     case /f/.test(operation):
//       return RunSqlFileComponent();
//     case /h/.test(operation):
//       return PrintHelpComponent();
//     default:
//       return ReadDataComponent(table);
//   }
// }
//
// export default async function OperationComponent(body: any) {
//   const { operation } = await body;
//
//   if (isNumericString(operation)) {
//     await ZeroRecordQuantity(operation)
//     return Body()
//   }
