"use client";

import handleViewChange from "@/scripts/handleViewChange";
import ViewButtton from "./ViewButton";

export default function Views({ views }) {
  return (
    <div className="flex w-min space-x-1">
      {views.map((view, index) => (
        <form
          action={() => handleViewChange(views, view, index)}
          key={view[0]}
          className={`rounded p-1 text-nowrap ${view[1] ? "bg-slate-700 hover:bg-slate-900 " : "bg-slate-900 hover:bg-slate-800 "}`}
        >
          <ViewButtton>{view[0]}</ViewButtton>
        </form>
      ))}
    </div>
  );
}
