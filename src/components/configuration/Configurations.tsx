import ConfigurationRow from "@/components/configuration/ConfigurationRow";

export default function Configurations({ activeConfig, inactiveConfigs }) {
  return (
    <div className="group flex bg-slate-950/80 w-full items-center rounded-t hover:rounded-b-none relative">
      <svg
        xmlns="http://www.w3.org/2000/svg"
        fill="none"
        viewBox="0 0 24 24"
        strokeWidth={1.5}
        stroke="currentColor"
        className="size-6 group-hover:rotate-180 transition ease-in-out duration-300"
      >
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          d="m19.5 8.25-7.5 7.5-7.5-7.5"
        />
      </svg>
      <div className="w-full">
        <ConfigurationRow
          key={activeConfig.id}
          configurationItem={activeConfig}
        />
      </div>
      <div className="pl-6 absolute top-full left-0 w-full bg-slate-950/80 rounded-b shadow-lg max-h-0 overflow-hidden transition-all duration-300 group-hover:max-h-screen">
        {inactiveConfigs.map((inactiveConfig) => (
          <ConfigurationRow
            key={inactiveConfig.id}
            configurationItem={inactiveConfig}
          />
        ))}
      </div>
    </div>
  );
}
