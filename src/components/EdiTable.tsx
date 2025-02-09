// import * as elements from "typed-html";
// export default async function DataNotFormComponent(table, id, key, value) {
//   try {
//     return (
//       <td>
//         <form hx-put={`/datanotform/${table}/${key}/${id}`} hx-target="#body">
//           <input
//             name="value"
//             value={decodeURIComponent(value)}
//             class="text-white bg-gray-600"
//           />
//         </form>
//       </td>
//     );
//   } catch (error) {
//     console.log(error);
//   }
// }
import * as elements from "typed-html";

export default async function DataNotFormComponent(table, id, key, value) {
  try {
    // Escape single quotes for SQL
    const sanitizedValue = decodeURIComponent(value).replace(/'/g, "''");

    return (
      <td>
        <form hx-put={`/datanotform/${table}/${key}/${id}`} hx-target="#body">
          <input
            name="value"
            value={sanitizedValue}
            class="text-white  bg-gray-600"
          />
        </form>
      </td>
    );
  } catch (error) {
    console.log(error);
  }
}
