"use client";
import Views from "./Views";
import handleConfigurationClick from "@/scripts/handleConfigurationChange";
import { useFormStatus } from "react-dom";

export default function ConfigurationRow({ configurationItem }) {
  const status = useFormStatus();

  return (
    <>
      <div className={`flex space space-x-1 p-2`}>
        <form action={() => handleConfigurationClick(configurationItem.id)}>
          <button
            className={`p-1 rounded ${configurationItem.quantity === 1
                ? "bg-red-800 hover:bg-red-900"
                : "hover:bg-blue-900 bg-blue-950"
              }`}
            disabled={status.pending}
          >
            {configurationItem.configurationName}
          </button>
        </form>
        <Views
          views={Object.entries(configurationItem.views)}
          configurationId={configurationItem.id}
        />
      </div>
    </>
  );
}
