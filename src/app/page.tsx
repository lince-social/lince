'use client';

import { useEffect, useState } from 'react';

export default function Home() {
  const [views, setViews] = useState<{ id: number; view_name: string }[]>([]);
  const [data, setData] = useState<{ [viewId: number]: any[] }>({});
  const [loading, setLoading] = useState<{ [viewId: number]: boolean }>({});
  const [status, setStatus] = useState<string | null>(null);

  useEffect(() => {
    const performStartupTasks = async () => {
      try {
        const res = await fetch("/api/startup");
        const data = await res.json();
        console.log(data);
        if (res.ok) {
          setStatus(data.status);
        } else {
          throw new Error(data.error || "Unknown error occured");
        }
      } catch (error) {
        console.log("Startup failed:", error);
        setStatus("Startup failed. Check logs.");
      }
    };

    performStartupTasks();
  }, []);

  useEffect(() => {
    const fetchViews = async () => {
      try {
        const response = await fetch('/api/views');
        const result = await response.json();
        if (Array.isArray(result)) {
          setViews(result);
        } else {
          console.log('Invalid data format for views:', result);
        }
      } catch (error) {
        console.log('Error fetching views:', error);
      }
    };
    fetchViews();
  }, []);

  const fetchData = async (viewId: number) => {
    setLoading((prevLoading) => ({
      ...prevLoading,
      [viewId]: true,
    }));

    try {
      const response = await fetch(`/api/views?viewId=${viewId}`);
      const result = await response.json();

      setData((prevData) => ({
        ...prevData,
        [viewId]: result,
      }));
    } catch (error) {
      console.error('Error fetching data:', error);
    } finally {
      setLoading((prevLoading) => ({
        ...prevLoading,
        [viewId]: false,
      }));
    }
  };

  const toggleView = (viewId: number) => {
    if (data[viewId]) {
      setData((prevData) => {
        const newData = { ...prevData };
        delete newData[viewId];
        return newData;
      });
    } else {
      fetchData(viewId);
    }
  };

  const clearAll = () => {
    setData({});
  };

  return (
    <div className="flex text-white">
      <header className="flex flex-col fixed p-1 mt-1 ml-3">
        <button onClick={clearAll} className="bg-slate-800 hover:bg-red-400 cursor-pointer">
          Clear All
        </button>
        {views.length > 0 ? (
          views.map((view) => (
            <button
              key={view.id}
              onClick={() => toggleView(view.id)}
              className={`${
                data[view.id] ? 'bg-gray-700' : 'bg-gray-500'
              } hover:bg-slate-500 p-1 cursor-pointer`}
            >
              {view.view_name}
            </button>
          ))
        ) : (
          <p className="text-white">No views available</p>
        )}
      </header>

      <main className="ml-40 mr-[1%] mt-2 w-full">
        {Object.values(loading).some((status) => status) && (
          <p className="text-white">Loading...</p>
        )}

        {!Object.values(loading).some((status) => status) && Object.keys(data).length > 0 && (
          <div className="flex flex-wrap gap-4">
            {Object.entries(data).map(([viewId, rows]) => (
              <div key={viewId} className="mb-4 w-full md:w-[48%] lg:w-[32%] flex-shrink-0 min-w-fit">
                <h2 className="text-xl font-semibold text-gray-700">View {viewId}</h2>
                {rows && rows.length > 0 ? (
                  <div className="overflow-auto">
                    <table className="table-auto border-collapse mt-4 w-full max-w-full">
                      <thead>
                        <tr>
                          {Object.keys(rows[0]).map((key) => (
                            <th
                              key={key}
                              className="border border-gray-300 p-2 text-white bg-gray-600 whitespace-nowrap"
                            >
                              {key}
                            </th>
                          ))}
                        </tr>
                      </thead>
                      <tbody>
                        {rows.map((row, rowIndex) => (
                          <tr key={rowIndex}>
                            {Object.values(row).map((value, colIndex) => (
                              <td
                                key={colIndex}
                                className="border border-gray-300 p-2 text-white bg-gray-500 whitespace-normal break-words"
                              >
                                {String(value)}
                              </td>
                            ))}
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                ) : (
                  <p className="text-white">No data available</p>
                )}
              </div>
            ))}
          </div>
        )}
      </main>
    </div>
  );
}
