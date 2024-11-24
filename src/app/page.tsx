'use client'
// import { useEffect, useState } from 'react';

// export default function Home() {
//   const [views, setViews] = useState<{ id: number; view_name: string }[]>([]);
//   const [data, setData] = useState<any>({});
//   const [loading, setLoading] = useState(false);

//   useEffect(() => {
//     // Fetch available views
//     const fetchViews = async () => {
//       try {
//         const response = await fetch('/api/views');
//         const result = await response.json();
//         setViews(result);
//       } catch (error) {
//         console.error('Error fetching views:', error);
//       }
//     };

//     fetchViews();
//   }, []);

//   const fetchData = async (viewId: number) => {
//     setLoading(true);

//     try {
//       const response = await fetch(`/api/views?viewId=${viewId}`);
//       const result = await response.json();

//       // Store the result for each viewId
//       setData((prevData) => ({
//         ...prevData,
//         [viewId]: result,
//       }));
//     } catch (error) {
//       console.error('Error fetching data:', error);
//     } finally {
//       setLoading(false);
//     }
//   };

//   const toggleView = (viewId: number) => {
//     // Check if the view is already toggled
//     if (data[viewId]) {
//       // If it's toggled, remove it from the data
//       setData((prevData) => {
//         const newData = { ...prevData };
//         delete newData[viewId];
//         return newData;
//       });
//     } else {
//       // Otherwise, fetch the data and add it to the data object
//       fetchData(viewId);
//     }
//   };

//   const clearAll = () => {
//     setData({});
//   };

//   return (
//     <div style={{ padding: '20px' }}>
//       <h1>Dynamic Views</h1>

//       {/* Toggle Buttons */}
//       <div style={{ marginBottom: '20px' }}>
//         {views.map((view) => (
//           <button
//             key={view.id}
//             onClick={() => toggleView(view.id)}
//             style={{
//               padding: '10px 20px',
//               margin: '5px',
//               background: data[view.id] ? '#0070f3' : '#ccc',
//               color: '#fff',
//               border: 'none',
//               cursor: 'pointer',
//             }}
//           >
//             {view.view_name}
//           </button>
//         ))}
//       </div>

//       {/* Clear All Button */}
//       <div style={{ marginBottom: '20px' }}>
//         <button
//           onClick={clearAll}
//           style={{
//             padding: '10px 20px',
//             background: '#e60000',
//             color: '#fff',
//             border: 'none',
//             cursor: 'pointer',
//           }}
//         >
//           Clear All
//         </button>
//       </div>

//       {/* Loading Indicator */}
//       {loading && <p>Loading...</p>}

//       {/* Dynamic Tables */}
//       {!loading && Object.keys(data).length > 0 && (
//         <div>
//           {Object.entries(data).map(([viewId, rows]) => (
//             <div key={viewId} style={{ marginBottom: '20px' }}>
//               <h2>View {viewId}</h2>
//               <table style={{ width: '100%', borderCollapse: 'collapse' }}>
//                 <thead>
//                   <tr>
//                     {Object.keys(rows[0]).map((key) => (
//                       <th key={key} style={{ border: '1px solid #ddd', padding: '8px' }}>
//                         {key}
//                       </th>
//                     ))}
//                   </tr>
//                 </thead>
//                 <tbody>
//                   {rows.map((row, rowIndex) => (
//                     <tr key={rowIndex}>
//                       {Object.values(row).map((value, colIndex) => (
//                         <td key={colIndex} style={{ border: '1px solid #ddd', padding: '8px' }}>
//                           {String(value)}
//                         </td>
//                       ))}
//                     </tr>
//                   ))}
//                 </tbody>
//               </table>
//             </div>
//           ))}
//         </div>
//       )}
//     </div>
//   );
// }

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
    <div style={{ padding: '20px' }}>
      <h1>Dynamic Views</h1>

      {/* Toggle Buttons */}
      <div style={{ marginBottom: '20px' }}>
        {views.length > 0 ? (
          views.map((view) => (
            <button
              key={view.id}
              onClick={() => toggleView(view.id)}
              style={{
                padding: '10px 20px',
                margin: '5px',
                background: data[view.id] ? '#0070f3' : '#ccc',
                color: '#fff',
                border: 'none',
                cursor: 'pointer',
              }}
            >
              {view.view_name}
            </button>
          ))
        ) : (
          <p>No views available</p>
        )}
      </div>

      {/* Clear All Button */}
      <div style={{ marginBottom: '20px' }}>
        <button
          onClick={clearAll}
          style={{
            padding: '10px 20px',
            background: '#e60000',
            color: '#fff',
            border: 'none',
            cursor: 'pointer',
          }}
        >
          Clear All
        </button>
      </div>

      {/* Loading Indicator */}
      {Object.values(loading).some((status) => status) && <p>Loading...</p>}

      {/* Dynamic Tables */}
      {!Object.values(loading).some((status) => status) && Object.keys(data).length > 0 && (
        <div>
          {Object.entries(data).map(([viewId, rows]) => (
            <div key={viewId} style={{ marginBottom: '20px' }}>
              <h2>View {viewId}</h2>
              {rows && rows.length > 0 ? (
                <table style={{ width: '100%', borderCollapse: 'collapse' }}>
                  <thead>
                    <tr>
                      {Object.keys(rows[0]).map((key) => (
                        <th key={key} style={{ border: '1px solid #ddd', padding: '8px' }}>
                          {key}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {rows.map((row, rowIndex) => (
                      <tr key={rowIndex}>
                        {Object.values(row).map((value, colIndex) => (
                          <td key={colIndex} style={{ border: '1px solid #ddd', padding: '8px' }}>
                            {String(value)}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              ) : (
                <p>No data available</p>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

