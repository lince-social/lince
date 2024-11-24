'use client'
import { useEffect, useState } from 'react';

export default function Home() {
  const [views, setViews] = useState<{ id: number; view_name: string }[]>([]);
  const [data, setData] = useState<{ [viewId: number]: any[] }>({});
  const [loading, setLoading] = useState<{ [viewId: number]: boolean }>({});

  useEffect(() => {
    const fetchViews = async () => {
      try {
        const response = await fetch('/api/views');
        const result = await response.json();
        if (Array.isArray(result)) {
          setViews(result);
        } else {
          console.error('Invalid data format for views:', result);
        }
      } catch (error) {
        console.error('Error fetching views:', error);
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
  <div className="bg-indigo-950 p-2">
  <header className="flex">
    <div className="mb-4">
      <button
        onClick={clearAll}
        className="px-4 py-2 bg-red-600 text-white border-none cursor-pointer"
      >
        Clear All
      </button>
    </div>
    <div className="mb-4">
      {views.length > 0 ? (
        views.map((view) => (
          <button
            key={view.id}
            onClick={() => toggleView(view.id)}
            className={`px-4 py-2 m-1 ${data[view.id] ? 'bg-blue-500' : 'bg-gray-300'} text-white border-none cursor-pointer`}
          >
            {view.view_name}
          </button>
        ))
      ) : (
        <p className="text-white">No views available</p>
      )}
    </div>
    </header>


    {Object.values(loading).some((status) => status) && <p className="text-white">Loading...</p>}

    {!Object.values(loading).some((status) => status) && Object.keys(data).length > 0 && (
      <div>
        {Object.entries(data).map(([viewId, rows]) => (
          <div key={viewId} className="mb-8">
            <h2 className="text-xl font-semibold text-white">View {viewId}</h2>
            {rows && rows.length > 0 ? (
              <table className="w-full border-collapse mt-4">
                <thead>
                  <tr>
                    {Object.keys(rows[0]).map((key) => (
                      <th key={key} className="border border-gray-300 p-2 text-white">
                        {key}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {rows.map((row, rowIndex) => (
                    <tr key={rowIndex}>
                      {Object.values(row).map((value, colIndex) => (
                        <td key={colIndex} className="border border-gray-300 p-2 text-white">
                          {String(value)}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            ) : (
              <p className="text-white">No data available</p>
            )}
          </div>
        ))}
      </div>
    )}
  </div>
);
