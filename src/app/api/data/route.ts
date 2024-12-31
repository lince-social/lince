import { NextResponse } from "next/server";
import { prisma } from "@lib/prisma";

export async function GET() {
  try {
    const response = await fetch("http://localhost:3000/api/views");

    const queries = await response.json();

    if (!queries) {
      return NextResponse.json(
        { success: false, error: "Missing viewQuery parameter" },
        { status: 400 },
      );
    }

    const data = await Promise.all(
      queries.map(
        async (viewQuery: string) => await prisma.$queryRawUnsafe(viewQuery),
      ),
    );

    return NextResponse.json(data);
  } catch (error) {
    console.error("Error in API:", error);
    return NextResponse.json(
      { success: false, error: "Internal Server Error" },
      { status: 500 },
    );
  }
}
