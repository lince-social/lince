import * as elements from "typed-html";
import { getTableData } from "./Crud";

export function Table({ data, table }) {
  if (!Array.isArray(data) || data.length === 0) {
    return <p class="text-center">No data available on table: {table}</p>;
  }

  const headers = Object.keys(data[0]).filter(
    (key) => data[0][key] !== undefined,
  );

  return (
    <div class="overflow-x-auto mt-2">
      <h2 class="font-bold mb-4">{table[0].toUpperCase() + table.slice(1)}</h2>
      <table class="table-auto border-collapse w-fit">
        <thead>
          <tr class="bg-black text-white font-bold">
            {headers.map((header) => (
              <th key={header} class="border border-white text-white p-2">
                {header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {data.map((row) => (
            <tr key={row.id} class="border border-white p-2 text-white">
              <td>{row.id}</td>
              {headers.slice(1).map((header) => (
                <td
                  hx-get={`/inputcell/${table}/${row.id}/${header}/${row[header] !== null ? row[header].toString() : ""}`}
                  hx-swap="outerHTML"
                  hx-trigger="click"
                  key={header}
                  class="p-2 border border-white text-white"
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
