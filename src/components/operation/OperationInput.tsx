"use client";

import { useRouter } from "next/navigation";

export default function TopInput() {
  const router = useRouter();
  function routeIt(event) {
    event.preventDefault();
    const form = event.currentTarget;
    const formData = new FormData(form);
    form.reset();
    const operationInput: string = formData.get("operationInput");
    router.push(`/operation/${operationInput}`);
  }

  return (
    <>
      <div className="">
        <form onSubmit={routeIt} className="space-x-2">
          <input
            type="text"
            name="operationInput"
            placeholder="Operation"
            className="rounded"
            required
          />
          <button
            className="bg-gray-600 hover:bg-gray-500 rounded pl-1 pr-1"
            type="submit"
          >
            Send
          </button>
        </form>
      </div>
    </>
  );
}
