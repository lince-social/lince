interface TableProps {
  data: Array<Record<string, any>>;
}

export default function Table({ data }: TableProps) {
  if (!data || data.length === 0) {
    return <p className="text-center">No data available</p>;
  }

  const headers = Object.keys(data[0]);

  return (
    <div className="rounded overflow-x-auto ">
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
              {headers.map((header) => (
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
