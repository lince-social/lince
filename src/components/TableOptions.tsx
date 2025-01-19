"use client";

import { useRouter } from "next/navigation";

interface TableOption {
  table_name: string;
}

interface TableOptionsProps {
  tableNames: TableOption[];
}

export default function TableOptions({ tableNames }: TableOptionsProps) {
  const router = useRouter();

  const handleChange = (event: React.ChangeEvent<HTMLSelectElement>) => {
    const selectedTable = event.target.value;
    if (selectedTable !== "Tables") {
      router.push(`/table/${selectedTable}`); // Redirect to the page for the selected table
    }
  };

  return (
    <div>
      <select
        className="text-gray-400 hover:text-white bg-mantle-theme rounded"
        onChange={handleChange}
        defaultValue="Tables"
      >
        <option value="Tables" disabled>
          Tables
        </option>
        {tableNames.map((option) => (
          <option key={option.table_name} value={option.table_name}>
            {option.table_name}
          </option>
        ))}
      </select>
    </div>
  );
}
