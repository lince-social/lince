import { NextResponse } from "next/server";

export async function GET() {
    try {
        return NextResponse.json({ test: "Hello, world", test2: "Hello, again" });
    } catch (error) {
        console.error("Error in API:", error);
        return NextResponse.json(
            { success: false, error: "Internal Server Error" },
            { status: 500 },
        );
    }
}
