import * as elements from "typed-html";
export default async function DataNotFormComponent(table, id, key, value) {
  return (
    <td>
      <form hx-put={`/datanotform/${table}/${key}/${id}`} hx-target="#body">
        <input
          name="value"
          value={decodeURIComponent(value)}
          class="text-white bg-gray-600"
        />
      </form>
    </td>
  );
}
