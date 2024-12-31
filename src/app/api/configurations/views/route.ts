import { NextResponse } from "next/server";
import { prisma } from "@lib/prisma";

export async function POST(request: Request) {
  try {
    const body = await request.json();
    const updatedViews = body.updatedViews;

    if (!updatedViews) {
      return NextResponse.json(
        { error: "Updating requires views." },
        { status: 400 },
      );
    }

    const updatedConfig = await prisma.configuration.updateMany({
      where: { quantity: 1 },
      data: {
        views: updatedViews,
      },
    });

    return NextResponse.json({
      message: "Views updated successfully.",
      updatedConfig,
    });
  } catch (error) {
    console.log("Error updating quantities:", error);
    return NextResponse.json(
      { error: "Error updating quantities." },
      { status: 500 },
    );
  }
}
