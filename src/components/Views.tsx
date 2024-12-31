"use client";

import { useState } from "react";

export default function Views({ initialViews }) {
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
      }
    } catch (error) {
      console.log("Error: ", error);
    }
  }

  return (
    <div className="rounded flex w-min space-x-1 p-2 m-3 bg-blue-800">
      {Object.keys(views).map((viewName, index) => (
        <button
          onClick={() => handleClick(viewName)}
          key={index}
          className={`rounded bg-blue-600 hover:bg-blue-500 p-1 text-nowrap ${
            views[viewName]
              ? "border-2 border-gray-400"
              : "border-2 border-blue-800"
          }`}
        >
          {viewName}
        </button>
      ))}
    </div>
  );
}
