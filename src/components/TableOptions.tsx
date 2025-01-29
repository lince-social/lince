import { sql } from "bun";
import * as elements from "typed-html";

interface TableOption {
  table_name: string;
}

interface TableOptionsProps {
  tableNames: TableOption[];
}

export default async function ClosedTableOptions() {
  const tableNames: TableOptionsProps =
    await sql`SELECT table_name FROM information_schema.tables WHERE table_schema = 'public'`;
  return (
    <div id="tableoptions">
      <button
        class="flex items-center justify-between bg-crust-theme text-gray-400 hover:text-white px-4 py-2 space-x-2 rounded"
        hx-trigger="click"
        hx-get="/openedtableoptions"
        hx-target="#tableoptions"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          fill="none"
          viewBox="0 0 24 24"
          strokeWidth={1.5}
          stroke="currentColor"
          class="w-5 h-5"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="m19.5 8.25-7.5 7.5-7.5-7.5"
          />
        </svg>
        <p>Tables</p>
      </button>
    </div>
  );
}

export async function OpenedTableOptions() {
  const tableNames: TableOptionsProps =
    await sql`SELECT table_name FROM information_schema.tables WHERE table_schema = 'public'`;
  return (
    <div id="tableoptions">
      <button
        class="flex items-center justify-between bg-crust-theme text-gray-400 hover:text-white px-4 py-2 space-x-2 rounded"
        hx-trigger="click"
        hx-get="/closedtableoptions"
        hx-target="#tableoptions"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          fill="none"
          viewBox="0 0 24 24"
          strokeWidth={1.5}
          stroke="currentColor"
          class="w-5 h-5"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="m19.5 8.25-7.5 7.5-7.5-7.5"
          />
        </svg>
        <p>Tables</p>
      </button>
      <select>
        {tableNames.map((tableName) => {
          <option key={tableName}>{tableName}</option>;
        })}
      </select>
    </div>
  );
}
