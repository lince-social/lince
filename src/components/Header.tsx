import NavCenter from "./NavCenter";
import NavLeft from "./NavLeft";
import NavRight from "./NavRight";

export default function Header() {
  return (
    <>
      <div className="m-4 flex justify-between">
        <NavLeft />
        <div className="flex">
          <NavCenter />
          <NavRight />
        </div>
      </div>
    </>
  );
}
