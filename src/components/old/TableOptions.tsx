interface TableOption {
  table_name: string;
}

interface TableOptionsProps {
  tableNames: TableOption[];
}

export default function TableOptions({ tableNames }: TableOptionsProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [searchTerm, setSearchTerm] = useState("");
  const router = useRouter();

  const handleSelect = (tableName: string) => {
    setIsOpen(false);
    if (tableName !== "Tables") {
      router.push(`/table/${tableName}`);
    }
  };

  const filteredTableNames = tableNames.filter((option) =>
    option.table_name.toLowerCase().includes(searchTerm.toLowerCase()),
  );

  return (
    <div className="relative">
      <button
        className="w-full flex items-center justify-between bg-crust-theme text-gray-400 hover:text-white px-4 py-2 rounded"
        onClick={() => setIsOpen(!isOpen)}
      >
        Tables
        <svg
          xmlns="http://www.w3.org/2000/svg"
          fill="none"
          viewBox="0 0 24 24"
          strokeWidth={1.5}
          stroke="currentColor"
          className={`w-5 h-5 transition-transform ${isOpen ? "rotate-180" : "rotate-0"
            }`}
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="m19.5 8.25-7.5 7.5-7.5-7.5"
          />
        </svg>
      </button>
      {isOpen && (
        <div
          className="absolute z-50 left-0 mt-2 bg-crust-theme rounded shadow-lg min-w-[max-content]"
          onClick={(e) => e.stopPropagation()}
        >
          <div className="p-2">
            <input
              type="text"
              placeholder="Search tables..."
              className="w-full px-4 py-2 text-gray-400 bg-gray-800 rounded focus:outline-none focus:ring-2 focus:ring-blue-500"
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
            />
          </div>
          <ul className="max-h-48 overflow-y-auto">
            {filteredTableNames.length > 0 ? (
              filteredTableNames.map((option) => (
                <li
                  key={option.table_name}
                  className="px-4 py-2 text-gray-400 hover:text-white hover:bg-gray-700 cursor-pointer"
                  onClick={() => handleSelect(option.table_name)}
                >
                  {option.table_name}
                </li>
              ))
            ) : (
              <li className="px-4 py-2 text-gray-500">No results found</li>
            )}
          </ul>
        </div>
      )}
    </div>
  );
}
