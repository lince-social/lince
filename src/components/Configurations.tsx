import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import { faChevronDown } from "@fortawesome/free-solid-svg-icons";
import ConfigurationRow from "./ConfigurationRow";

export default function Configurations({ activeConfig, inactiveConfigs }) {
  return (
    <div className="group flex bg-crust-theme w-full items-center rounded-t hover:rounded-b-none relative">
      <FontAwesomeIcon
        icon={faChevronDown}
        width={35}
        height={35}
        className="pt-2 pb-2 pl-2"
      />
      <div className="w-full">
        <ConfigurationRow
          key={activeConfig.id}
          configurationItem={activeConfig}
        />
      </div>
      <div className="pl-6 absolute top-full left-0 w-full bg-crust-theme rounded-b shadow-lg max-h-0 overflow-hidden transition-all duration-300 group-hover:max-h-screen">
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
