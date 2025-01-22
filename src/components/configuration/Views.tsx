"use client";
import {
  handleViewToggle,
  handleViewRemove,
  handleViewAdd,
} from "@/scripts/handleView";
import { useFormStatus } from "react-dom";

export default function Views({ views, configurationId }) {
  const status = useFormStatus();
  return (
    <div className="flex w-min space-x-1">
      {views.map((view, index: number) => (
        <div
          key={view[0]}
          className={`group flex space-x-1 rounded p-1 text-nowrap ${view[1]
              ? "bg-slate-700 hover:bg-slate-900"
              : "bg-slate-900 hover:bg-slate-800"
            }`}
        >
          <form action={() => handleViewToggle(views, view, index)}>
            <button disabled={status.pending}>{view[0]}</button>
          </form>
          <form action={() => handleViewRemove(views, view, configurationId)}>
            <button
              disabled={status.pending}
              className="w-0 overflow-hidden opacity-0 transition-all duration-200 group-hover:w-6 group-hover:opacity-100 text-transparent group-hover:text-red-theme"
            >
              x
            </button>
          </form>
        </div>
      ))}
      <form
        className="flex items-center justify-center"
        action={() => handleViewAdd(views, configurationId)}
      >
        <button
          className="flex items-center justify-center bg-gray-200 text-black hover:bg-white ml-2 w-6 h-6 rounded"
          disabled={status.pending}
        >
          +
        </button>
      </form>
    </div>
  );
}
