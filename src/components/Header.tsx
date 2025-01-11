import Nav from "./Nav";
import TopInput from "./TopInput";

export default async function Header({ activeConfig, inactiveConfigs }) {
  return (
    <>
      <Nav activeConfig={activeConfig} inactiveConfigs={inactiveConfigs} />
    </>
  );
}
