import NavLeft from "@/components/nav/NavLeft";
import NavCenter from "@/components/nav/NavCenter";
import NavRight from "@/components/nav/NavRight";

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

