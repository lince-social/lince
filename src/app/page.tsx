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
  <div className="flex text-white">
  <header className="flex flex-col fixed p-1 mt-1 ml-3">
      <button onClick={clearAll} className="bg-slate-800 hover:bg-red-400 cursor-pointer">x</button>
      {views.length > 0 ? (
        views.map((view) => (
          <button key={view.id} onClick={() => toggleView(view.id)}
            className={`${data[view.id] ? 'bg-gray-700' : 'bg-gray-500'} hover:bg-slate-500 p-1 cursor-pointer`} >
            {view.view_name} </button> )) ) : ( <p className="text-white">No views available</p> )}
  </header>
  <main className="flex-1 ml-[9%] mr-[1%] mt-2">
    {Object.values(loading).some((status) => status) && <p className="text-white">Loading...</p>}

    {!Object.values(loading).some((status) => status) && Object.keys(data).length > 0 && (
      <div>
        {Object.entries(data).map(([viewId, rows]) => (
          <div key={viewId} className="mb-8 ">
            <h2 className="text-xl font-semibold text-gray-700">View {viewId}</h2>
            {rows && rows.length > 0 ? (
              <table className="table-auto w-full border-collapse mt-4">
                <thead>
                  <tr> {Object.keys(rows[0]).map((key, index) => (
                      <th key={key} className={`border border-gray-300 p-2 text-white bg-gray-600 ${ index === Object.keys(rows[0]).length - 1 ? 'w-full' : 'whitespace-nowrap' }`} > {key} </th> ))} </tr>
                </thead>
                <tbody>
                  {rows.map((row, rowIndex) => (
                    <tr key={rowIndex}> {Object.values(row).map((value, colIndex) => (
                        <td key={colIndex} className={`border border-gray-300 p-2 text-white bg-gray-500 ${ colIndex === Object.values(row).length - 1 ? 'w-full' : 'whitespace-nowrap' }`} > {String(value)} </td> ))}
                    </tr> ))}
                </tbody>
              </table>
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
