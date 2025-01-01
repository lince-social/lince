import { NextResponse } from "next/server";
import { prisma } from "@lib/prisma";

export async function GET(request: Request) {
  try {
    const url = new URL(request.url);
    const onlyActive = url.searchParams.get("active");
    const onlyViews = url.searchParams.get("views");
    const onlyStyle = url.searchParams.get("style");

    const configResponse = onlyActive
      ? await prisma.configuration.findFirst({ where: { quantity: 1 } })
      : await prisma.configuration.findMany({ orderBy: { quantity: "desc" } });

    if (!configResponse) {
      return NextResponse.json(
        { success: false, error: "No configuration found" },
        { status: 404 },
      );
    }

    if (onlyViews) {
      if (Array.isArray(configResponse)) {
        return NextResponse.json(
          {
            success: false,
            error: "Cannot fetch views for multiple configurations",
          },
          { status: 400 },
        );
      }
      return NextResponse.json(configResponse.views);
    }

    return NextResponse.json(configResponse);
  } catch (error) {
    console.error("Error in API:", error);
    return NextResponse.json(
      { success: false, error: "Internal Server Error" },
      { status: 500 },
    );
  }
}
