"use client";
import { useFormStatus } from "react-dom";

export default function ViewButtton({ children }) {
  const status = useFormStatus();
  return (
    <>
      <button disabled={status.pending}>{children}</button>
    </>
  );
}
