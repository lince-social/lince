"use client";

import { useState } from "react";

export default function ConfigurationBar({
  initialConfigurations,
}: {
  initialConfigurations: any[];
}) {
  const [configurations, setConfigurations] = useState(initialConfigurations);

  const handleClick = async (id: number) => {
    try {
      const response = await fetch(
        "http://localhost:3000/api/configurations/quantities",
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ id }),
        },
      );

      if (response.ok) {
        setConfigurations((prevConfigurations) =>
          prevConfigurations.map((config) =>
            config.id === id
              ? { ...config, quantity: 1 }
              : { ...config, quantity: 0 },
          ),
        );
      } else {
        console.log("Failed to update quantity");
      }
    } catch (error) {
      console.log("Error updating quantity:", error);
    }
  };

  return (
    <div className="rounded flex w-min space-x-1 p-2 m-3 bg-red-800">
      {configurations.map((config) => (
        <button
          key={config.id}
          onClick={() => handleClick(config.id)}
          className={`rounded bg-red-600 hover:bg-red-500 p-1 text-nowrap ${
            config.quantity === 1
              ? "border-2 border-gray-400"
              : "border-2 border-red-800"
          }`}
        >
          {config.configurationName}
        </button>
      ))}
    </div>
  );
}
