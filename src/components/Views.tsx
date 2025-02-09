// import * as elements from "typed-html";
//
// export default function Views({ views, configurationId }) {
//   return (
//     <div class="flex w-min space-x-1">
//       {Object.entries(views).map(([viewName, isActive]) => (
//         <div
//           key={viewName}
//           class={`group flex space-x-1 rounded p-1 text-nowrap ${isActive
//               ? "bg-slate-700 hover:bg-slate-900"
//               : "bg-slate-900 hover:bg-slate-800"
//             }`}
//         >
//           <button
//             hx-trigger="click"
//             hx-put="/view"
//             hx-vals={`js:{ views: ${JSON.stringify(views)}, view: { "${viewName}": ${isActive} }, configurationId: ${configurationId} }`}
//             hx-target="#main"
//           >
//             {viewName}
//           </button>
//           <button
//             hx-trigger="click"
//             hx-delete="/view"
//             hx-vals={`js:{viewName: "${viewName}", configurationId: ${configurationId}}`}
//             hx-target="#main"
//             class="w-0 overflow-hidden opacity-0 bg-red-600 hover:bg-red-500 rounded text-white items-center justify-center transition-all duration-200 group-hover:w-6 group-hover:opacity-100 group-hover:text-red"
//           >
//             x
//           </button>
//         </div>
//       ))}
//       <form
//         hx-trigger="click"
//         hx-post={`/addviewcomponent/${configurationId}`}
//         hx-swap="outerHTML"
//         class="flex items-center justify-center"
//       >
//         <button class="flex items-center justify-center bg-gray-200 text-black hover:bg-white ml-2 w-6 h-6 rounded">
//           +
//         </button>
//         <input name="viewname" type="hidden" value="" />
//         <input name="query" type="hidden" value="" />
//       </form>
//     </div>
//   );
// }
// import { sql } from "bun";
// import * as elements from "typed-html";
//
// export default async function Views({ configurationId }) {
//   const results = await sql`
//     SELECT v.view_name, cv.is_active
//     FROM configuration_view cv
//     JOIN view v ON cv.view_id = v.id
//     WHERE cv.configuration_id = ${configurationId};
//   `;
//
//   const views = Object.fromEntries(
//     results.map(({ view_name, is_active }) => [view_name, is_active]),
//   );
//
//   return (
//     <div class="flex w-min space-x-1">
//       {Object.entries(views).map(([viewName, isActive]) => (
//         <div
//           key={viewName}
//           class={`group flex space-x-1 rounded p-1 text-nowrap ${isActive
//               ? "bg-slate-700 hover:bg-slate-900"
//               : "bg-slate-900 hover:bg-slate-800"
//             }`}
//         >
//           <button
//             hx-trigger="click"
//             hx-put="/view"
//             hx-vals={`js:{ view: { "${viewName}": ${isActive} }, configurationId: ${configurationId} }`}
//             hx-target="#main"
//           >
//             {viewName}
//           </button>
//           <button
//             hx-trigger="click"
//             hx-delete="/view"
//             hx-vals={`js:{viewName: "${viewName}", configurationId: ${configurationId}}`}
//             hx-target="#main"
//             class="w-0 overflow-hidden opacity-0 bg-red-600 hover:bg-red-500 rounded text-white items-center justify-center transition-all duration-200 group-hover:w-6 group-hover:opacity-100 group-hover:text-red"
//           >
//             x
//           </button>
//         </div>
//       ))}
//       <form
//         hx-trigger="click"
//         hx-post={`/addviewcomponent/${configurationId}`}
//         hx-swap="outerHTML"
//         class="flex items-center justify-center"
//       >
//         <button class="flex items-center justify-center bg-gray-200 text-black hover:bg-white ml-2 w-6 h-6 rounded">
//           +
//         </button>
//         <input name="viewname" type="hidden" value="" />
//         <input name="query" type="hidden" value="" />
//       </form>
//     </div>
//   );
// }
import { sql } from "bun";
import * as elements from "typed-html";

export default async function Views({ configurationId }) {
  const results = await sql`
    SELECT v.id AS view_id, v.view_name, cv.is_active
    FROM configuration_view cv
    JOIN view v ON cv.view_id = v.id
    WHERE cv.configuration_id = ${configurationId};
  `;

  const views = results.map(({ view_id, view_name, is_active }) => ({
    view_id,
    view_name,
    is_active,
  }));
  // console.log(views, configurationId);

  return (
    <div class="flex w-min space-x-1">
      {views.map(({ view_id, view_name, is_active }) => (
        <div
          key={view_id}
          class={`group flex space-x-1 rounded p-1 text-nowrap ${is_active
              ? "bg-slate-700 hover:bg-slate-900"
              : "bg-slate-900 hover:bg-slate-800"
            }`}
        >
          <button
            hx-trigger="click"
            hx-put="/view"
            hx-vals={`js:{ viewId: ${view_id}, isActive: ${is_active}, configurationId: ${configurationId} }`}
            hx-target="#main"
          >
            {view_name}
          </button>
          <button
            hx-trigger="click"
            hx-delete="/view"
            hx-vals={`js:{viewId: ${view_id}, configurationId: ${configurationId}}`}
            hx-target="#main"
            class="w-0 overflow-hidden opacity-0 bg-red-600 hover:bg-red-500 rounded text-white items-center justify-center transition-all duration-200 group-hover:w-6 group-hover:opacity-100 group-hover:text-red"
          >
            x
          </button>
        </div>
      ))}
      <form
        hx-trigger="click"
        hx-post={`/addviewcomponent/${configurationId}`}
        hx-swap="outerHTML"
        class="flex items-center justify-center"
      >
        <button class="flex items-center justify-center bg-gray-200 text-black hover:bg-white ml-2 w-6 h-6 rounded">
          +
        </button>
        <input name="viewname" type="hidden" value="" />
        <input name="query" type="hidden" value="" />
      </form>
    </div>
  );
}
