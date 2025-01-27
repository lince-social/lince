import * as elements from "typed-html";
import Nav from "../Nav";

export default async function Header() {
  const nav = await (<Nav />);
  return (
    <header>
      <div>{nav}</div>
    </header>
  );
}
