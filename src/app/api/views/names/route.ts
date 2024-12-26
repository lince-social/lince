import { NextResponse } from "next/server";
import { prisma } from "@lib/prisma";

export async function GET() {
  try {
    const views = await prisma.views.findMany({
      select: { viewName: true },
    });

    const viewNames = views.map((view) => view.viewName);

    return NextResponse.json(viewNames);
  } catch (error) {
    console.error("Error in API:", error);
    return NextResponse.json(
      { success: false, error: "Internal Server Error" },
      { status: 500 },
    );
  }
}
