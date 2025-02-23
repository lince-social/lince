// import { sql } from "bun";
// import * as elements from "typed-html";
// 
// export default async function TableOptions() {
//   const tableNames =
//     await sql`SELECT table_name FROM information_schema.tables WHERE table_schema = 'public'`;
// 
//   return (
//     <div id="tableoptions">
//       <form>
//         <select
//           hx-post="/operation"
//           hx-trigger="change"
//           hx-target="#body"
//           name="operation"
//           class="bg-black border m-4 border-white rounded text-white font-bold"
//         >
//           <option disabled selected value="">
//             Table
//           </option>
//           {tableNames.map((tableItem) => (
//             <option
//               class="text-white font-bold bg-black border border-white m-4 p-2"
//               key={tableItem.table_name}
//               value={tableItem.table_name}
//             >
//               {tableItem.table_name}
//             </option>
//           ))}
//         </select>
//       </form>
//     </div>
//   );
// }
