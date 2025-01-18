"use client";
import Link from "next/link";
import TopInput from "./TopInput";
import { usePathname } from "next/navigation";

export default function Header() {
  const pathName = usePathname();

  return (
    <>
      <div className="space-y-2">
        <div className="flex justify-between m-4">
          <TopInput />
          <div className="flex space-x-2 text-gray-500">
            <Link
              className={`hover:text-gray-300 ${pathName === "/" ? "text-white" : ""}`}
              href={"/"}
            >
              Home
            </Link>
            <Link
              className={`hover:text-gray-300 ${pathName === "/options" ? "text-white" : ""}`}
              href={"/options"}
            >
              Options
            </Link>
            <Link
              className={`hover:text-gray-300 ${pathName === "/profile" ? "text-white" : ""}`}
              href={"/profile"}
            >
              Profile
            </Link>
          </div>
        </div>
      </div>
    </>
  );
}
