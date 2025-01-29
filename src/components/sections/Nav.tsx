import * as elements from "typed-html";
import OperationInput from "../OperationInput";

export default async function Nav() {
  return (
    <div>
      <nav>{<OperationInput />}</nav>
    </div>
  );
}
