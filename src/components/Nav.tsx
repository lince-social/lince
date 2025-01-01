import Configurations from "./Configurations";
import Style from "./Style";
import TopInput from "./TopInput";
import Views from "./Views";

export default async function Nav() {
  const configurationNamesResponse = await fetch(
    "http://localhost:3000/api/configurations",
  );
  const myConfigurations = await configurationNamesResponse.json();

  const viewNamesResponse = await fetch(
    "http://localhost:3000/api/configurations?active=true&views=true",
  );
  const myViews = await viewNamesResponse.json();

  return (
    <div className="space-y-2">
      <TopInput />
      <div className="flex">
        <Configurations initialConfigurations={myConfigurations} />
        <Style />
      </div>
      <Views initialViews={myViews} />
    </div>
  );
}
