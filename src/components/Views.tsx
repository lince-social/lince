import * as elements from "typed-html";

export default function Views({ views, configurationId }) {
  return (
    <div class="flex w-min space-x-1">
      {Object.entries(views).map(([viewName, isActive]) => (
        <div
          key={viewName}
          class={`group flex space-x-1 rounded p-1 text-nowrap ${
            isActive
              ? "bg-slate-700 hover:bg-slate-900"
              : "bg-slate-900 hover:bg-slate-800"
          }`}
        >
          <button
            hx-post="/view"
            hx-target="#main"
            hx-vals={`js:{ views: ${JSON.stringify(views)}, view: { "${viewName}": ${isActive} }, configurationId: ${configurationId} }`}
            hx-trigger="click"
          >
            {viewName}
          </button>
          <button
            class="w-0 overflow-hidden opacity-0 bg-red-600 hover:bg-red-500 rounded text-white items-center justify-center transition-all duration-200 group-hover:w-6 group-hover:opacity-100 text-transparent group-hover:text-red"
            hx-trigger="click"
            hx-delete="/view"
            hx-target="#main"
            hx-vals={`js:{viewName: "${viewName}", configurationId: ${configurationId}}`}
          >
            x
          </button>
        </div>
      ))}
      <div class="flex items-center justify-center">
        <button class="flex items-center justify-center bg-gray-200 text-black hover:bg-white ml-2 w-6 h-6 rounded">
          +
        </button>
      </div>
    </div>
  );
}
