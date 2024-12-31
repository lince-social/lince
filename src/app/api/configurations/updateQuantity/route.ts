import { NextResponse } from "next/server";
import { prisma } from "@lib/prisma";

export async function POST(request: Request) {
  try {
    const { id } = await request.json();

    if (!id) {
      return NextResponse.json({ error: "ID is required." }, { status: 400 });
    }

    // Reset all quantities to 0
    await prisma.configuration.updateMany({
      data: { quantity: 0 },
    });

    // Set the selected configuration's quantity to 1
    await prisma.configuration.update({
      where: { id },
      data: { quantity: 1 },
    });

    return NextResponse.json({ message: "Quantity updated successfully." });
  } catch (error) {
    console.log("Error updating quantities:", error);
    return NextResponse.json(
      { error: "Error updating quantities." },
      { status: 500 },
    );
  }
}
