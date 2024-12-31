import { NextResponse } from "next/server";
import { prisma } from "@lib/prisma";

export async function GET(request) {
    try {
        const { searchParams } = new URL(request.url);
        const viewQuery = searchParams.get("viewQuery");

        if (!viewQuery) {
            return NextResponse.json(
                { success: false, error: "Missing viewQuery parameter" },
                { status: 400 },
            );
        }

        const data = await prisma.$queryRawUnsafe(viewQuery);
        return NextResponse.json(data);
    } catch (error) {
        console.error("Error in API:", error);
        return NextResponse.json(
            { success: false, error: "Internal Server Error" },
            { status: 500 },
        );
    }
}
