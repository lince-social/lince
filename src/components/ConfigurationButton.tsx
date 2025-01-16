"use client";
import { useFormStatus } from "react-dom";

export default function ConfigurationButton({ configurationItem }) {
  const status = useFormStatus();
  return (
    <>
      <button
        className={`p-1 rounded ${
          configurationItem.quantity === 1
            ? "bg-red-800 hover:bg-red-900"
            : "hover:bg-blue-900 bg-blue-950"
        }`}
        disabled={status.pending}
      >
        {configurationItem.configurationName}
      </button>
    </>
  );
}
