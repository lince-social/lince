use maud::{Markup, html};

pub async fn presentation_web_table_editable_row(
    table: String,
    id: String,
    column: String,
    value: String,
) -> Markup {
    html!(td {input
        type="text"
        name="value"
        hx-params="*"
        // hx-trigger="keyup[Enter]"
        hx-patch=(format!("/table/{}/{}/{}", table, id, column)) value=(value){}})
}
// import * as elements from "typed-html";
//
// export default async function DataNotFormComponent(table, id, key, value) {
//   try {
//     const sanitizedValue = decodeURIComponent(value).replace(/'/g, "''");
//
//     return (
//       <td>
//         <form hx-put={`/datanotform/${table}/${key}/${id}`} hx-target="#body">
//           <input
//             name="value"
//             value={sanitizedValue}
//             class="text-white bg-gray-600"
//           />
//         </form>
//       </td>
//     );
//   } catch (error) {
//     console.log(error);
//   }
// }
