// export default function Table({ data }) {
//   if (!data || data.length === 0) {
//     return <p className="text-center text-gray-500">No data available</p>;
//   }
//
//   const headers = Object.keys(data[0]);
//
// //   <div className="overflow-x-auto my-4 rounded shadow">
// //     <table className="min-w-full table-auto border-collapse border border-gray-200">
// //       <thead className="bg-green-700 text-white">
// //         <tr>
// //           {headers.map((header) => (
// //             <th
// //               key={header}
// //               className="px-4 py-2 text-left border-b-2 border-gray-300"
// //             >
// //               {header.charAt(0).toUpperCase() + header.slice(1)}{" "}
// //               {/* Capitalize headers */}
// //             </th>
// //           ))}
// //         </tr>
// //       </thead>
// //       <tbody>
// //         {data.map((row, rowIndex) => (
// //           <tr
// //             key={rowIndex}
// //             className={`${
// // rowIndex % 2 === 0 ? "bg-gray-100" : "bg-white"
// // } text-black`}
// //           >
// //             {headers.map((header) => (
// //               <td key={header} className="px-4 py-2 border-b border-gray-300">
// //                 {row[header] !== null ? row[header].toString() : ""}
// //               </td>
// //             ))}
// //           </tr>
// //         ))}
// //       </tbody>
// //     </table>
// //   </div>
//   return (
//     <pre></pre>
//   );
// }

interface TableProps {
  data: any;
}

export default function Table({ data }: TableProps) {
  return (
    <div className="p-4 bg-gray-800 rounded">
      <pre>{JSON.stringify(data, null, 2)}</pre>
    </div>
  );
}
