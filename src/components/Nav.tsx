import Bar from "./Bar";

export default async function Nav() {
  const myConfigurations: string[] = ["config1", "config2"];
  const viewNamesResponse = await fetch(
    "http://localhost:3000/api/views/names",
  );
  const viewNames = await viewNamesResponse.json();
  return (
    <div className="space-y-1 m-2">
      <Bar barList={myConfigurations} barType="configuration" />
      <Bar barList={viewNames} barType="view" />
    </div>
  );
}
