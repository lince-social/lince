import * as elements from "typed-html";
import Header from "./Header";
import Main from "./Main";

export default async function Body() {
  const header = await (<Header />);
  const main = await (<Main />);
  return (
    <div id="body" class="space-y-2">
      <div>{header}</div>
      <div>{main}</div>
    </div>
  );
}

export async function FatherBody({ children }) {
  const header = await (<Header />);
  const main = await (<Main />);
  return (
    <div id="body" class="space-y-2">
      <div>{header}</div>
      <div>{main}</div>
      <div>{children}</div>
    </div>
  );
}
