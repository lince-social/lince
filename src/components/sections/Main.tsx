import * as elements from "typed-html";

import ConfigurationsUnhovered, {
  ConfigurationsHovered,
} from "../Configurations";
import Tables from "../Tables";

export default async function UnhoveredMain() {
  const configurations = await (<ConfigurationsUnhovered />);
  const tables = await (<Tables />);

  return (
    <main id="main">
      <div>{configurations}</div>
      <div>{tables}</div>
    </main>
  );
}

export async function HoveredMain() {
  const configurations = await (<ConfigurationsHovered />);
  const tables = await (<Tables />);

  return (
    <main id="main" class="">
      <div>{configurations}</div>
      <div>{tables}</div>
    </main>
  );
}
