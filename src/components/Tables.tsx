import * as elements from "typed-html";
import { getTableData } from "../utils/crud";

export function Table({ data, tableName }) {
  if (!Array.isArray(data) || data.length === 0) {
    return <p class="text-center">No data available on table: {tableName}</p>;
  }

  const headers = Object.keys(data[0]).filter(
    (key) => data[0][key] !== undefined,
  );

  return (
    <div class="overflow-x-auto mt-2">
      <h2 class="font-bold mb-4">
        {tableName[0].toUpperCase() + tableName.slice(1)}
      </h2>
      <table class="w-full rounded-t-lg rounded-b-lg overflow-hidden">
        <thead>
          <tr>
            {headers.map((header, index) => (
              <th
                key={index}
                class={`px-4 py-2 bg-gray-700 text-left ${
                  index === 0 ? "rounded-tl-lg" : ""
                } ${index === headers.length - 1 ? "rounded-tr-lg" : ""}`}
              >
                {header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {data.map((row, rowIndex) => (
            <tr key={rowIndex}>
              {headers.map((header, cellIndex) => (
                <td
                  key={cellIndex}
                  class={`px-4 py-2 bg-gray-500 ${
                    rowIndex === data.length - 1 && cellIndex === 0
                      ? "rounded-bl-lg"
                      : ""
                  } ${
                    rowIndex === data.length - 1 &&
                    cellIndex === headers.length - 1
                      ? "rounded-br-lg"
                      : ""
                  }`}
                >
                  {row[header] === null || row[header] === undefined
                    ? ""
                    : row[header].toString()}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

//   <table class="table-auto border-collapse w-min text-left">
//     <thead>
//       <tr class="bg-surface1-theme">
//         {headers.map((header) => (
//           <th
//             key={header}
//             class="border border-surface0-theme text-text-theme px-2 py-2 text-sm"
//           >
//             {header}
//           </th>
//         ))}
//       </tr>
//     </thead>
//     <tbody>
//       {data.map((row, rowIndex) => (
//         <tr
//           key={rowIndex}
//           class={
//             rowIndex % 2 === 0 ? "bg-subtext1-theme" : "bg-subtext0-theme"
//           }
//         >
//           <td
//             class="border border-surface0-theme px-2 py-2 text-sm text-mantle-theme hover:bg-text-theme"
//           >
//             <button
//               class="hover:text-red-700 focus:outline-none"
//             >
//               {hoveredRow === rowIndex ? "X" : row.id}
//             </button>
//           </form>
//         </td>
//
//             {
//           headers.slice(1).map((header) => (
//             <td
//               key={header}
//               class="border border-surface0-theme px-2 py-2 text-sm text-mantle-theme hover:bg-text-theme"
//             >
//               {row[header] !== null ? row[header].toString() : ""}
//             </td>
//           ))
//         }
//           </tr>
//         ))}
//   </tbody>
// </table>
//
export default async function Tables() {
  const [dataArray, tableNames] = await getTableData();
  return (
    <div class="flex space-x-2">
      {dataArray.map((data, index) => (
        <Table data={data} tableName={tableNames[index]} />
      ))}
    </div>
  );
}
