use maud::{Markup, html};

use crate::application::providers::view::get_active_view_data::provider_view_get_active_view_data;

pub async fn presentation_web_tables() -> Markup {
    let tables = provider_view_get_active_view_data().await.unwrap();
    html! {
        main id="main" {
        @for (table_name, table) in tables {
            @let headers: Vec<String> = table.first()
                .map(|row| row.keys().cloned().collect())
                .unwrap_or_default();

            div {

            p {(table_name)}
            table class="framed" {
                @if !headers.is_empty() {
                    thead {
                        tr {
                            @for key in &headers {
                                th { (key) }
                            }
                        }
                    }
                }
                tbody {
                    @for row in table {
                        tr {
                            @for key in &headers {
                                td hx-trigger="click" hx-swap="outerHTML" hx-get=(format!("/table/{}/{}/{}/{}", table_name, row.get("id").unwrap(), key, match row.get(key).unwrap().as_str() {
                                   "" => "None",
                                   a => a
                                } )) {
                                @if key == "id" {
                                    button
                                    hx-delete=(format!("/table/{}/{}", table_name, row.get(key).unwrap_or(&"NULL".to_string())))
                                    hx-target="#main"
                                    hx-trigger="click"
                                    { "x"}
                                }
                                    (row.get(key).unwrap_or(&"NULL".to_string())) }
                            }
                        }
                    }
            }
                }
            }
        }
    }
        }
}

// import * as elements from "typed-html";
// import { getTableData } from "./CrudTables";
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
//                 class={`p-2 ${index === 0 ? "rounded-tl" : ""} ${
//                   index === headers.length - 1 ? "rounded-tr" : ""
//                 }`}
//               >
//                 {header}
//               </th>
//             ))}
//           </tr>
//         </thead>
//         <tbody>
//           {data.map((row, rowIndex) => (
//             <tr
//               key={row.id || rowIndex}
//               class="group bg-gray-600 text-white relative"
//             >
//               {headers.map((header, colIndex) => (
//                 <td
//                   key={header}
//                   class={`p-2 ${
//                     rowIndex === data.length - 1 && colIndex === 0
//                       ? "rounded-bl"
//                       : ""
//                   } ${
//                     rowIndex === data.length - 1 &&
//                     colIndex === headers.length - 1
//                       ? "rounded-br"
//                       : ""
//                   }`}
//                   hx-get={`/inputcell/${table}/${row.id || rowIndex}/${header}/${encodeURIComponent(row[header] !== null ? row[header].toString() : "")}`}
//                   hx-swap="outerHTML"
//                   hx-trigger="click"
//                 >
//                   {row[header] !== null ? row[header].toString() : ""}
//                 </td>
//               ))}
//               {/* Absolute Positioned Delete Button */}
//               <td class="absolute top-1/2 left-0 transform -translate-y-1/2 p-2 invisible group-hover:visible">
//                 <button
//                   class="bg-red-500 text-white p-2 rounded-full w-6 h-6 flex items-center justify-center text-xl"
//                   hx-post={`/deletedata/${table}`}
//                   hx-trigger="click"
//                   hx-vals={JSON.stringify(
//                     Object.fromEntries(
//                       headers.map((header) => [
//                         header,
//                         row[header] !== null ? row[header] : "",
//                       ]),
//                     ),
//                   )}
//                   hx-target="#body"
//                 >
//                   &times;
//                 </button>
//               </td>
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
//     <div class="flex flex-wrap space-x-2">
//       {dataArray.map((data, index) => (
//         <Table data={data} table={tableNames[index]} />
//       ))}
//     </div>
//   );
// }
