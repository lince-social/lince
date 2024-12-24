import { NextResponse } from "next/server";
import pool from "@/db/pool";

//export async function GET() {
//    return NextResponse.json({ message: "i love beans" });
//}

export async function GET() {
    try {
        // Example query: Replace with your actual query
        const client = await pool.connect();
        //const result = await client.query("SELECT now() AS timestamp");
        //client.release();

        //return NextResponse.json({ success: true, data: result.rows });
        return NextResponse.json({ message: "absuabiub" });
    } catch (error) {
        console.error("Error in API:", error);
        return NextResponse.json(
            { success: false, error: "Internal Server Error" },
            { status: 500 },
        );
    }
}
