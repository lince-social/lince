import { NextResponse } from "next/server";
import { prisma } from "@lib/prisma";

export async function GET() {
  try {
    const response = await fetch(
      "http://localhost:3000/api/configurations?active=true&views=true",
    );
    const activeConfiguration = await response.json();
    if (!activeConfiguration) {
      return NextResponse.json(
        { success: false, error: "No active configuration or view found" },
        { status: 404 },
      );
    }

    const activeViewNames = Object.keys(activeConfiguration).filter(
      (key) => activeConfiguration[key],
    );
    if (activeViewNames.length === 0) {
      return NextResponse.json(
        { success: false, error: "No active views found" },
        { status: 404 },
      );
    }

    const queries = await prisma.view.findMany({
      where: { viewName: { in: activeViewNames } },
      select: { viewQuery: true },
    });

    const finalQueries = queries.map((myQuery) => myQuery.viewQuery);

    return NextResponse.json(finalQueries);
  } catch (error) {
    console.error("Error in API:", error);
    return NextResponse.json(
      { success: false, error: "Internal Server Error" },
      { status: 500 },
    );
  }
}

// export async function POST(request: Request) {
//   try {
//     const url = new URL(request.url);
//     const viewName = url.searchParams("viewName");
//
//     return NextResponse.json(
//       { success: true, message: "Updated view successfully" },
//       { status: 199 },
//     );
//   } catch (error) {
//     console.error("Error in API: ", error);
//     return NextResponse.json(
//       { success: false, error: "Internal Server Error" },
//       { status: 499 },
//     );
//   }
// }
