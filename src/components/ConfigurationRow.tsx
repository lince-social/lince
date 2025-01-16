"use client";
import ConfigurationButton from "./ConfigurationButton";
import Views from "./Views";
import handleConfigurationClick from "@/scripts/handleConfigurationChange";

export default function ConfigurationRow({ configurationItem }) {
  return (
    <>
      <div className={`flex space space-x-1 p-2`}>
        <form action={() => handleConfigurationClick(configurationItem.id)}>
          <ConfigurationButton configurationItem={configurationItem} />
        </form>
        <Views views={Object.entries(configurationItem.views)} />
      </div>
    </>
  );
}
