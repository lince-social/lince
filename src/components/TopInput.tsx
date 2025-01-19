"use client";

import handleOperation from "@/scripts/handleOperation";

export default function TopInput() {
  return (
    <>
      <div className="">
        <form onSubmit={handleOperation} className="space-x-2">
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
