import * as elements from "typed-html";
import Header from "./Header";
import Main from "./Main";

export default async function Body() {
  const header = await <Header />
  const main = await <Main />
  return (
    <div>
      {header}
      {main}
    </div>
  )
}
