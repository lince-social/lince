import * as elements from "typed-html";
import Configurations from "../Configurations";

export default async function Header() {
  const configurations = await (<Configurations />);
  return <header>{configurations}</header>;
}
