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
                class={`p-2 ${index === 0 ? "rounded-tl" : ""} ${index === headers.length - 1 ? "rounded-tr" : ""
                  }`}
              >
                {header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {data.map((row, rowIndex) => (
            <tr key={row.id || rowIndex} class="group bg-gray-600 text-white">
              {headers.map((header, colIndex) => (
                <td
                  key={header}
                  class={`p-2 ${rowIndex === data.length - 1 && colIndex === 0
                      ? "rounded-bl"
                      : ""
                    } ${rowIndex === data.length - 1 &&
                      colIndex === headers.length - 1
                      ? "rounded-br"
                      : ""
                    }`}
                  hx-get={`/inputcell/${table}/${row.id || rowIndex}/${header}/${row[header] !== null ? row[header].toString() : ""
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
    <div class="flex flex-wrap space-x-2">
      {dataArray.map((data, index) => (
        <Table data={data} table={tableNames[index]} />
      ))}
    </div>
  );
}
