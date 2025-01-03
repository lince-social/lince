import { NextResponse } from "next/server";
import { prisma } from "@lib/prisma";

export async function GET() {
  try {
    const views = await prisma.views.findMany({
      select: { view: true },
    });
    const view = views.map((view) => view.view);

    return NextResponse.json(view);
  } catch (error) {
    console.error("Error in API:", error);
    return NextResponse.json(
      { success: false, error: "Internal Server Error" },
      { status: 500 },
    );
  }
}
