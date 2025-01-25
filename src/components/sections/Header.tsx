import * as elements from "typed-html";
import Nav from "./Nav";
import Configurations from "../Configurations";

export default async function Header() {
  const nav = await <Nav />
  const configurations = await <Configurations />
  return (
    <div>
      {nav}
      {configurations}
    </div>
  );
}
