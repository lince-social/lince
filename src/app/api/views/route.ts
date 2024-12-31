import { prisma } from "@lib/prisma";
import { NextResponse } from "next/server";

export async function GET() {
  try {
    const activeConfiguration = await prisma.configuration.findFirst({
      where: { quantity: 1 },
      select: {
        views: true,
      },
    });

    if (!activeConfiguration) {
      return NextResponse.json(
        { success: false, error: "No active configuration found" },
        { status: 404 },
      );
    }
    const views = activeConfiguration.views;

    if (!views) {
      return NextResponse.json(
        { success: false, error: "No view found" },
        { status: 404 },
      );
    }

    const data = await prisma.view.findMany({
      where: { viewName: "" },
    });

    return NextResponse.json(data);
  } catch (error) {
    console.error("Error in API:", error);
    return NextResponse.json(
      { success: false, error: "Internal Server Error" },
      { status: 500 },
    );
  }
}
