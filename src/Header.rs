import * as elements from "typed-html";
import Nav from "../sections/Nav";

export default async function Header() {
  const nav = await (<Nav />);
  return (
    <header>
      <div>{nav}</div>
    </header>
  );
}
