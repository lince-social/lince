// import * as elements from "typed-html";
// import { getTableData } from "./Crud";
//
// export function Table({ data, table }) {
//   if (!Array.isArray(data) || data.length === 0) {
//     return <p class="text-center">No data available on table: {table}</p>;
//   }
//
//   const headers = Object.keys(data[0]).filter(
//     (key) => data[0][key] !== undefined,
//   );
//
//   return (
//     <div class="overflow-x-auto mt-2">
//       <h2 class="font-bold mb-4">{table[0].toUpperCase() + table.slice(1)}</h2>
//       <table class="table-auto border-collapse w-fit">
//         <thead>
//           <tr class="bg-black text-white font-bold">
//             {headers.map((header) => (
//               <th key={header} class="border border-white text-white p-2">
//                 {header}
//               </th>
//             ))}
//           </tr>
//         </thead>
//         <tbody>
//           {data.map((row) => (
//             <tr key={row.id} class="border border-white p-2 text-white">
//               <td>{row.id}</td>
//               {headers.slice(1).map((header) => (
//                 <td
//                   hx-get={`/inputcell/${table}/${row.id}/${header}/${row[header] !== null ? row[header].toString() : ""}`}
//                   hx-swap="outerHTML"
//                   hx-trigger="click"
//                   key={header}
//                   class="p-2 border border-white text-white"
//                 >
//                   {row[header] !== null ? row[header].toString() : ""}
//                 </td>
//               ))}
//             </tr>
//           ))}
//         </tbody>
//       </table>
//     </div>
//   );
// }
//
// export default async function Tables() {
//   const [dataArray, tableNames] = await getTableData();
//   return (
//     <div class="flex space-x-2">
//       {dataArray.map((data, index) => (
//         <Table data={data} table={tableNames[index]} />
//       ))}
//     </div>
//   );
// }
// import * as elements from "typed-html";
// import { getTableData } from "./Crud";
//
// export function Table({ data, table }) {
//   if (!Array.isArray(data) || data.length === 0) {
//     return <p class="text-center">No data available on table: {table}</p>;
//   }
//
//   const headers = Object.keys(data[0]).filter(
//     (key) => data[0][key] !== undefined,
//   );
//
//   return (
//     <div class="overflow-x-auto mt-2">
//       <h2 class="font-bold mb-4">{table[0].toUpperCase() + table.slice(1)}</h2>
//       <table class="table-auto border-collapse w-fit rounded-lg overflow-hidden">
//         <thead>
//           <tr class="bg-gray-800 text-white font-bold">
//             {headers.map((header) => (
//               <th
//                 key={header}
//                 class="border border-white text-white p-2 first:rounded-tl last:rounded-tr"
//               >
//                 {header}
//               </th>
//             ))}
//           </tr>
//         </thead>
//         <tbody>
//           {data.map((row) => (
//             <tr
//               key={row.id}
//               class="bg-gray-600 border border-white p-2 text-white"
//             >
//               <td class="first:rounded-bl last:rounded-br">{row.id}</td>
//               {headers.slice(1).map((header) => (
//                 <td
//                   hx-get={`/inputcell/${table}/${row.id}/${header}/${row[header] !== null ? row[header].toString() : ""}`}
//                   hx-swap="outerHTML"
//                   hx-trigger="click"
//                   key={header}
//                   class="p-2 border border-white text-white last:rounded-br"
//                 >
//                   {row[header] !== null ? row[header].toString() : ""}
//                 </td>
//               ))}
//             </tr>
//           ))}
//         </tbody>
//       </table>
//     </div>
//   );
// }
//
// export default async function Tables() {
//   const [dataArray, tableNames] = await getTableData();
//   return (
//     <div class="flex space-x-2">
//       {dataArray.map((data, index) => (
//         <Table data={data} table={tableNames[index]} />
//       ))}
//     </div>
//   );
// }
// import * as elements from "typed-html";
// import { getTableData } from "./Crud";
//
// export function Table({ data, table }) {
//   if (!Array.isArray(data) || data.length === 0) {
//     return <p class="text-center">No data available on table: {table}</p>;
//   }
//
//   const headers = Object.keys(data[0]).filter(
//     (key) => data[0][key] !== undefined,
//   );
//
//   return (
//     <div class="overflow-x-auto mt-2">
//       <h2 class="font-bold mb-4">{table[0].toUpperCase() + table.slice(1)}</h2>
//       <table class="table-auto w-full rounded-lg overflow-hidden">
//         <thead>
//           <tr class="bg-gray-800 text-white font-bold">
//             {headers.map((header, index) => (
//               <th
//                 key={header}
//                 class={`p-2 ${index === 0 ? "rounded-tl" : ""
//                   } ${index === headers.length - 1 ? "rounded-tr" : ""}`}
//               >
//                 {header}
//               </th>
//             ))}
//           </tr>
//         </thead>
//         <tbody>
//           {data.map((row) => (
//             <tr key={row.id} class="bg-gray-600 text-white">
//               {headers.map((header, index) => (
//                 <td
//                   key={header}
//                   class={`p-2 ${index === 0 ? "rounded-bl" : ""
//                     } ${index === headers.length - 1 ? "rounded-br" : ""}`}
//                   hx-get={`/inputcell/${table}/${row.id}/${header}/${row[header] !== null ? row[header].toString() : ""
//                     }`}
//                   hx-swap="outerHTML"
//                   hx-trigger="click"
//                 >
//                   {row[header] !== null ? row[header].toString() : ""}
//                 </td>
//               ))}
//             </tr>
//           ))}
//         </tbody>
//       </table>
//     </div>
//   );
// }
//
// export default async function Tables() {
//   const [dataArray, tableNames] = await getTableData();
//   return (
//     <div class="flex space-x-2">
//       {dataArray.map((data, index) => (
//         <Table data={data} table={tableNames[index]} />
//       ))}
//     </div>
//   );
// }
import * as elements from "typed-html";
import { getTableData } from "./Crud";

