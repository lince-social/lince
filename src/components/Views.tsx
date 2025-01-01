"use client";

import { useState } from "react";

export default function Views({ initialViews /* , onViewsChange  */ }) {
  const [views, setViews] = useState(initialViews);

  async function handleClick(viewName) {
    try {
      const updatedViews = {
        ...views,
        [viewName]: !views[viewName],
      };

      const response = await fetch(
        `http://localhost:3000/api/configurations/views`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ updatedViews }),
        },
      );

      if (!response.ok) {
        console.error("Failed to update view state");
      } else {
        setViews(updatedViews);
        // onViewsChange(updatedViews); // Notify parent component about changes
      }
    } catch (error) {
      console.log("Error: ", error);
    }
  }

  return (
    <div className="rounded flex w-min space-x-1">
      {Object.keys(views).map((viewName, index) => (
        <button
          onClick={() => handleClick(viewName)}
          key={index}
          className={`rounded p-1 text-nowrap ${views[viewName]
              ? "bg-blue-700 hover:bg-blue-900 "
              : "bg-blue-900 hover:bg-blue-800 "
            }`}
        >
          {viewName}
        </button>
      ))}
    </div>
  );
}
