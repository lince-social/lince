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

export async function DELETE(request: Request) {
  try {
    const url = new URL(request.url);
    const id = url.searchParams.get("id");
    const tableName = url.searchParams.get("tableName");

    if (!id) {
      return NextResponse.json(
        { success: false, error: "Missing id parameter" },
        { status: 400 },
      );
    }

    if (!tableName) {
      return NextResponse.json(
        { success: false, error: "Missing tableName parameter" },
        { status: 400 },
      );
    }

    await prisma[tableName].delete({
      where: {
        id: Number(id),
      },
    });

    return NextResponse.json(
      {
        success: true,
        message: `ID '${id}' in table '${tableName}' deleted successfully`,
      },
      { status: 200 },
    );
  } catch (error) {
    console.error("Error in API:", error);
    return NextResponse.json(
      { success: false, error: `Internal Server Error ${error}` },
      { status: 500 },
    );
  }
}