export function Table({ data, table }) {
  // Handle empty or invalid data
  if (!Array.isArray(data) || data.length === 0) {
    return <p class="text-center">No data available on table: {table}</p>;
  }

  // Derive headers from the first row of data
  const headers = Object.keys(data[0]).filter(
    (key) => data[0][key] !== undefined,
  );

  return (
    <div class="overflow-x-auto mt-2">
      <h2 class="font-bold mb-4">{table[0].toUpperCase() + table.slice(1)}</h2>
      <table class="table-auto w-full rounded-lg overflow-hidden">
        <thead>
          <tr class="bg-gray-800 text-white font-bold">
            {headers.map((header, index) => (
              <th
                key={header}
                class={`p-2 ${index === 0 ? "rounded-tl" : ""} ${
                  index === headers.length - 1 ? "rounded-tr" : ""
                }`}
              >
                {header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {data.map((row, rowIndex) => (
            <tr key={row.id || rowIndex} class="bg-gray-600 text-white">
              {headers.map((header, colIndex) => (
                <td
                  key={header}
                  class={`p-2 ${
                    rowIndex === data.length - 1 && colIndex === 0
                      ? "rounded-bl"
                      : ""
                  } ${
                    rowIndex === data.length - 1 &&
                    colIndex === headers.length - 1
                      ? "rounded-br"
                      : ""
                  }`}
                  hx-get={`/inputcell/${table}/${row.id || rowIndex}/${header}/${
                    row[header] !== null ? row[header].toString() : ""
                  }`}
                  hx-swap="outerHTML"
                  hx-trigger="click"
                >
                  {row[header] !== null ? row[header].toString() : ""}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export default async function Tables() {
  const [dataArray, tableNames] = await getTableData();
  return (
    <div class="flex space-x-2">
      {dataArray.map((data, index) => (
        <Table data={data} table={tableNames[index]} />
      ))}
    </div>
  );
}
