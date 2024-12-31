import { NextResponse } from "next/server";
import { prisma } from "@lib/prisma";

export async function GET() {
  try {
    const configurations = await prisma.configuration.findMany({
      orderBy: { quantity: "desc" },
    });
    return NextResponse.json(configurations);
  } catch (error) {
    console.error("Error in API:", error);
    return NextResponse.json(
      { success: false, error: "Internal Server Error" },
      { status: 500 },
    );
  }
}
