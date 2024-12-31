import { NextResponse } from "next/server";
import { prisma } from "@lib/prisma";

export async function POST(request: Request) {
  try {
    const body = await request.json();
    const updateQuantityId = body.id;

    if (!updateQuantityId) {
      return NextResponse.json(
        { error: "Updating requires an ID." },
        { status: 400 },
      );
    }

    await prisma.configuration.updateMany({
      data: { quantity: 0 },
    });

    await prisma.configuration.update({
      where: { id: Number(updateQuantityId) },
      data: { quantity: 1 },
    });

    return NextResponse.json({
      message: "Quantity updated successfully.",
    });
  } catch (error) {
    console.log("Error updating quantities:", error);
    return NextResponse.json(
      { error: "Error updating quantities." },
      { status: 500 },
    );
  }
}
