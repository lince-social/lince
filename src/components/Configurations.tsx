import * as elements from "typed-html";
import { getActiveConfiguration, getInactiveConfigurations } from "../utils/getData";

function ConfigurationRow({ configurationItem }) {
  return (
    <div class={`flex space space-x-1 p-2`}>
      <form action={() => handleConfigurationClick(configurationItem.id)}>
        <button
          class={`p-1 rounded ${configurationItem.quantity === 1
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
  );
}


export default async function Configurations() {
  const activeConfiguration = await getActiveConfiguration()
  const inactiveConfigurations = await getInactiveConfigurations()

  return (
    <div class="group flex bg-slate-950/80 w-full items-center rounded-t hover:rounded-b-none relative">
      <svg
        xmlns="http://www.w3.org/2000/svg"
        fill="none"
        viewBox="0 0 24 24"
        strokeWidth={1.5}
        stroke="currentColor"
        class="size-6 group-hover:rotate-180 transition ease-in-out duration-300"
      >
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          d="m19.5 8.25-7.5 7.5-7.5-7.5"
        />
      </svg>
      <div class="w-full">
        <ConfigurationRow
          key={activeConfiguration[0].id}
          configurationItem={activeConfiguration[0]}
        />
      </div>
      <div class="pl-6 absolute top-full left-0 w-full bg-slate-950/80 rounded-b shadow-lg max-h-0 overflow-hidden transition-all duration-300 group-hover:max-h-screen">
        {inactiveConfigurations.map((inactiveConfiguration) => (
          <ConfigurationRow
            key={inactiveConfiguration.id}
            configurationItem={inactiveConfiguration}
          />
        ))}
      </div>
    </div>
  );
}

