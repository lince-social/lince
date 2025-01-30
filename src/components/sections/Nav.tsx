import * as elements from "typed-html";
import OperationInput from "../OperationInput";
import ClosedTableOptions from "../TableOptions";

export default async function Nav() {
  const ClosedTableOptionsComponent = await ClosedTableOptions();
  return (
    <nav class="flex items-center justify-between">
      <OperationInput />
      {ClosedTableOptionsComponent}
    </nav>
  );
}
