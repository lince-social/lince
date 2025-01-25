import * as elements from "typed-html";
import { sql } from "bun";
import Views from "./Views";

export async function ConfigurationRow({ configurationItem }) {
  const views = await (
    <Views
      views={Object.entries(configurationItem.views)}
      configurationId={configurationItem.id}
    />
  );

  return (
    <div class="flex space space-x-1 p-2">
      <button
        class={`p-1 rounded ${configurationItem.quantity === 1 ? "bg-red-800 hover:bg-red-900" : "hover:bg-blue-900 bg-blue-950"}`}
      >
        {configurationItem.configuration_name}
      </button>
      {views}
    </div>
  );
}

export default async function Configurations() {
  const activeConfiguration =
    await sql`SELECT id, configuration_name, quantity, views FROM configuration WHERE quantity = 1`;
  const activeConfigurationRow = await (
    <ConfigurationRow
      key={activeConfiguration[0].id}
      configurationItem={activeConfiguration[0]}
    />
  );

  const inactiveConfigurations =
    await sql`SELECT id, configuration_name, quantity, views  FROM configuration WHERE quantity <> 1`;
  const inactiveConfigurationsRows = inactiveConfigurations.map(
    (inactiveConfiguration) => (
      <ConfigurationRow
        key={inactiveConfiguration.id}
        configurationItem={inactiveConfiguration}
      />
    ),
  );

  return (
    <div class="group flex bg-slate-950/80 w-full items-center rounded-t hover:rounded-b-none relative text-white">
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
      <div class="w-full">{activeConfigurationRow}</div>
      <div class="pl-6 absolute top-full left-0 w-full bg-slate-950/80 rounded-b shadow-lg max-h-0 overflow-hidden transition-all duration-300 group-hover:max-h-screen">
        {inactiveConfigurationsRows}
      </div>
    </div>
  );
}
