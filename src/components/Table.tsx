interface TableProps {
  data: Array<Record<string, any>>;
}

export default function Table({ data }: TableProps) {
  if (!data || data.length === 0) {
    return <p className="text-center">No data available</p>;
  }

  const headers = Object.keys(data[0]);

  return (
    <div className="rounded overflow-x-auto border-transparent border-2 hover:border-rosewater">
      <table className="table-auto border-collapse  w-full text-left">
        <thead>
          <tr className="bg-gray-200">
            {headers.map((header) => (
              <th
                key={header}
                className="border border-gray-300 px-4 py-2 text-sm font-medium text-gray-700"
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
              className={rowIndex % 2 === 0 ? "bg-gray-50" : "bg-white"}
            >
              {headers.map((header) => (
                <td
                  key={header}
                  className="border border-gray-300 px-4 py-2 text-sm text-gray-700"
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
