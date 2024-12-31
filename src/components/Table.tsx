interface TableProps {
  data: Array<Record<string, any>>;
}

export default function Table({ data }: TableProps) {
  // Handle empty data gracefully
  if (!data || data.length === 0) {
    return <p className="text-center">No data available</p>;
  }

  // Extract column headers dynamically from keys of the first object
  const headers = Object.keys(data[0]);

  return (
    <div className="overflow-x-auto my-4 rounded">
      <table className="min-w-full table-auto shadow-lg">
        <thead className="bg-green-700 text-white">
          <tr>
            {headers.map((header) => (
              <th
                key={header}
                className="px-4 py-2 text-left border-b-2 border-gray-300"
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
              className={`${rowIndex % 2 === 0 ? "bg-gray-100" : "bg-white"
                } text-black`}
            >
              {headers.map((header) => (
                <td key={header} className="px-4 py-2 border-b border-gray-300">
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
