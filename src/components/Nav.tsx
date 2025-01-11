import ConfigurationsBar from "./Configurations";
import Profile from "./Profile";
import Options from "./Options";

export default function Nav({ activeConfig, inactiveConfigs }) {
  return (
    <div className="flex space-x-2 justify-between items-center bg-base-theme rounded m-2 p-2">
      <ConfigurationsBar
        activeConfig={activeConfig}
        inactiveConfigs={inactiveConfigs}
      />
      <div className="flex space-x-2">
        <Profile />
        <Options />
      </div>
    </div>
  );
}
