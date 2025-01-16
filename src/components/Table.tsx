import { useState } from "react";

interface TableProps {
  data: Array<Record<string, any>>;
  tableName: string;
  onDelete: (id: string) => void;
}

export default function Table({ data, tableName }: TableProps) {
  const [hoveredRow, setHoveredRow] = useState<number | null>(null);

  if (!data || data.length === 0) {
    return (
      <p className="text-center">No data available on table: {tableName}</p>
    );
  }

  const headers = Object.keys(data[0]);

  async function handleDelete(id: string, tableName: string) {
    try {
      const response = await fetch(
        `http://localhost:3000/api/data?id=${id}&tableName=${tableName}`,
        {
          method: "DELETE",
        },
      );

      if (!response.ok) {
        throw new Error("Failed to delete row");
      }
    } catch (error) {
      console.error("Error deleting row:", error);
    }
  }

  return (
    <div className="rounded overflow-x-auto">
      <p>{tableName[0].toUpperCase() + tableName.slice(1)}</p>
      <table className="table-auto border-collapse w-min text-left">
        <thead>
          <tr className="bg-surface1-theme">
            {headers.map((header) => (
              <th
                key={header}
                className="border border-surface0-theme text-text-theme px-2 py-2 text-sm"
              >
                {header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {data.map((row, rowIndex) => (
            <tr
              key={rowIndex}
              className={
                rowIndex % 2 === 0 ? "bg-subtext1-theme" : "bg-subtext0-theme"
              }
            >
              <td
                className="border border-surface0-theme px-2 py-2 text-sm text-mantle-theme hover:bg-text-theme"
                onMouseEnter={() => setHoveredRow(rowIndex)}
                onMouseLeave={() => setHoveredRow(null)}
              >
                <button
                  onClick={() => handleDelete(row.id, tableName)}
                  className="hover:text-red-700 focus:outline-none"
                >
                  {hoveredRow === rowIndex ? "X" : row.id}
                </button>
              </td>

              {headers.slice(1).map((header) => (
                <td
                  key={header}
                  className="border border-surface0-theme px-2 py-2 text-sm text-mantle-theme hover:bg-text-theme"
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
