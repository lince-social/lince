import NavCenter from "./NavCenter";
import NavLeft from "./NavLeft";
import NavRight from "./NavRight";

export default function Header() {
  return (
    <>
      <div className="m-4 flex justify-between items-center">
        <NavLeft />
        <div className="flex space-x-2 items-center">
          <NavCenter />
          <NavRight />
        </div>
      </div>
    </>
  );
}
