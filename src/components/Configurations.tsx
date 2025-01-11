"use client";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import { faChevronDown } from "@fortawesome/free-solid-svg-icons";
import { useState } from "react";

async function handleConfigurationClick(configurationItem) {
  alert(configurationItem.configurationName);
  // await prisma.configuration.updateMany({
  //    data: { quantity: 0 },
  //  });
  //
  //  await prisma.configuration.update({
  //    where: { id: Number(updateQuantityId) },
  //    data: { quantity: 1 },
  //  });
}

function ConfigurationRow({ configurationItem }) {
  return (
    <div className={`space space-x-1 p-2`}>
      <button
        onClick={() => handleConfigurationClick(configurationItem)}
        className={`p-1 rounded ${
          configurationItem.quantity === 1
            ? "bg-red-800 hover:bg-red-900"
            : "hover:bg-blue-900 bg-blue-950"
        }`}
      >
        {configurationItem.configurationName}
      </button>
      {Object.entries(configurationItem.views).map(([viewName, isActive]) => (
        <button
          key={viewName}
          className={`rounded p-1 ${isActive ? "bg-slate-700 hover:bg-slate-800" : "bg-slate-800 hover:bg-slate-700 "}`}
        >
          {viewName}
        </button>
      ))}
    </div>
  );
}

export default function ConfigurationsBar({ activeConfig, inactiveConfigs }) {
  const [isHovered, setIsHovered] = useState(false);

  return (
    <div
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
      className="flex bg-crust-theme w-full items-center rounded-t rounded-b hover:rounded-b-none"
    >
      <FontAwesomeIcon icon={faChevronDown} className="pt-2 pb-2 pl-2" />
      <div className="w-full relative flex flex-col">
        <ConfigurationRow
          key={activeConfig.id}
          configurationItem={activeConfig}
        />
        <div
          className={`absolute bg-crust-theme top-full left-0 w-full overflow-hidden rounded-b ${
            isHovered ? "max-h-screen" : "max-h-0"
          }`}
        >
          {inactiveConfigs.map((inactiveConfig) => (
            <ConfigurationRow
              key={inactiveConfig.id}
              configurationItem={inactiveConfig}
            />
          ))}
        </div>
      </div>
    </div>
  );
}
