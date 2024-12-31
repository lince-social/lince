import Configurations from "./Configurations";
import Views from "./Views";

export default async function Nav() {
  const configurationNamesResponse = await fetch(
    "http://localhost:3000/api/configurations",
  );
  const myConfigurations = await configurationNamesResponse.json();

  const viewNamesResponse = await fetch("http://localhost:3000/api/views");
  const myViews = await viewNamesResponse.json();

  return (
    <div className="space-y-1 m-2">
      <Configurations initialConfigurations={myConfigurations} />
      <Views views={myViews} />
    </div>
  );
}
