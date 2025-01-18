"use client";

export default function TopInput() {
  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = event.currentTarget;
    const formData = new FormData(event.currentTarget);
    const inputValue = formData.get("operationInput") as string;

    if (!inputValue.trim()) {
      alert("Input cannot be empty");
      return;
    }

    try {
      const response = await fetch("http://localhost:3000/api/operation", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ operation: inputValue }),
      });

      if (!response.ok) {
        throw new Error("Failed to send data");
      }

      form.reset();
    } catch (error) {
      console.error("Error:", error);
      alert(`An error occurred while sending the data ${error}`);
    }
  }

  return (
    <>
      <div className="">
        <form onSubmit={handleSubmit} className="space-x-2">
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

// import { NextResponse } from "next/server";
//
// export async function POST(request: Request) {
//   try {
//     const body = await request.json();
//
//     if (!body.operation) {
//       return NextResponse.json({
//         success: false,
//         message: `Empty operation: "${body}"; Quotes should be empty`,
//       });
//     }
//     console.log(body.operation);
//
//     return NextResponse.json(
//       {
//         success: true,
//         message: `Successfully posted Operation: ${body.operation}`,
//       },
//       { status: 201 },
//     );
//   } catch (error) {
//     console.log(`Error posting an 'Operation': ${error}`);
//     return NextResponse.json(
//       { success: false, message: `Error posting an 'Operation': ${error}` },
//       { status: 500 },
//     );
//   }
// }
//
